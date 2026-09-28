//! Phase 5 — progressive, resumable deepening. Deterministic phases produce facts; semantic work
//! (what a module is *for*, which commits encode decisions) is handed out as small tasks to whichever
//! agent is connected (`ctx_init` / `ctx_init_submit`), or to an `llm` adapter in autopilot mode.
//! Progress lives in the store itself (`deepened` markers, decision `commits`), so any clone or
//! teammate can continue where the last one stopped.
use super::history::{Candidate, History};
use super::render::{FACTS_START, replace_section};
use super::scan::{Inventory, Module};
use super::{InitState, load_cached, save_state};
use crate::app::App;
use crate::ops;
use crate::secrets;
use crate::store::{Entry, FmValue, Store};
use crate::util;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt::Write as _;

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<(String, usize)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub docs: Vec<String>,
    #[serde(skip)]
    pub commits: Vec<Candidate>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub facts: String,
}

fn string_or_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(match v {
        Value::String(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Value::Array(a) => a.into_iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DecisionSub {
    pub title: String,
    pub summary: Option<String>,
    pub context: Option<String>,
    pub decision: String,
    pub consequences: Option<String>,
    #[serde(deserialize_with = "string_or_vec")]
    pub paths: Vec<String>,
    #[serde(deserialize_with = "string_or_vec")]
    pub commits: Vec<String>,
    pub date: Option<String>,
    pub status: Option<String>,
    #[serde(deserialize_with = "string_or_vec")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LearningSub {
    pub title: String,
    pub body: String,
    #[serde(deserialize_with = "string_or_vec")]
    pub paths: Vec<String>,
    #[serde(deserialize_with = "string_or_vec")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Submission {
    pub task_id: String,
    pub summary: Option<String>,
    pub overview: Option<String>,
    pub decisions: Vec<DecisionSub>,
    pub learnings: Vec<LearningSub>,
    pub skip: bool,
    pub reason: Option<String>,
}

pub struct Plan {
    pub pending: Vec<Task>,
    pub done: usize,
    pub total: usize,
}

fn module_by_doc<'a>(inv: &'a Inventory, e: &Entry) -> Option<&'a Module> {
    inv.modules.iter().find(|m| m.id == e.id)
}

pub fn plan(app: &App, inv: &Inventory, hist: &History, state: &InitState) -> Plan {
    let (entries, _) = app.entries();
    let mut first: Vec<Task> = Vec::new();
    let mut later: Vec<Task> = Vec::new();
    let mut refresh: Vec<Task> = Vec::new();
    let mut done = 0;
    let mut total = 0;

    if let Some(ov) = entries.iter().find(|e| e.kind == "architecture" && e.id == "overview") {
        total += 1;
        if ov.get_extra("deepened").is_some() || state.done.contains_key("purpose") {
            done += 1;
        } else {
            let mut docs: Vec<String> = ["README.md", "AGENTS.md", "CLAUDE.md", "Claude.md"]
                .iter()
                .filter(|f| app.repo.abs(f).is_file())
                .map(|s| s.to_string())
                .collect();
            docs.extend(inv.doc_dirs.iter().take(6).map(|d| format!("{d}/")));
            first.push(Task {
                id: "purpose".into(),
                kind: "purpose".into(),
                title: format!("Describe what {} is, for whom, and its main moving parts", app.project_name()),
                target: Some(ov.rel_path.clone()),
                files: Vec::new(),
                docs,
                commits: Vec::new(),
                facts: {
                    let mut ranked: Vec<&Module> = inv.modules.iter().collect();
                    ranked.sort_by_key(|m| m.rank);
                    format!(
                        "Stack: {}\nMain modules: {}",
                        ov.get_extra("stack").unwrap_or_default(),
                        ranked.iter().take(14).map(|m| m.path.clone()).collect::<Vec<_>>().join(", ")
                    )
                },
            });
        }
    }

    let mut mods: Vec<&Entry> = ops::module_docs(&entries);
    mods.sort_by_key(|e| e.get_extra("rank").and_then(|r| r.parse::<usize>().ok()).unwrap_or(9999));
    for (i, e) in mods.iter().enumerate() {
        let Some(m) = module_by_doc(inv, e) else { continue };
        total += 1;
        let deepened = e.get_extra("deepened").is_some();
        let stale = deepened && e.get_extra("deepened_fingerprint").as_deref() != Some(m.fingerprint.as_str());
        let make = |kind: &str| Task {
            id: format!("{}:{}", if kind == "refresh" { "refresh" } else { "mod" }, m.id.trim_start_matches("mod-")),
            kind: kind.into(),
            title: format!("{} module {}", if kind == "refresh" { "Refresh the summary of" } else { "Summarize" }, m.path),
            target: Some(e.rel_path.clone()),
            files: m.key_files.clone(),
            docs: m.docs.clone(),
            commits: Vec::new(),
            facts: e.body.find(FACTS_START).map(|s| e.body[s..].to_string()).unwrap_or_default(),
        };
        if !deepened {
            if i < 8 { first.push(make("module")) } else { later.push(make("module")) }
        } else if stale {
            done += 1;
            refresh.push(make("refresh"));
        } else {
            done += 1;
        }
    }

    let referenced: HashSet<String> =
        entries.iter().flat_map(|e| e.commits.iter().map(|c| c.chars().take(7).collect::<String>())).collect();
    let mut dec_tasks = Vec::new();
    for c in hist.clusters.iter().take(app.cfg().init.max_decision_tasks) {
        total += 1;
        let all_referenced = c.commits.iter().all(|s| referenced.contains(&s.chars().take(7).collect::<String>()));
        if state.done.contains_key(&c.id) || all_referenced {
            done += 1;
            continue;
        }
        let commits: Vec<Candidate> = c.commits.iter().filter_map(|s| hist.candidates.iter().find(|x| &x.short == s).cloned()).collect();
        dec_tasks.push(Task {
            id: c.id.clone(),
            kind: "decisions".into(),
            title: format!("Distill decisions from {} commit(s) in {}", commits.len(), c.module.as_deref().unwrap_or("the repository")),
            target: None,
            files: Vec::new(),
            docs: Vec::new(),
            commits,
            facts: String::new(),
        });
    }
    let mut pending = first;
    pending.extend(dec_tasks);
    pending.extend(later);
    pending.extend(refresh);
    Plan { pending, done, total }
}

const SUMMARY_RULE: &str = "one line, at most 200 characters: what it is *for* (purpose, not a file list)";

pub fn render_task(app: &App, t: &Task, inline: bool, budget_chars: usize) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "## Task `{}` — {}", t.id, t.title);
    match t.kind.as_str() {
        "purpose" => {
            let _ = writeln!(s, "\nRead (skim; stop once you can explain the project): {}", t.docs.join(", "));
            let _ = writeln!(s, "\nKnown facts:\n{}", t.facts);
            let _ = writeln!(
                s,
                "\nSubmit with `ctx_init_submit`:\n- task_id: \"purpose\"\n- summary: {SUMMARY_RULE}\n- overview: 4–10 lines of Markdown — what the system does, for whom, the main moving parts and how they talk, anything a newcomer must know first."
            );
        }
        "module" | "refresh" => {
            if !t.docs.is_empty() {
                let _ = writeln!(s, "\nRead these docs first: {}", t.docs.join(", "));
            }
            if !t.files.is_empty() {
                let _ = writeln!(s, "\nThen the key files (most important first; skim, stop when you understand the module):");
                for (p, l) in &t.files {
                    let _ = writeln!(s, "- {p} ({l} lines)");
                }
            }
            if !t.facts.is_empty() {
                let _ = writeln!(
                    s,
                    "\nGenerated facts:\n{}",
                    t.facts.lines().filter(|l| !l.starts_with("<!--")).collect::<Vec<_>>().join("\n")
                );
            }
            let _ = writeln!(
                s,
                "\nSubmit with `ctx_init_submit`:\n- task_id: \"{}\"\n- summary: {SUMMARY_RULE}\n- overview: 5–15 lines of Markdown: responsibility, main flows and entry points, important invariants, and pitfalls a newcomer would hit. Do not restate the facts list.\n- learnings (optional): up to 2 non-obvious gotchas worth sharing with the team, as [{{title, body, paths}}].",
                t.id
            );
        }
        "decisions" => {
            let _ = writeln!(s, "\nThese commits look like they encode design decisions:");
            for c in &t.commits {
                let _ = writeln!(s, "\n- {} {} {} — {}", c.date, c.short, c.author, c.subject);
                if !c.reasons.is_empty() {
                    let _ = writeln!(s, "  signals: {}", c.reasons.join("; "));
                }
                if !c.body.trim().is_empty() {
                    let _ = writeln!(s, "  {}", util::truncate_chars(&util::one_line(&c.body), if inline { 900 } else { 400 }));
                }
                if !c.files.is_empty() {
                    let _ = writeln!(s, "  files: {}", c.files.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
                }
            }
            let _ = writeln!(
                s,
                "\nInspect with `git show <sha>` where the message is not enough. Record only durable choices — a direction taken, an alternative rejected, a rule introduced — and skip routine fixes.\n\nSubmit with `ctx_init_submit`:\n- task_id: \"{}\"\n- decisions: at most 3, each {{title, summary, context, decision, consequences, paths, commits, date}} — `decision` states the choice in one or two sentences, `context` why it was needed, `consequences` what it costs or implies; `commits` lists the short shas it came from. An empty list is a valid answer.",
                t.id
            );
        }
        _ => {}
    }
    if inline {
        let mut used = s.len();
        let mut attached = String::new();
        let mut paths: Vec<String> = t.docs.iter().filter(|d| !d.ends_with('/')).cloned().collect();
        paths.extend(t.files.iter().map(|(p, _)| p.clone()));
        for p in paths {
            if used >= budget_chars || secrets::is_sensitive_path(&p) {
                break;
            }
            let Ok(text) = std::fs::read_to_string(app.repo.abs(&p)) else { continue };
            let room = (budget_chars - used).min(6000);
            let excerpt = util::truncate_chars(&text, room);
            let (excerpt, _) = secrets::redact(&excerpt);
            let _ = writeln!(attached, "\n----- {p} -----\n{excerpt}");
            used += excerpt.len() + p.len() + 20;
        }
        for c in &t.commits {
            if used >= budget_chars {
                break;
            }
            if let Ok(stat) = app.repo.git(&["show", "--stat", "--format=%B", &c.short]) {
                let room = (budget_chars - used).min(2500);
                let excerpt = util::truncate_chars(&stat, room);
                let _ = writeln!(attached, "\n----- git show --stat {} -----\n{excerpt}", c.short);
                used += excerpt.len() + 40;
            }
        }
        if !attached.is_empty() {
            let _ = writeln!(s, "\n### Attached sources\n{attached}");
        }
    }
    s
}

