//! Event outbox. Hooks and tools append events (`capture`, `promote`, `sync`, `remove`) to
//! `<git-common-dir>/kontext/outbox.jsonl`; `kontext outbox flush` delivers them to the adapters
//! subscribed in config and keeps whatever failed for the next flush. Git hooks never wait on
//! adapters: they queue and spawn a detached flush. Nothing is dropped silently: an attempt counts
//! only for the adapter actually tried, an event for an adapter skipped by `when` stays queued, and
//! an event that failed [`MAX_ATTEMPTS`] times moves to `outbox.dead.jsonl`, shown by
//! `kontext outbox` and `kontext status` until `kontext outbox retry` queues it again.
use crate::adapters::Registry;
use crate::config::{Config, OnCfg};
use crate::repo::Repo;
use crate::store::Entry;
use crate::util;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event: String,
    pub at: String,
    pub payload: Value,
    /// The most attempts any one `adapter:op` made.
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub delivered: Vec<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Failed attempts per `adapter:op`; only an adapter actually tried counts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tries: BTreeMap<String, u32>,
    /// When it moved to the dead letters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_at: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct FlushReport {
    pub delivered: Vec<String>,
    pub failed: Vec<String>,
    pub remaining: usize,
    /// Events that ran out of attempts in this flush and moved to the dead letters.
    pub dead: Vec<String>,
}

pub struct Outbox {
    path: PathBuf,
    dead: PathBuf,
    lock: PathBuf,
}

/// Failed attempts of one `adapter:op` before its event moves to the dead letters.
pub const MAX_ATTEMPTS: u32 = 8;

pub fn entry_payload(repo: &Repo, e: &Entry, visibility: &str) -> Value {
    json!({
        "id": e.id,
        "kind": e.kind,
        "title": e.title,
        "status": e.status,
        "date": e.date,
        "summary": e.summary_text(300),
        "tags": e.tags,
        "paths": e.paths,
        "body": e.body,
        "markdown": e.markdown(),
        "path": e.rel_path,
        "visibility": visibility,
        "source": {"repo": repo.id, "branch": repo.branch},
    })
}

/// What a `remove` event carries: the entry that was at `path` (its id and kind when the old
/// version could be read), so a mirror can delete its copy.
pub fn removal_payload(repo: &Repo, path: &str, old: Option<&Entry>) -> Value {
    json!({
        "id": old.map(|e| e.id.as_str()),
        "kind": old.map(|e| e.kind.as_str()),
        "title": old.map(|e| e.title.as_str()),
        "path": path,
        "visibility": "team",
        "source": {"repo": repo.id, "branch": repo.branch},
    })
}

/// Whether any of `on` takes this event.
fn subscribes(on: &[OnCfg], event: &str, visibility: Option<&str>, kind: Option<&str>) -> bool {
    on.iter().any(|o| {
        o.event == event
            && o.visibility.as_deref().is_none_or(|v| Some(v) == visibility)
            && (o.kinds.is_empty() || kind.is_some_and(|k| o.kinds.iter().any(|x| x == k)))
    })
}

impl Outbox {
    pub fn open(repo: &Repo) -> Outbox {
        let dir = repo.state_dir();
        Outbox { path: dir.join("outbox.jsonl"), dead: dir.join("outbox.dead.jsonl"), lock: dir.join("outbox.lock") }
    }

