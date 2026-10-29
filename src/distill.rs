//! Distill team knowledge from agent threads through an `llm` adapter. Each thread is read into a
//! compact, redacted transcript and split into parts; the model names the durable decisions,
//! conventions, pitfalls and incidents it finds; they land in the local inbox, to be reviewed and
//! promoted with a commit like anything else captured.
use crate::app::App;
use crate::inbox::Inbox;
use crate::ops::{self, CaptureReq};
use crate::threads::{self, Found};
use crate::util;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Transcript characters per model call.
const PART_CHARS: usize = 45_000;

#[derive(Debug, Deserialize, Default)]
struct Answer {
    #[serde(default)]
    entries: Vec<Candidate>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Candidate {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default, deserialize_with = "crate::init::deepen::string_or_vec")]
    pub paths: Vec<String>,
    #[serde(default, deserialize_with = "crate::init::deepen::string_or_vec")]
    pub commits: Vec<String>,
    #[serde(default, deserialize_with = "crate::init::deepen::string_or_vec")]
    pub tags: Vec<String>,
}

pub struct Options {
    pub llm: Option<String>,
    /// Most entries kept per thread.
    pub max_per_thread: usize,
    /// Most transcript parts read per thread (evenly spread over a longer thread).
    pub max_parts: usize,
    pub jobs: usize,
    /// Print instead of capturing.
    pub dry_run: bool,
}

pub struct Outcome {
    pub thread: String,
    pub captured: Vec<(Candidate, String)>,
    pub notes: Vec<String>,
}

const PROMPT: &str = "You are distilling durable team knowledge for the repository {repo} from part {part} of {parts} of an agent thread \
(a developer working with a coding agent). Record only what the team should still know months from now:\n\
- decision: a direction taken or an alternative rejected, and why;\n\
- convention: a rule for how things are done here;\n\
- learning: a non-obvious pitfall or gotcha that cost time;\n\
- incident: something that broke, why, and the fix.\n\
Skip routine progress, debugging chatter, one-off facts, plans that were not carried out, and anything already recorded (titles below). \
Prefer zero to three strong entries over many weak ones; an empty list is a valid answer. \
Write for the whole team and keep people and data out: roles instead of names (\"the support team\", \"the client's PM\"), \
and no email addresses, phone numbers, guest or customer records, credentials, hostnames or IP addresses.\n\n\
Already recorded:\n{known}\n\n\
{thread}\n<<<\n{text}\n>>>\n\n\
Answer with ONLY one JSON object, no prose and no code fence, and do not call any tools:\n\
{\"entries\": [{\"kind\": \"decision|convention|learning|incident\", \"title\": \"a short statement\", \
\"body\": \"Markdown, a few lines: what, why, consequences; for a decision with rejected options, a ## Alternatives section saying why each was rejected\", \"paths\": [\"repository-relative paths or globs it governs\"], \
\"commits\": [\"short shas the thread committed it in, if any\"]}]}";

fn pick_llm(app: &App, explicit: Option<&str>) -> Result<String> {
    if let Some(n) = explicit.or(app.cfg().init.llm.as_deref()) {
        return Ok(n.to_string());
    }
    let reg = app.registry();
    let llms: Vec<&str> = reg.adapters.iter().filter(|a| a.has_op("llm")).map(|a| a.name.as_str()).collect();
    match llms.as_slice() {
        [one] => Ok(one.to_string()),
        [] => bail!("no adapter with an `llm` op — add one, e.g. `kontext adapters add llm-codex`"),
        many => bail!("choose the model with --llm ({})", many.join(", ")),
    }
}

/// Invoke a configured LLM without capturing or staging anything. Both directions are redacted.
pub fn generate_json(app: &App, explicit: Option<&str>, prompt: &str) -> Result<serde_json::Value> {
    // one code for "no model": `model_unavailable` (older binaries said `llm_unavailable`)
    let name = pick_llm(app, explicit).map_err(|e| crate::api::error_with_alias("model_unavailable", "llm_unavailable", e.to_string()))?;
    let adapter =
        app.registry().get(&name).filter(|a| a.has_op("llm")).ok_or_else(|| {
            crate::api::error_with_alias("model_unavailable", "llm_unavailable", format!("no active llm adapter '{name}'"))
        })?;
    // nothing leaves the machine while a redaction pattern does not compile
    crate::secrets::redaction(&app.loaded)?;
    let output = adapter.llm(&mask_text(app, prompt))?;
    crate::init::deepen::extract_json_object(&mask_text(app, &output))
        .ok_or_else(|| crate::api::error("invalid_model_output", "LLM response must contain a JSON object"))
}

