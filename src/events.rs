//! Event outbox. Hooks and tools append events (`capture`, `promote`, `sync`) to
//! `<git-common-dir>/kontext/outbox.jsonl`; `kontext outbox flush` delivers them to the adapters
//! subscribed in config and keeps whatever failed for the next flush. Git hooks never wait on
//! adapters: they queue and spawn a detached flush.
use crate::adapters::Registry;
use crate::repo::Repo;
use crate::store::Entry;
use crate::util;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event: String,
    pub at: String,
    pub payload: Value,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub delivered: Vec<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct FlushReport {
    pub delivered: Vec<String>,
    pub failed: Vec<String>,
    pub remaining: usize,
}

pub struct Outbox {
    path: PathBuf,
    lock: PathBuf,
}

const MAX_ATTEMPTS: u32 = 8;

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

impl Outbox {
    pub fn open(repo: &Repo) -> Outbox {
        let dir = repo.state_dir();
        Outbox { path: dir.join("outbox.jsonl"), lock: dir.join("outbox.lock") }
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
        };
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&ev)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Event> {
        std::fs::read_to_string(&self.path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    /// Deliver queued events. Only one flush runs at a time per clone.
    pub fn flush(&self, reg: &Registry) -> Result<FlushReport> {
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
        let mut keep = Vec::new();
        for mut ev in events {
            let visibility = ev.payload.get("visibility").and_then(|v| v.as_str()).map(str::to_string);
            let kind = ev.payload.get("kind").and_then(|v| v.as_str()).map(str::to_string);
            let mut pending_error = None;
            for a in &reg.adapters {
                for sub in a.subscriptions(&ev.event, visibility.as_deref(), kind.as_deref()) {
                    let key = format!("{}:{}", a.name, sub.op);
                    if ev.delivered.contains(&key) {
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
                            let msg = format!("{key}: {e:#}");
                            report.failed.push(msg.clone());
                            pending_error = Some(msg);
                        }
                    }
                }
            }
            if let Some(err) = pending_error {
                ev.attempts += 1;
                ev.last_error = Some(util::truncate_chars(&err, 300));
                if ev.attempts < MAX_ATTEMPTS {
                    keep.push(ev);
                }
            }
        }
        report.remaining = keep.len();
        if !keep.is_empty() {
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
            for e in &keep {
                writeln!(f, "{}", serde_json::to_string(e)?)?;
            }
        }
        let _ = std::fs::remove_file(&processing);
        Ok(report)
    }
}

fn read_events(path: &std::path::Path) -> Vec<Event> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
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