    pub fn push(&self, event: &str, payload: Value) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let ev = Event {
            id: format!("{}-{}", chrono::Local::now().format("%Y%m%d%H%M%S%3f"), util::short_hash(&payload.to_string())),
            event: event.to_string(),
            at: util::now_iso(),
            payload,
            attempts: 0,
            delivered: Vec::new(),
            last_error: None,
            tries: BTreeMap::new(),
            dead_at: None,
        };
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&ev)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Event> {
        read_events(&self.path)
    }

    /// Events that ran out of attempts, oldest first.
    pub fn dead(&self) -> Vec<Event> {
        read_events(&self.dead)
    }

    /// Queue every dead event again with fresh attempts (what it delivered already stays
    /// delivered). Returns how many.
    pub fn retry(&self) -> Result<usize> {
        let Some(_guard) = LockFile::acquire(&self.lock) else { bail!("a flush is running; try again when it is done") };
        let dead = self.dead();
        if dead.is_empty() {
            return Ok(0);
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        for mut e in dead.iter().cloned() {
            e.tries.clear();
            e.attempts = 0;
            e.dead_at = None;
            writeln!(f, "{}", serde_json::to_string(&e)?)?;
        }
        std::fs::remove_file(&self.dead)?;
        Ok(dead.len())
    }

    /// [`Outbox::flush_with`] without the configuration: an event for an adapter skipped by `when`
    /// cannot be told apart and is treated as done. Kept for callers that have no `Config`.
    #[allow(dead_code)]
    pub fn flush(&self, reg: &Registry) -> Result<FlushReport> {
        self.flush_with(reg, None)
    }

    /// Deliver queued events. Only one flush runs at a time per clone. An adapter that fails is not tried again in this flush, and its
    /// remaining events lose no attempt; an event waiting on an adapter that `cfg` declares but
    /// `when` skipped stays queued; one that ran out of attempts moves to the dead letters.
    pub fn flush_with(&self, reg: &Registry, cfg: Option<&Config>) -> Result<FlushReport> {
        let mut report = FlushReport::default();
        let _guard = match LockFile::acquire(&self.lock) {
            Some(g) => g,
            None => {
                report.remaining = self.list().len();
                return Ok(report);
            }
        };
        // Take the queue atomically; hooks keep appending to a fresh file meanwhile.
        let processing = self.path.with_extension("processing");
        let mut events: Vec<Event> = read_events(&processing);
        if self.path.exists() {
            let taken = self.path.with_extension(format!("taken{}", std::process::id()));
            std::fs::rename(&self.path, &taken)?;
            events.extend(read_events(&taken));
            let text: String = events.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|l| l + "\n").collect();
            util::write_atomic(&processing, &text)?;
            let _ = std::fs::remove_file(&taken);
        }
        // adapters skipped by `when` (not disabled ones): their subscriptions keep events queued
        let waiting: Vec<&[OnCfg]> = cfg
            .map(|cfg| {
                reg.skipped
                    .iter()
                    .filter(|(_, why)| why != "disabled")
                    .filter_map(|(name, _)| cfg.adapters.get(name))
                    .filter(|a| a.enabled != Some(false))
                    .map(|a| a.on.as_slice())
                    .collect()
            })
            .unwrap_or_default();
        let mut down: HashSet<String> = HashSet::new();
        let mut keep = Vec::new();
        let mut dead = Vec::new();
        for mut ev in events {
            let visibility = ev.payload.get("visibility").and_then(|v| v.as_str()).map(str::to_string);
            let kind = ev.payload.get("kind").and_then(|v| v.as_str()).map(str::to_string);
            let mut pending = false;
            let mut exhausted = false;
            for a in &reg.adapters {
                for sub in a.subscriptions(&ev.event, visibility.as_deref(), kind.as_deref()) {
                    let key = format!("{}:{}", a.name, sub.op);
                    if ev.delivered.contains(&key) {
                        continue;
                    }
                    if down.contains(&a.name) {
                        // not tried: it failed earlier in this flush
                        pending = true;
                        continue;
                    }
                    match a.deliver(&sub.op, &ev.payload) {
                        Ok(_) => {
                            ev.delivered.push(key.clone());
                            report.delivered.push(format!(
                                "{} {} → {key}",
                                ev.event,
                                ev.payload.get("id").and_then(|v| v.as_str()).unwrap_or("")
                            ));
                        }
                        Err(e) => {
                            down.insert(a.name.clone());
                            let msg = crate::secrets::scrub_error(&format!("{key}: {e:#}"));
                            report.failed.push(msg.clone());
                            let n = ev.tries.entry(key).or_insert(0);
                            *n += 1;
                            exhausted |= *n >= MAX_ATTEMPTS;
                            ev.last_error = Some(util::truncate_chars(&msg, 300));
                            pending = true;
                        }
                    }
                }
            }
            if waiting.iter().any(|on| subscribes(on, &ev.event, visibility.as_deref(), kind.as_deref())) {
                pending = true;
            }
            ev.attempts = ev.tries.values().copied().max().unwrap_or(0);
            if exhausted {
                ev.dead_at = Some(util::now_iso());
                report.dead.push(format!("{} {}", ev.event, ev.payload.get("id").and_then(|v| v.as_str()).unwrap_or("")));
                dead.push(ev);
            } else if pending {
                keep.push(ev);
            }
        }
        report.remaining = keep.len();
        append_events(&self.path, &keep)?;
        append_events(&self.dead, &dead)?;
        let _ = std::fs::remove_file(&processing);
        Ok(report)
    }
}

fn append_events(path: &Path, events: &[Event]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    for e in events {
        writeln!(f, "{}", serde_json::to_string(e)?)?;
    }
    Ok(())
}

fn read_events(path: &Path) -> Vec<Event> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Keep a log under `max_bytes`: past it, the file moves to `<name>.1` (replacing an older one) and
/// the next write starts a fresh file. Returns whether it rotated.
#[allow(dead_code)] // wired by the integrator
pub fn rotate_log(path: &Path, max_bytes: u64) -> std::io::Result<bool> {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > max_bytes => {
            let mut old = path.as_os_str().to_owned();
            old.push(".1");
            std::fs::rename(path, PathBuf::from(old))?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

struct LockFile(PathBuf);

impl LockFile {
    fn acquire(path: &PathBuf) -> Option<LockFile> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // stale locks (crashed flush) expire after 10 minutes
        if let Ok(md) = std::fs::metadata(path)
            && md.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age.as_secs() > 600)
        {
            let _ = std::fs::remove_file(path);
        }
        std::fs::OpenOptions::new().write(true).create_new(true).open(path).ok().map(|_| LockFile(path.clone()))
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Start `kontext outbox flush` detached so git hooks return immediately.
pub fn spawn_background_flush(repo: &Repo) {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::process::Command::new(exe)
        .arg("-C")
        .arg(&repo.root)
        .args(["outbox", "flush", "--quiet"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_rotates_past_its_size() {
        let dir = std::env::temp_dir().join(format!("kontext-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("adapter.log");
        assert!(!rotate_log(&log, 10).unwrap(), "no file, nothing to do");
        std::fs::write(&log, "short").unwrap();
        assert!(!rotate_log(&log, 10).unwrap());
        std::fs::write(&log, "a much longer line").unwrap();
        assert!(rotate_log(&log, 10).unwrap());
        assert!(!log.exists());
        assert_eq!(std::fs::read_to_string(dir.join("adapter.log.1")).unwrap(), "a much longer line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subscriptions_match_event_visibility_and_kind() {
        let on = vec![OnCfg { event: "sync".into(), op: "store".into(), visibility: Some("team".into()), kinds: vec!["decision".into()] }];
        assert!(subscribes(&on, "sync", Some("team"), Some("decision")));
        assert!(!subscribes(&on, "sync", Some("private"), Some("decision")));
        assert!(!subscribes(&on, "sync", Some("team"), Some("learning")));
        assert!(!subscribes(&on, "capture", Some("team"), Some("decision")));
    }
}