fn clean(s: &str) -> String {
    secrets::redact(s.trim()).0
}

fn first_commit_date(app: &App, shas: &[String]) -> Option<String> {
    let mut dates: Vec<String> = shas
        .iter()
        .filter_map(|s| app.repo.git_opt(&["show", "-s", "--format=%ad", "--date=short", s]))
        .filter(|d| !d.is_empty())
        .collect();
    dates.sort();
    dates.pop()
}

pub fn submit(app: &App, sub: Submission) -> Result<String> {
    let (inv, hist) = load_cached(app)?;
    let mut state = super::load_state(app);
    let id = sub.task_id.trim().to_string();
    if id.is_empty() {
        bail!("task_id is required");
    }
    if let Some(pid) = autopilot_owner(app, &id) {
        bail!(
            "`{id}` is being written by `kontext init --deepen` (pid {pid}); give your result as the JSON answer instead of calling ctx_init_submit"
        );
    }
    let store = Store::new(&app.repo, app.cfg());
    let (entries, _) = store.load_all();
    let _lock = app.write_lock.lock().unwrap();
    let mut written: Vec<String> = Vec::new();

    if sub.skip {
        state.done.insert(
            id.clone(),
            format!("skipped {}{}", util::today(), sub.reason.as_deref().map(|r| format!(": {r}")).unwrap_or_default()),
        );
        // skipping a refresh means "the summary still holds": accept the module's current fingerprint
        if let Some(slug) = id.strip_prefix("refresh:") {
            let doc_id = format!("mod-{slug}");
            if let (Some(doc), Some(m)) = (entries.iter().find(|e| e.id == doc_id), inv.modules.iter().find(|m| m.id == doc_id)) {
                let mut e = doc.clone();
                e.set_extra("deepened_fingerprint", FmValue::Str(m.fingerprint.clone()));
                util::write_atomic(&app.repo.abs(&e.rel_path), &e.markdown())?;
            }
        }
        save_state(app, &state)?;
        return Ok(format!("Skipped `{id}`.\n{}", progress_line(app, &inv, &hist, &state)));
    }

    let summary = sub.summary.as_deref().map(clean).map(|s| util::truncate_chars(&util::one_line(&s), 300));
    let overview = sub.overview.as_deref().map(clean).map(|s| util::truncate_chars(&s, 8000));

    if id == "purpose" {
        let ov = entries
            .iter()
            .find(|e| e.kind == "architecture" && e.id == "overview")
            .ok_or_else(|| anyhow!("no overview yet — run `kontext init` first"))?;
        let mut e = ov.clone();
        if let Some(s) = summary {
            e.summary = Some(s);
        }
        if let Some(o) = overview {
            e.body = replace_section(&e.body, "Purpose", &o);
        }
        e.set_extra("deepened", FmValue::Str(util::today()));
        util::write_atomic(&app.repo.abs(&e.rel_path), &e.markdown())?;
        written.push(e.rel_path.clone());
        state.done.insert(id.clone(), util::today());
    } else if let Some(slug) = id.strip_prefix("mod:").or_else(|| id.strip_prefix("refresh:")) {
        let doc_id = format!("mod-{slug}");
        let doc = entries.iter().find(|e| e.id == doc_id).ok_or_else(|| anyhow!("no module doc '{doc_id}'"))?;
        if summary.is_none() && overview.is_none() {
            bail!("submit a summary and/or an overview (or skip=true)");
        }
        let mut e = doc.clone();
        if let Some(s) = summary {
            e.summary = Some(s);
        }
        if let Some(o) = overview {
            e.body = replace_section(&e.body, "Overview", &o);
        }
        e.set_extra("deepened", FmValue::Str(util::today()));
        if let Some(m) = inv.modules.iter().find(|m| m.id == doc_id) {
            e.set_extra("deepened_fingerprint", FmValue::Str(m.fingerprint.clone()));
        }
        util::write_atomic(&app.repo.abs(&e.rel_path), &e.markdown())?;
        written.push(e.rel_path.clone());
        state.done.insert(id.clone(), util::today());
    } else if id.starts_with("dec:") {
        for d in sub.decisions.iter().take(5) {
            if d.title.trim().is_empty() || d.decision.trim().is_empty() {
                continue;
            }
            let mut body = String::new();
            if let Some(c) = d.context.as_deref().filter(|c| !c.trim().is_empty()) {
                let _ = writeln!(body, "## Context\n\n{}\n", clean(c));
            }
            let _ = writeln!(body, "## Decision\n\n{}\n", clean(&d.decision));
            if let Some(c) = d.consequences.as_deref().filter(|c| !c.trim().is_empty()) {
                let _ = writeln!(body, "## Consequences\n\n{}\n", clean(c));
            }
            let commits: Vec<String> =
                d.commits.iter().map(|c| c.trim().chars().take(12).collect()).filter(|c: &String| !c.is_empty()).collect();
            let date = d.date.clone().filter(|x| x.len() >= 10).or_else(|| first_commit_date(app, &commits)).unwrap_or_else(util::today);
            let mut e = Entry {
                kind: "decision".into(),
                title: clean(&d.title),
                status: Some(d.status.as_deref().map(crate::store::normalize_status).unwrap_or_else(|| "accepted".into())),
                date: Some(date.chars().take(10).collect()),
                summary: d.summary.as_deref().map(clean).filter(|s| !s.is_empty()).map(|s| util::truncate_chars(&util::one_line(&s), 240)),
                tags: d.tags.clone(),
                paths: d.paths.clone(),
                commits,
                origin: Some("init".into()),
                body,
                ..Default::default()
            };
            if let Some(rel) = write_init_entry(app, &store, &entries, &mut e)? {
                written.push(rel);
            }
        }
        state.done.insert(id.clone(), util::today());
    } else {
        bail!("unknown task id '{id}'");
    }

    for l in sub.learnings.iter().take(3) {
        if l.title.trim().is_empty() || l.body.trim().is_empty() {
            continue;
        }
        let mut e = Entry {
            kind: "learning".into(),
            title: clean(&l.title),
            date: Some(util::today()),
            tags: l.tags.clone(),
            paths: l.paths.clone(),
            origin: Some("init".into()),
            body: format!("{}\n", clean(&l.body)),
            ..Default::default()
        };
        if let Some(rel) = write_init_entry(app, &store, &entries, &mut e)? {
            written.push(rel);
        }
    }
    save_state(app, &state)?;
    let (entries, _) = store.load_all();
    store.update_index_file(&entries)?;
    let mut out = format!("Recorded `{id}` → {}.\n", if written.is_empty() { "(no entries)".to_string() } else { written.join(", ") });
    out.push_str(&progress_line(app, &inv, &hist, &state));
    Ok(out)
}