/// `text` with secrets and personal data masked, by the built-in rules and the `secrets.redact`
/// patterns of every config layer (private and repository). A pattern that does not compile is
/// skipped here; whatever sends text to a model checks [`crate::secrets::redaction`] first and
/// refuses.
pub fn mask_text(app: &App, text: &str) -> String {
    crate::secrets::redact_personal(&crate::secrets::redact(text).0, &crate::secrets::patterns(&crate::secrets::redact_list(&app.loaded))).0
}

/// Extract from arbitrary text. Caller controls the source URI; no thread-provider assumptions.
/// Unlike `distill`, this operation never writes candidates or deduplicates a selected finding.
pub fn extract_text(app: &App, text: &str, source: Option<&str>, llm: Option<&str>, max: usize) -> Result<Vec<Candidate>> {
    if text.len() > 2_000_000 {
        return Err(crate::api::error("invalid_input", "text exceeds 2 MB; submit smaller documents"));
    }
    crate::secrets::redaction(&app.loaded)?;
    let titles: Vec<String> = app.entries().0.iter().filter(|e| e.is_active()).map(|e| e.title.clone()).collect();
    let parts = threads::parts(&mask_text(app, text), PART_CHARS);
    let mut candidates: Vec<Candidate> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let known: String = known_titles(&titles, part);
        let prompt = PROMPT
            .replace("{repo}", &threads::prompt_repo_name(&app.repo.id))
            .replace("{part}", &(i + 1).to_string())
            .replace("{parts}", &parts.len().to_string())
            .replace("{known}", &known)
            .replace("{thread}", source.unwrap_or("provided text"))
            .replace("{text}", part);
        let answer: Answer = serde_json::from_value(generate_json(app, llm, &prompt)?)?;
        for mut c in answer.entries {
            let Some((kind, implied_tag)) = ops::normalize_kind(app, &c.kind) else { continue };
            if c.title.trim().is_empty() || c.body.trim().is_empty() {
                continue;
            }
            c.kind = kind;
            if let Some(tag) = implied_tag {
                c.tags.push(tag.to_string());
            }
            c.title = mask_text(app, &c.title);
            c.body = mask_text(app, &c.body);
            // Future files and globs are meaningful metadata; reject traversal, not absence on disk.
            c.paths.retain(|p| {
                !p.is_empty()
                    && !std::path::Path::new(p).is_absolute()
                    && !std::path::Path::new(p).components().any(|c| matches!(c, std::path::Component::ParentDir))
            });
            if !candidates.iter().any(|kept| ops::title_similarity(&kept.title, &c.title) >= 0.6) {
                candidates.push(c);
            }
        }
        if candidates.len() >= max {
            break;
        }
    }
    candidates.truncate(max);
    Ok(candidates)
}

/// Titles a prompt names as already recorded: those of `titles` closest to `text`, not the whole
/// index.
fn known_titles(titles: &[String], text: &str) -> String {
    let near = threads::relevant_titles(titles, text, KNOWN_TITLES);
    if near.is_empty() { "(nothing close)\n".to_string() } else { near.iter().map(|t| format!("- {t}\n")).collect() }
}

/// How many recorded titles a prompt lists.
const KNOWN_TITLES: usize = 40;

/// Up to `max` parts, spread evenly over the whole thread when it has more.
fn spread(parts: Vec<String>, max: usize) -> (Vec<(usize, String)>, usize) {
    let total = parts.len();
    if total <= max {
        return (parts.into_iter().enumerate().map(|(i, p)| (i + 1, p)).collect(), total);
    }
    let pick: HashSet<usize> = (0..max).map(|k| k * (total - 1) / (max - 1).max(1)).collect();
    (parts.into_iter().enumerate().filter(|(i, _)| pick.contains(i)).map(|(i, p)| (i + 1, p)).collect(), total)
}

/// Repository-relative paths that exist (or globs); absolute ones inside the repository are made relative.
fn clean_paths(app: &App, cwd: Option<&str>, paths: &[String]) -> Vec<String> {
    let root = app.repo.root.to_string_lossy().to_string();
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let mut p = p.trim().trim_start_matches("./").to_string();
        for prefix in [cwd.unwrap_or(""), root.as_str()] {
            if !prefix.is_empty()
                && let Some(rest) = p.strip_prefix(&format!("{}/", prefix.trim_end_matches('/')))
            {
                p = rest.to_string();
            }
        }
        let glob = p.contains(['*', '?', '[']);
        if p.is_empty() || p.starts_with('/') || p.contains("..") || (!glob && !app.repo.abs(&p).exists()) {
            continue;
        }
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out.truncate(8);
    out
}

