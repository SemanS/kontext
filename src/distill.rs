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
use serde::Deserialize;
use std::collections::HashSet;

/// Transcript characters per model call.
const PART_CHARS: usize = 45_000;

#[derive(Debug, Deserialize, Default)]
struct Answer {
    #[serde(default)]
    entries: Vec<Candidate>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Candidate {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default, deserialize_with = "crate::init::deepen::string_or_vec")]
    pub paths: Vec<String>,
    #[serde(default, deserialize_with = "crate::init::deepen::string_or_vec")]
    pub commits: Vec<String>,
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
Prefer zero to three strong entries over many weak ones; an empty list is a valid answer.\n\n\
Already recorded:\n{known}\n\n\
{thread}\n<<<\n{text}\n>>>\n\n\
Answer with ONLY one JSON object, no prose and no code fence, and do not call any tools:\n\
{\"entries\": [{\"kind\": \"decision|convention|learning|incident\", \"title\": \"a short statement\", \
\"body\": \"Markdown, a few lines: what, why, consequences\", \"paths\": [\"repository-relative paths or globs it governs\"], \
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
    let (entries, _) = app.entries();
    let mut known: Vec<String> = entries
        .iter()
        .filter(|e| matches!(e.kind.as_str(), "decision" | "convention" | "learning" | "incident") && e.is_active())
        .map(|e| e.title.clone())
        .collect();
    known.extend(Inbox::open(&app.repo).list().into_iter().map(|e| e.title));

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
        let text = threads::render(&t, &app.repo.root);
        let head = text.lines().next().unwrap_or("").to_string();
        let (parts, total) = spread(threads::parts(&text, PART_CHARS), opt.max_parts.max(1));
        if parts.len() < total {
            out.notes.push(format!("read {} of {total} parts (raise --max-parts to read all)", parts.len()));
        }
        let known_list: String = known.iter().rev().take(200).map(|k| format!("- {k}\n")).collect();
        let prompts: Vec<(usize, String)> = parts
            .iter()
            .map(|(i, p)| {
                let prompt = PROMPT
                    .replace("{repo}", &app.repo.id)
                    .replace("{part}", &i.to_string())
                    .replace("{parts}", &total.to_string())
                    .replace("{known}", if known_list.is_empty() { "(nothing yet)\n" } else { &known_list })
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
        for (i, r) in answers {
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
            kept.push(Candidate { kind, ..c });
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
}