/// Init records each finding once: a resubmitted task (or the same finding from a neighbouring
/// module) replaces its own earlier entry instead of adding `…-2`, and an entry people or agents
/// wrote with that title is left alone (None).
fn write_init_entry(app: &App, store: &Store, entries: &[Entry], e: &mut Entry) -> Result<Option<String>> {
    let key = util::slugify(&e.title, 80);
    let Some(old) = entries.iter().find(|x| x.kind == e.kind && util::slugify(&x.title, 80) == key) else {
        return store.write_new(e).map(Some);
    };
    if old.origin.as_deref() != Some("init") {
        return Ok(None);
    }
    e.id = old.id.clone();
    e.rel_path = old.rel_path.clone();
    e.style = old.style;
    if old.date.is_some() {
        e.date = old.date.clone();
    }
    util::write_atomic(&app.repo.abs(&old.rel_path), &e.markdown())?;
    Ok(Some(old.rel_path.clone()))
}

pub fn progress_line(app: &App, inv: &Inventory, hist: &History, state: &InitState) -> String {
    let p = plan(app, inv, hist, state);
    match p.pending.first() {
        Some(next) => {
            format!("Progress: {}/{} tasks. Next: `{}` — {} (call `ctx_init` for details).", p.done, p.total, next.id, next.title)
        }
        None => format!(
            "Progress: {}/{} — bootstrap complete. Review `git diff -- {}` and commit it on a branch for team review.",
            p.done,
            p.total,
            app.cfg().store.dir
        ),
    }
}