pub fn distill(app: &App, found: &[Found], opt: &Options, report: &dyn Fn(&str)) -> Result<Vec<Outcome>> {
    if !app.is_set_up() && !opt.dry_run {
        bail!("kontext is not set up in this repository (no knowledge store) — run `kontext init` first, or use --dry-run");
    }
    let llm = pick_llm(app, opt.llm.as_deref())?;
    let adapter = app.registry().get(&llm).ok_or_else(|| anyhow!("no active adapter '{llm}'"))?;
    if !adapter.has_op("llm") {
        bail!("adapter '{llm}' has no `llm` op");
    }
    // fails closed: a pattern that does not compile stops everything before a thread is read
    let extra = threads::redaction(&app.loaded)?;
    // what the model writes is masked again: it may quote the thread
    let mask = |s: &str| crate::secrets::redact_personal(&crate::secrets::redact(s).0, &extra).0;
    let (entries, _) = app.entries();
    let mut known: Vec<String> = entries
        .iter()
        .filter(|e| matches!(e.kind.as_str(), "decision" | "convention" | "learning" | "incident") && e.is_active())
        .map(|e| e.title.clone())
        .collect();
    known.extend(Inbox::open(&app.repo).list().into_iter().map(|e| e.title));
    // what someone dismissed from the inbox is not proposed again
    known.extend(threads::dismissed_titles(&app.repo));
    let mut distilled = threads::Distilled::load(&app.repo);

    let mut outcomes = Vec::new();
    for f in found {
        let t = match threads::load(f) {
            Ok(t) => t,
            Err(e) => {
                report(&format!("  {} ✗ {e:#}", f.label()));
                continue;
            }
        };
        let label = match &f.workspace {
            Some(ws) => format!("superset:{}/{}", threads::short(ws), f.label()),
            None => f.label(),
        };
        let mut out = Outcome { thread: label.clone(), captured: Vec::new(), notes: Vec::new() };
        if !t.turns.iter().any(|x| x.role == threads::Role::User) {
            report(&format!("  {label} · nothing said in it"));
            outcomes.push(out);
            continue;
        }
        // what was distilled before is not sent to a model again
        let key = threads::distilled_key(f);
        let size = std::fs::metadata(&f.path).map(|m| m.len()).unwrap_or(0);
        if !opt.dry_run && size > 0 && distilled.unchanged(&key, size) {
            report(&format!("  {label} · distilled before, nothing new"));
            outcomes.push(out);
            continue;
        }
        let text = threads::render(&t, &app.repo.root, &extra);
        let head = text.lines().next().unwrap_or("").to_string();
        let (parts, total) = spread(threads::parts(&text, PART_CHARS), opt.max_parts.max(1));
        if parts.len() < total {
            out.notes.push(format!("read {} of {total} parts (raise --max-parts to read all)", parts.len()));
        }
        let seen = parts.iter().filter(|(_, p)| distilled.seen(&key, p)).count();
        let parts: Vec<(usize, String)> =
            if opt.dry_run { parts } else { parts.into_iter().filter(|(_, p)| !distilled.seen(&key, p)).collect() };
        if seen > 0 && !opt.dry_run {
            out.notes.push(format!("{seen} part(s) distilled before, skipped"));
        }
        let repo_name = threads::prompt_repo_name(&app.repo.id);
        let prompts: Vec<(usize, String)> = parts
            .iter()
            .map(|(i, p)| {
                let known_list = known_titles(&known, p);
                let prompt = PROMPT
                    .replace("{repo}", &repo_name)
                    .replace("{part}", &i.to_string())
                    .replace("{parts}", &total.to_string())
                    .replace("{known}", if known.is_empty() { "(nothing yet)\n" } else { &known_list })
                    .replace("{thread}", &head)
                    .replace("{text}", p);
                (*i, prompt)
            })
            .collect();
        let started = std::time::Instant::now();
        let mut answers: Vec<(usize, Result<String>)> = Vec::new();
        for batch in prompts.chunks(opt.jobs.clamp(1, 8)) {
            let got: Vec<(usize, Result<String>)> = std::thread::scope(|s| {
                let hs: Vec<_> = batch
                    .iter()
                    .map(|(i, prompt)| {
                        let a = adapter.clone();
                        s.spawn(move || (*i, a.llm(prompt)))
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap_or((0, Err(anyhow!("worker panicked"))))).collect()
            });
            answers.extend(got);
        }
        let mut cands: Vec<Candidate> = Vec::new();
        let mut done: Vec<String> = Vec::new();
        for (i, r) in answers {
            if r.is_ok()
                && let Some((_, p)) = parts.iter().find(|(n, _)| *n == i)
            {
                done.push(p.clone());
            }
            match r.and_then(|text| {
                let v = crate::init::deepen::extract_json_object(&text)
                    .ok_or_else(|| anyhow!("answer was not JSON: {}", util::truncate_chars(&util::one_line(&text), 120)))?;
                Ok(serde_json::from_value::<Answer>(v)?)
            }) {
                Ok(a) => cands.extend(a.entries),
                Err(e) => out.notes.push(format!("part {i}: {}", util::truncate_chars(&format!("{e:#}"), 160))),
            }
        }
        // one entry per finding: parts overlap in what they see, and some of it is recorded already
        let mut kept: Vec<Candidate> = Vec::new();
        for c in cands {
            let kind = match c.kind.trim().to_lowercase().as_str() {
                "pitfall" | "gotcha" => "learning".to_string(),
                k @ ("decision" | "convention" | "learning" | "incident") => k.to_string(),
                _ => continue,
            };
            if c.title.trim().is_empty() || c.body.trim().is_empty() {
                continue;
            }
            let dup = |t: &str| ops::title_similarity(t, &c.title) >= 0.6;
            if known.iter().any(|k| dup(k)) || kept.iter().any(|k| dup(&k.title)) {
                continue;
            }
            kept.push(Candidate { kind, title: mask(&c.title), body: mask(&c.body), ..c });
        }
        kept.truncate(opt.max_per_thread);
        for c in kept {
            let paths = clean_paths(app, t.cwd.as_deref(), &c.paths);
            if opt.dry_run {
                out.captured.push((Candidate { paths, ..c.clone() }, String::new()));
                known.push(c.title.clone());
                continue;
            }
            let req = CaptureReq {
                kind: c.kind.clone(),
                title: c.title.clone(),
                body: c.body.clone(),
                paths: paths.clone(),
                visibility: Some("team".into()),
                origin: "thread".into(),
                source: Some(label.clone()),
                commits: c.commits.clone(),
                ..Default::default()
            };
            match ops::capture(app, req) {
                Ok(o) => {
                    known.push(c.title.clone());
                    out.captured.push((Candidate { paths, ..c }, o.location));
                }
                Err(e) => out.notes.push(format!("{}: {e:#}", c.title)),
            }
        }
        if !opt.dry_run && !done.is_empty() {
            distilled.record(&key, size, &done);
            if let Err(e) = distilled.save(&app.repo) {
                out.notes.push(format!("could not record what was distilled: {e:#}"));
            }
        }
        report(&format!(
            "  {label} · {} part(s), {:.0}s · {} entr{}{}",
            parts.len(),
            started.elapsed().as_secs_f32(),
            out.captured.len(),
            if out.captured.len() == 1 { "y" } else { "ies" },
            if out.notes.is_empty() { String::new() } else { format!(" · {}", out.notes.join("; ")) }
        ));
        outcomes.push(out);
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_parts_over_long_threads() {
        let parts: Vec<String> = (1..=10).map(|i| i.to_string()).collect();
        let (kept, total) = spread(parts, 4);
        assert_eq!(total, 10);
        let idx: Vec<usize> = kept.iter().map(|(i, _)| *i).collect();
        assert_eq!(idx.first(), Some(&1));
        assert_eq!(idx.last(), Some(&10));
        assert_eq!(idx.len(), 4);
    }

    #[test]
    fn lenient_answers() {
        let v = serde_json::json!({"entries": [{"kind": "decision", "title": "T", "body": "B", "paths": "a/b, c", "commits": "abc1234"}]});
        let a: Answer = serde_json::from_value(v).unwrap();
        assert_eq!(a.entries[0].paths, vec!["a/b", "c"]);
        assert_eq!(a.entries[0].commits, vec!["abc1234"]);
    }

    #[test]
    fn distill_refuses_invalid_redact_pattern() {
        let dir = std::env::temp_dir().join(format!("kontext-distill-redact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ai")).unwrap();
        std::fs::write(dir.join(".ai/kontext.toml"), "[secrets]\nredact = ['\\bAcme Corp(\\b']\n").unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"]).arg(&dir).status().unwrap().success());
        let app = App::open(&dir).unwrap();
        // nothing is sent to a model while a pattern does not compile, and every report says why
        let err = extract_text(&app, "Acme Corp signed off on the queue.", None, None, 3).unwrap_err();
        assert!(format!("{err:#}").contains("does not compile"), "{err:#}");
        assert!(app.warnings().iter().any(|w| w.contains("secrets.redact pattern")), "{:?}", app.warnings());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