/// Parse the first JSON object from an LLM answer (tolerates prose and code fences around it).
pub fn extract_json_object(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim())
        && v.is_object()
    {
        return Some(v);
    }
    let bytes = text.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' if start.is_some() => in_str = true,
            b'{' => {
                if start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' if start.is_some() => {
                depth -= 1;
                if depth == 0 {
                    if let Ok(v) = serde_json::from_str::<Value>(&text[start.unwrap()..=i]) {
                        return Some(v);
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    None
}

const AUTOPILOT_FORMAT: &str = "Answer with ONLY one JSON object (no prose, no code fence) with the fields named in the task: \
task_id, summary, overview, decisions, learnings — omit the ones that do not apply. Use plain Markdown inside strings. \
Do not call any tools to record it (no ctx_init_submit, no ctx_capture): kontext records your answer itself.";

/// Tasks an autopilot run is working on, so that an agent handed the same task text (through an
/// MCP server it can reach) does not record it a second time.
#[derive(Debug, Default, Serialize, Deserialize)]
struct AutopilotLock {
    pid: u32,
    tasks: Vec<String>,
}

fn lock_path(app: &App) -> std::path::PathBuf {
    app.repo.state_dir().join("autopilot.json")
}

struct LockGuard(std::path::PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The pid of another live process running this task through the autopilot.
fn autopilot_owner(app: &App, task: &str) -> Option<u32> {
    let lock: AutopilotLock = serde_json::from_str(&std::fs::read_to_string(lock_path(app)).ok()?).ok()?;
    (lock.pid != std::process::id() && lock.tasks.iter().any(|t| t == task) && pid_alive(lock.pid)).then_some(lock.pid)
}

/// Run pending tasks through an `llm` adapter, `jobs` at a time. Returns (done, failed).
pub fn autopilot(app: &App, llm: &str, jobs: usize, max: usize, report: &dyn Fn(&str)) -> Result<(usize, usize)> {
    let adapter = app
        .registry()
        .get(llm)
        .ok_or_else(|| anyhow!("no adapter '{llm}' (add one with an `llm` op, e.g. `kontext adapters add llm-claude`)"))?;
    if !adapter.has_op("llm") {
        bail!("adapter '{llm}' has no `llm` op");
    }
    let (inv, hist) = load_cached(app)?;
    let state = super::load_state(app);
    let p = plan(app, &inv, &hist, &state);
    let tasks: Vec<Task> = p.pending.into_iter().take(max).collect();
    if tasks.is_empty() {
        report("Nothing to deepen — all tasks are done.");
        return Ok((0, 0));
    }
    let budget = app.cfg().init.task_inline_chars;
    let prompts: Vec<(Task, String)> = tasks
        .into_iter()
        .map(|t| {
            // the task text is written for connected agents; here the answer is the JSON itself
            let task = render_task(app, &t, true, budget).replace("Submit with `ctx_init_submit`:", "Your answer has these fields:");
            let prompt = format!(
                "You are documenting the repository {} for its team. Be concrete and brief.\n\n{task}\n\n{AUTOPILOT_FORMAT}",
                app.repo.id
            );
            (t, prompt)
        })
        .collect();
    let lock = AutopilotLock { pid: std::process::id(), tasks: prompts.iter().map(|(t, _)| t.id.clone()).collect() };
    util::write_atomic(&lock_path(app), &serde_json::to_string(&lock)?)?;
    let _guard = LockGuard(lock_path(app));
    let total = prompts.len();
    let (mut ok, mut failed) = (0, 0);
    let jobs = jobs.clamp(1, 8);
    for (batch_i, batch) in prompts.chunks(jobs).enumerate() {
        let results: Vec<(String, Result<String>, std::time::Duration)> = std::thread::scope(|s| {
            let handles: Vec<_> = batch
                .iter()
                .map(|(t, prompt)| {
                    let a = adapter.clone();
                    s.spawn(move || {
                        let started = std::time::Instant::now();
                        let r = a.llm(prompt);
                        (t.id.clone(), r, started.elapsed())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| (String::new(), Err(anyhow!("worker panicked")), Default::default())))
                .collect()
        });
        for (i, (id, r, took)) in results.into_iter().enumerate() {
            let n = batch_i * jobs + i + 1;
            let outcome = r.and_then(|text| {
                let mut v = extract_json_object(&text)
                    .ok_or_else(|| anyhow!("answer was not JSON: {}", util::truncate_chars(&util::one_line(&text), 160)))?;
                v["task_id"] = Value::String(id.clone());
                let sub: Submission = serde_json::from_value(v)?;
                submit(app, sub)
            });
            match outcome {
                Ok(_) => {
                    ok += 1;
                    report(&format!("  [{n}/{total}] {id} ✓ ({:.1}s)", took.as_secs_f32()));
                }
                Err(e) => {
                    failed += 1;
                    report(&format!("  [{n}/{total}] {id} ✗ {}", util::truncate_chars(&format!("{e:#}"), 200)));
                }
            }
        }
    }
    Ok((ok, failed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_from_prose() {
        let t = "Sure! Here it is:\n```json\n{\"task_id\": \"x\", \"summary\": \"a {b}\"}\n```\nDone.";
        let v = extract_json_object(t).unwrap();
        assert_eq!(v["summary"], "a {b}");
    }

    #[test]
    fn lenient_submission() {
        let v = serde_json::json!({"task_id": "dec:x", "decisions": [{"title": "T", "decision": "D", "paths": "a/b, c", "commits": ["abc1234"]}]});
        let s: Submission = serde_json::from_value(v).unwrap();
        assert_eq!(s.decisions[0].paths, vec!["a/b", "c"]);
    }
}
