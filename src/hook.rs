//! `kontext hook --host claude|codex|cursor`: agent-host hooks that push the team's knowledge at the
//! moment it matters, without the agent having to ask.
//!
//! The host runs the command with its hook JSON on stdin and reads an answer in its own shape on
//! stdout (`SHAPES`). A session start or a subagent start gets the brief; an edit gets the
//! committed, accepted entries that govern the edited path, each told once per session (again when
//! its blob changes); after a compaction the session hears which governing entries it was told.
//! Everything an answer carries is spent from one budget per session (`[hooks]
//! session_budget_tokens`), shared with `ctx_brief` calls that report through [`record_spend`].
//!
//! The hook never blocks the host and never asks a model: any error answers nothing and exits 0.
//! The one refusal is the guard on an agent running `kontext trust`. `KONTEXT_HOOKS=off` or
//! `[hooks] enabled = false` switches the context off.
use crate::app::App;
use crate::repo::Repo;
use crate::store::Entry;
use crate::{lint, ops, util};
use anyhow::{Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    Claude,
    Codex,
    Cursor,
}

impl Host {
    pub fn parse(s: &str) -> Option<Host> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Some(Host::Claude),
            "codex" => Some(Host::Codex),
            "cursor" => Some(Host::Cursor),
            _ => None,
        }
    }

    /// The most characters one answer may carry: Claude Code caps hook context at 10,000
    /// characters, Codex at about 2,500 tokens; Cursor publishes no cap, so Claude's is used.
    fn cap_chars(self) -> usize {
        match self {
            Host::Claude | Host::Cursor => 10_000,
            Host::Codex => 2_500 * 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A session starts, resumes, is cleared or compacted.
    Start,
    /// A subagent starts with a context of its own.
    Subagent,
    /// A tool is about to edit (or has edited) files.
    Edit,
    /// A shell command: only the `kontext trust` guard answers.
    Shell,
    /// The agent finished its turn.
    Stop,
}

/// The JSON a host reads back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    /// `{"hookSpecificOutput": {"hookEventName": <event>, "additionalContext": <text>}}`
    Context,
    /// `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": <text>}}`
    Deny,
    /// `{"additional_context": <text>}`
    SnakeContext,
    /// `{"permission": "deny", "user_message": <text>, "agent_message": <text>}`
    CursorDeny,
    /// The host reads nothing back for this event (or would act on it in a way kontext never wants).
    Silent,
}

/// What each host reads back, one row per host and kind of event.
///
/// - Claude Code: hooks reference, code.claude.com/docs/en/hooks — `hookSpecificOutput.additionalContext`
///   on SessionStart, SubagentStart, PreToolUse/PostToolUse; `permissionDecision: "deny"` on PreToolUse.
///   Stop has no context field of its own; the answer is still context only, never `decision: "block"`.
/// - Codex: developers.openai.com/codex/hooks — `.codex/hooks.json` takes Claude Code's schema and
///   reads the same `hookSpecificOutput` answers; `apply_patch` edits arrive as PreToolUse.
/// - Cursor: cursor.com/docs/agent/hooks — `.cursor/hooks.json`; `sessionStart` and `postToolUse`
///   answer `additional_context`, `beforeShellExecution` answers `permission`. Its `stop` answer
///   (`followup_message`) would start another turn, and `subagentStart` takes no context: silent.
const SHAPES: &[(Host, Kind, Form)] = &[
    (Host::Claude, Kind::Start, Form::Context),
    (Host::Claude, Kind::Subagent, Form::Context),
    (Host::Claude, Kind::Edit, Form::Context),
    (Host::Claude, Kind::Shell, Form::Deny),
    (Host::Claude, Kind::Stop, Form::Context),
    (Host::Codex, Kind::Start, Form::Context),
    (Host::Codex, Kind::Subagent, Form::Context),
    (Host::Codex, Kind::Edit, Form::Context),
    (Host::Codex, Kind::Shell, Form::Deny),
    (Host::Codex, Kind::Stop, Form::Context),
    (Host::Cursor, Kind::Start, Form::SnakeContext),
    (Host::Cursor, Kind::Subagent, Form::Silent),
    (Host::Cursor, Kind::Edit, Form::SnakeContext),
    (Host::Cursor, Kind::Shell, Form::CursorDeny),
    (Host::Cursor, Kind::Stop, Form::Silent),
];

fn render(host: Host, kind: Kind, event: &str, text: &str) -> Option<Value> {
    let form = SHAPES.iter().find(|(h, k, _)| *h == host && *k == kind).map_or(Form::Silent, |r| r.2);
    let event = if event.is_empty() { default_event(kind) } else { event };
    match form {
        Form::Context => Some(json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": text}})),
        Form::Deny => Some(
            json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": text}}),
        ),
        Form::SnakeContext => Some(json!({"additional_context": text})),
        Form::CursorDeny => Some(json!({"permission": "deny", "user_message": text, "agent_message": text})),
        Form::Silent => None,
    }
}

fn default_event(kind: Kind) -> &'static str {
    match kind {
        Kind::Start => "SessionStart",
        Kind::Subagent => "SubagentStart",
        Kind::Edit | Kind::Shell => "PreToolUse",
        Kind::Stop => "Stop",
    }
}

/// Why the guard refuses an agent's `kontext trust`.
const TRUST_DENY: &str = "`kontext trust` approves the adapters this repository declares for this clone. A person runs it in a terminal after reading them; an agent does not. Ask the user to run it.";

// ------------------------------------------------------------------------------------ settings

/// `[hooks]` keys for agent-host hooks, read from every config layer (later wins). The git-hook
/// keys of the same table (`validate`, `trailers`, `remind_inbox`) are not read here.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HookSettings {
    /// `false` turns the context off (the `kontext trust` guard stays).
    pub enabled: bool,
    /// Everything kontext pushes into one agent session: hooks and `ctx_brief` together.
    pub session_budget_tokens: usize,
    /// The most one edit's answer may carry.
    pub edit_budget_tokens: usize,
    /// Answer the Stop hook with what this session was told (context only, never a block).
    pub stop_note: bool,
}

impl Default for HookSettings {
    fn default() -> Self {
        HookSettings { enabled: true, session_budget_tokens: 6000, edit_budget_tokens: 900, stop_note: false }
    }
}

pub fn settings(app: &App) -> HookSettings {
    let mut merged = toml::Table::new();
    for layer in &app.loaded.layers {
        let Ok(text) = std::fs::read_to_string(layer) else { continue };
        let Ok(table) = text.parse::<toml::Table>() else { continue };
        if let Some(toml::Value::Table(hooks)) = table.get("hooks") {
            for (k, v) in hooks {
                merged.insert(k.clone(), v.clone());
            }
        }
    }
    toml::Value::Table(merged).try_into().unwrap_or_default()
}

fn switched_off_by_env() -> bool {
    std::env::var("KONTEXT_HOOKS").is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "off" | "0" | "false" | "no"))
}

// ------------------------------------------------------------------------------------ sessions

/// What one agent session was told and spent. `.git/kontext/sessions/<session_id>.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Session {
    /// Entry id → the blob it was told at.
    #[serde(default)]
    told: BTreeMap<String, String>,
    #[serde(default)]
    spent: usize,
}

fn sessions_dir(repo: &Repo) -> PathBuf {
    repo.state_dir().join("sessions")
}

fn session_key(id: &str) -> String {
    let k: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(128).collect();
    if k.is_empty() { "default".into() } else { k }
}

fn session_path(repo: &Repo, id: &str) -> PathBuf {
    sessions_dir(repo).join(format!("{}.json", session_key(id)))
}

fn load_session(repo: &Repo, id: &str) -> Session {
    std::fs::read_to_string(session_path(repo, id)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_session(repo: &Repo, id: &str, s: &Session) -> Result<()> {
    util::write_atomic(&session_path(repo, id), &serde_json::to_string(s)?)
}

/// Count `tokens` a tool (`ctx_brief`) gave the agent of session `session_id` against the session's
/// one budget, so hooks in the same session hold back by as much.
pub fn record_spend(repo: &Repo, session_id: &str, tokens: usize) -> Result<()> {
    let mut s = load_session(repo, session_id);
    s.spent = s.spent.saturating_add(tokens);
    save_session(repo, session_id, &s)
}

/// Tokens session `session_id` may still be given (hooks and `ctx_brief` together).
pub fn remaining_tokens(app: &App, session_id: &str) -> usize {
    settings(app).session_budget_tokens.saturating_sub(load_session(&app.repo, session_id).spent)
}

/// Sessions untouched for seven days are forgotten.
fn prune_sessions(repo: &Repo) {
    let Ok(dir) = std::fs::read_dir(sessions_dir(repo)) else { return };
    let week = std::time::Duration::from_secs(7 * 24 * 3600);
    for f in dir.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") || name.starts_with('_') {
            continue;
        }
        let old = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > week);
        if old {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

// ------------------------------------------------------------------------------------ input

#[derive(Debug, Default)]
struct Input {
    event: String,
    kind: Option<Kind>,
    source: String,
    session: String,
    cwd: Option<PathBuf>,
    paths: Vec<String>,
    command: Option<String>,
}

const SHELL_TOOLS: &[&str] = &["bash", "shell", "local_shell", "exec_command", "run_terminal_cmd", "terminal", "container.exec"];

fn str_at<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str)).filter(|s| !s.is_empty())
}

/// File paths named by an `apply_patch` body (`*** Update File: src/a.rs`).
fn patch_paths(text: &str, out: &mut Vec<String>) {
    for line in text.lines() {
        let l = line.trim();
        for prefix in ["*** Add File:", "*** Update File:", "*** Delete File:", "*** Move to:"] {
            if let Some(p) = l.strip_prefix(prefix).map(str::trim).filter(|p| !p.is_empty()) {
                out.push(p.to_string());
            }
        }
    }
}

fn collect_patch_paths(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if s.contains("*** ") => patch_paths(s, out),
        Value::Array(a) => a.iter().for_each(|x| collect_patch_paths(x, out)),
        Value::Object(o) => o.values().for_each(|x| collect_patch_paths(x, out)),
        _ => {}
    }
}

fn parse_input(v: &Value) -> Input {
    let event = str_at(v, &["hook_event_name", "hookEventName", "event"]).unwrap_or_default().to_string();
    let tool = str_at(v, &["tool_name", "toolName"]).unwrap_or_default().to_string();
    let input = v.get("tool_input").or_else(|| v.get("toolInput")).cloned().unwrap_or(Value::Null);
    let mut paths = Vec::new();
    for src in [&input, v] {
        if let Some(p) = str_at(src, &["file_path", "filePath", "notebook_path", "target_file", "path"]) {
            paths.push(p.to_string());
        }
    }
    if let Some(edits) = input.get("edits").and_then(Value::as_array) {
        paths.extend(edits.iter().filter_map(|e| str_at(e, &["file_path", "path"])).map(str::to_string));
    }
    collect_patch_paths(&input, &mut paths);
    let command = match input.get("command").or_else(|| v.get("command")) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => Some(a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
        _ => None,
    };
    if let Some(c) = &command {
        patch_paths(c, &mut paths);
    }
    // a patch can be read from `tool_input` and from the command: each path once, in order
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    let is_shell = SHELL_TOOLS.contains(&tool.to_ascii_lowercase().as_str());
    let ev = event.to_ascii_lowercase().replace('_', "");
    let kind = match ev.as_str() {
        "sessionstart" => Some(Kind::Start),
        "subagentstart" => Some(Kind::Subagent),
        "stop" => Some(Kind::Stop),
        "beforeshellexecution" => Some(Kind::Shell),
        "pretooluse" | "posttooluse" => {
            if command.as_deref().is_some_and(is_agent_trust) && (is_shell || paths.is_empty()) {
                Some(Kind::Shell)
            } else if !paths.is_empty() {
                Some(Kind::Edit)
            } else if is_shell {
                Some(Kind::Shell)
            } else {
                None
            }
        }
        _ => None,
    };
    let cwd = str_at(v, &["cwd"])
        .map(PathBuf::from)
        .or_else(|| v.get("workspace_roots").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).map(PathBuf::from));
    Input {
        event,
        kind,
        source: str_at(v, &["source", "trigger"]).unwrap_or_default().to_ascii_lowercase(),
        session: str_at(v, &["session_id", "sessionId", "conversation_id", "conversationId"]).unwrap_or("default").to_string(),
        cwd,
        paths,
        command,
    }
}

/// Whether a shell command runs `kontext trust` (with any global options before it).
fn is_agent_trust(command: &str) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:^|[\s;&|(`"'])(?:\S*/)?kontext(?:\s+(?:-C|--dir)\s+\S+|\s+--?[A-Za-z][\w-]*(?:=\S+)?)*\s+trust(?:$|[\s;&|)`"'])"#)
            .unwrap()
    });
    RE.is_match(command)
}

/// `p` relative to the repository root, when it lies inside it.
fn repo_rel(root: &Path, cwd: Option<&Path>, p: &str) -> Option<String> {
    let pb = PathBuf::from(p);
    let abs = if pb.is_absolute() { pb } else { cwd.unwrap_or(root).join(pb) };
    let roots: Vec<PathBuf> = std::iter::once(root.to_path_buf()).chain(root.canonicalize().ok()).collect();
    let mut cands = vec![abs.clone()];
    if let (Some(parent), Some(name)) = (abs.parent(), abs.file_name())
        && let Ok(cp) = parent.canonicalize()
    {
        cands.push(cp.join(name));
    }
    for c in &cands {
        for r in &roots {
            if let Ok(rest) = c.strip_prefix(r) {
                let rel = rest.to_string_lossy().replace('\\', "/");
                let rel = rel.trim_start_matches("./").to_string();
                if !rel.is_empty() && !rel.split('/').any(|s| s == "..") {
                    return Some(rel);
                }
            }
        }
    }
    None
}

// ------------------------------------------------------------------------------------ knowledge

/// A committed, accepted entry as an edit answer names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Governing {
    id: String,
    kind: String,
    title: String,
    rule: String,
    paths: Vec<String>,
    blob: String,
    /// Instruction-lint rules the entry trips: then it is named by id only.
    flagged: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    key: String,
    entries: Vec<Governing>,
}

/// Committed, accepted entries at HEAD, cached in `.git/kontext/sessions/_entries.json` by the
/// listing of their blobs: an edit costs one `git ls-tree` when nothing changed.
fn committed(app: &App) -> Result<Vec<Governing>> {
    let store = app.store();
    let mut dirs: Vec<String> = app.cfg().kind_names().iter().map(|k| app.cfg().kind_path(k)).collect();
    dirs.sort();
    dirs.dedup();
    let mut args = vec!["ls-tree", "-r", "-z", "HEAD", "--"];
    args.extend(dirs.iter().map(String::as_str));
    let listing = app.repo.git_bytes(&args)?;
    let mut blobs: Vec<(String, String)> = Vec::new();
    for rec in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let rec = String::from_utf8_lossy(rec);
        let Some((meta, path)) = rec.split_once('\t') else { continue };
        if store.is_entry_path(path)
            && let Some(sha) = meta.split_whitespace().nth(2)
        {
            blobs.push((path.to_string(), sha.to_string()));
        }
    }
    let key = util::short_hash(&blobs.iter().map(|(p, s)| format!("{p}\t{s}\n")).collect::<String>());
    let cache_path = sessions_dir(&app.repo).join("_entries.json");
    if let Some(c) = std::fs::read_to_string(&cache_path).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok())
        && c.key == key
    {
        return Ok(c.entries);
    }
    let mut entries = Vec::new();
    if !blobs.is_empty() {
        let input: String = blobs.iter().map(|(_, s)| format!("{s}\n")).collect();
        let out = app.repo.git_with_stdin_bytes(&["cat-file", "--batch"], &input)?;
        let mut rest: &[u8] = &out;
        for (path, sha) in &blobs {
            let Some(nl) = rest.iter().position(|b| *b == b'\n') else { break };
            let header = String::from_utf8_lossy(&rest[..nl]).to_string();
            let after = &rest[nl + 1..];
            let size: usize = header.split_whitespace().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let body = &after[..size.min(after.len())];
            rest = after.get(size..).unwrap_or(&[]);
            rest = rest.strip_prefix(b"\n").unwrap_or(rest);
            let kind = store.kind_for_path(path).unwrap_or_default();
            let e = Entry::parse(path, &String::from_utf8_lossy(body), &kind);
            if !accepted(&e) || e.paths.is_empty() {
                continue;
            }
            entries.push(Governing {
                id: e.id.clone(),
                kind: e.kind.clone(),
                title: util::one_line(&e.title),
                rule: e.rule_text(280),
                paths: e.paths.clone(),
                blob: sha.clone(),
                flagged: lint::entry_findings(&e).into_iter().map(str::to_string).collect(),
            });
        }
    }
    // decisions first, then conventions, then the rest; newest (date-prefixed ids) first within a kind
    let rank = |k: &str| match k {
        "decision" => 0,
        "convention" => 1,
        _ => 2,
    };
    entries.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| b.id.cmp(&a.id)));
    let _ = util::write_atomic(&cache_path, &serde_json::to_string(&Cache { key, entries: entries.clone() })?);
    Ok(entries)
}

/// Reviewed and in force: committed text whose status is accepted (or carries none, as
/// conventions and learnings often do).
fn accepted(e: &Entry) -> bool {
    e.is_active() && matches!(e.status.as_deref(), None | Some("accepted"))
}

fn governs(g: &Governing, path: &str) -> bool {
    g.paths.iter().any(|p| crate::glob::path_matches(p, path) || crate::glob::path_overlaps(p, path))
}

fn line_for(g: &Governing) -> String {
    if g.flagged.is_empty() {
        format!("- [{}] {} ({}): {}", g.id, g.title, g.kind, g.rule)
    } else {
        format!("- [{}] ({}) text withheld: the instruction lint flagged it ({})", g.id, g.kind, g.flagged.join(", "))
    }
}

// ------------------------------------------------------------------------------------ answers

/// The governing entries for the edited paths this session has not been told (or was told at
/// another blob), within the edit and session budgets.
fn on_edit(app: &App, cfg: &HookSettings, host: Host, sess: &mut Session, paths: &[String]) -> Result<Option<String>> {
    if paths.is_empty() {
        return Ok(None);
    }
    let entries = committed(app)?;
    let pending: Vec<&Governing> =
        entries.iter().filter(|g| paths.iter().any(|p| governs(g, p))).filter(|g| sess.told.get(&g.id) != Some(&g.blob)).collect();
    if pending.is_empty() {
        return Ok(None);
    }
    let budget = cfg.edit_budget_tokens.min(cfg.session_budget_tokens.saturating_sub(sess.spent)).min(host.cap_chars() / 4);
    let shown: Vec<&str> = paths.iter().take(3).map(String::as_str).collect();
    let more = if paths.len() > 3 { format!(" and {} more", paths.len() - 3) } else { String::new() };
    let mut text = format!("Team knowledge governing {}{more} (committed, accepted; full text: ctx_read kx:<id>):\n", shown.join(", "));
    let mut told = Vec::new();
    for g in pending {
        let line = line_for(g);
        if util::est_tokens(&text) + util::est_tokens(&line) + 1 > budget {
            break;
        }
        text.push_str(&line);
        text.push('\n');
        told.push(g);
    }
    if told.is_empty() {
        return Ok(None);
    }
    for g in told {
        sess.told.insert(g.id.clone(), g.blob.clone());
    }
    Ok(Some(text))
}

/// The ids this session was told, for a context that lost them (compaction, resume).
fn told_line(app: &App, sess: &Session, max_tokens: usize) -> String {
    if sess.told.is_empty() {
        return String::new();
    }
    let entries = committed(app).unwrap_or_default();
    let mut s = String::from("Governing entries already told this session (full text: ctx_read kx:<id>):\n");
    for id in sess.told.keys() {
        let line = match entries.iter().find(|g| &g.id == id) {
            Some(g) if g.flagged.is_empty() => format!("- [{id}] {}\n", g.title),
            _ => format!("- [{id}]\n"),
        };
        if util::est_tokens(&s) + util::est_tokens(&line) > max_tokens {
            break;
        }
        s.push_str(&line);
    }
    s
}

fn on_start(app: &App, cfg: &HookSettings, host: Host, sess: &mut Session, source: &str) -> Option<String> {
    prune_sessions(&app.repo);
    let cap = (host.cap_chars() / 4).min(cfg.session_budget_tokens.saturating_sub(sess.spent));
    let brief_budget = app.cfg().brief.budget_tokens;
    match source {
        // the context kept what it was told, or lost only its detail: a short brief and the ids
        "compact" | "resume" => {
            let told = told_line(app, sess, cap / 3);
            let budget = (brief_budget / 2).min(cap.saturating_sub(util::est_tokens(&told)));
            let brief = if budget >= 150 { ops::brief(app, &[], budget, false) } else { String::new() };
            let text = [brief.trim_end(), told.trim_end()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("\n\n");
            (!text.is_empty()).then_some(text)
        }
        // a new context: whatever was told before is gone
        _ => {
            sess.told.clear();
            let budget = brief_budget.min(cap);
            (budget >= 150).then(|| ops::brief(app, &[], budget, false))
        }
    }
}

fn on_subagent(app: &App, cfg: &HookSettings, host: Host, sess: &Session) -> Option<String> {
    let budget = app.cfg().brief.budget_tokens.min(cfg.session_budget_tokens.saturating_sub(sess.spent)).min(host.cap_chars() / 4);
    (budget >= 150).then(|| ops::brief(app, &[], budget, false))
}

fn on_stop(cfg: &HookSettings, sess: &Session) -> Option<String> {
    if !cfg.stop_note || sess.told.is_empty() {
        return None;
    }
    let ids: Vec<&str> = sess.told.keys().take(8).map(String::as_str).collect();
    let mut s = format!("kontext this session: {} governing entr{} told", sess.told.len(), if sess.told.len() == 1 { "y" } else { "ies" });
    let _ = write!(s, " ({}{})", ids.join(", "), if sess.told.len() > ids.len() { ", …" } else { "" });
    let _ = write!(s, "; {} of {} context tokens spent.", sess.spent, cfg.session_budget_tokens);
    Some(s)
}

// ------------------------------------------------------------------------------------ entry

/// Run one hook for `host`: read its JSON from stdin, print its answer (or nothing). Always 0.
pub fn main(dir: Option<PathBuf>, host: &str) -> i32 {
    std::panic::set_hook(Box::new(|_| {}));
    let mut raw = String::new();
    if std::io::stdin().take(4 * 1024 * 1024).read_to_string(&mut raw).is_err() {
        return 0;
    }
    let host = host.to_string();
    let answer = std::panic::catch_unwind(move || answer(dir, &host, &raw).ok().flatten()).ok().flatten();
    if let Some(a) = answer {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{a}");
        let _ = out.flush();
    }
    0
}

/// The answer to one hook payload, as the line to print.
pub fn answer(dir: Option<PathBuf>, host: &str, raw: &str) -> Result<Option<String>> {
    let host = Host::parse(host).ok_or_else(|| anyhow!("unknown host '{host}' (claude, codex or cursor)"))?;
    let v: Value = serde_json::from_str(raw)?;
    let input = parse_input(&v);
    let Some(kind) = input.kind else { return Ok(None) };
    // the guard needs no repository and holds with the context switched off
    if kind == Kind::Shell {
        return Ok(input
            .command
            .as_deref()
            .filter(|c| is_agent_trust(c))
            .and_then(|_| render(host, kind, &input.event, TRUST_DENY))
            .map(|v| v.to_string()));
    }
    if switched_off_by_env() {
        return Ok(None);
    }
    let dir = dir.or_else(|| input.cwd.clone()).unwrap_or_else(|| PathBuf::from("."));
    let app = App::open(&dir)?;
    if !app.is_set_up() {
        return Ok(None);
    }
    let cfg = settings(&app);
    if !cfg.enabled {
        return Ok(None);
    }
    let mut sess = load_session(&app.repo, &input.session);
    let text = match kind {
        Kind::Start => on_start(&app, &cfg, host, &mut sess, &input.source),
        Kind::Subagent => on_subagent(&app, &cfg, host, &sess),
        Kind::Edit => {
            let root = app.repo.root.clone();
            let cwd = input.cwd.as_deref();
            let mut rel: Vec<String> = input.paths.iter().filter_map(|p| repo_rel(&root, cwd, p)).collect();
            rel.dedup();
            on_edit(&app, &cfg, host, &mut sess, &rel)?
        }
        Kind::Stop => on_stop(&cfg, &sess),
        Kind::Shell => None,
    };
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
        if kind == Kind::Start {
            save_session(&app.repo, &input.session, &sess)?;
        }
        return Ok(None);
    };
    let text = util::truncate_chars(&text, host.cap_chars());
    let Some(out) = render(host, kind, &input.event, &text) else { return Ok(None) };
    sess.spent = sess.spent.saturating_add(util::est_tokens(&text));
    save_session(&app.repo, &input.session, &sess)?;
    Ok(Some(out.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_and_kind_has_one_shape() {
        for host in [Host::Claude, Host::Codex, Host::Cursor] {
            for kind in [Kind::Start, Kind::Subagent, Kind::Edit, Kind::Shell, Kind::Stop] {
                assert_eq!(SHAPES.iter().filter(|(h, k, _)| *h == host && *k == kind).count(), 1, "{host:?} {kind:?}");
            }
        }
        // nothing kontext answers ever blocks a turn or asks for another one
        for (_, kind, form) in SHAPES {
            assert!(*kind == Kind::Shell || !matches!(form, Form::Deny | Form::CursorDeny));
        }
        let v = render(Host::Claude, Kind::Edit, "PreToolUse", "x").unwrap();
        assert_eq!(v, json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "x"}}));
        assert_eq!(render(Host::Cursor, Kind::Edit, "postToolUse", "x").unwrap(), json!({"additional_context": "x"}));
        assert!(render(Host::Cursor, Kind::Stop, "stop", "x").is_none());
    }

    #[test]
    fn the_trust_guard_matches_only_kontext_trust() {
        for yes in [
            "kontext trust",
            "cd repo && kontext trust",
            "kontext -C ../x trust",
            "/usr/local/bin/kontext --dir=. trust",
            "(kontext trust)",
        ] {
            assert!(is_agent_trust(yes), "{yes}");
        }
        for no in ["kontext search trust", "echo trust kontext", "kontext trusted", "grep -r 'trust' src", "kontext status"] {
            assert!(!is_agent_trust(no), "{no}");
        }
    }

    #[test]
    fn inputs_of_each_host_are_read() {
        let claude = json!({"session_id": "abc", "hook_event_name": "PreToolUse", "tool_name": "Edit", "cwd": "/r",
            "tool_input": {"file_path": "/r/src/a.rs", "old_string": "a", "new_string": "b"}});
        let i = parse_input(&claude);
        assert_eq!((i.kind, i.session.as_str(), i.paths.clone()), (Some(Kind::Edit), "abc", vec!["/r/src/a.rs".to_string()]));
        let codex = json!({"session_id": "s1", "hook_event_name": "PreToolUse", "tool_name": "apply_patch",
            "tool_input": {"command": "*** Begin Patch\n*** Update File: src/b.rs\n@@\n-x\n+y\n*** Add File: src/c.rs\n+z\n*** End Patch\n"}});
        assert_eq!(parse_input(&codex).paths, vec!["src/b.rs", "src/c.rs"]);
        let cursor = json!({"conversation_id": "c1", "hook_event_name": "postToolUse", "tool_name": "Write",
            "tool_input": {"file_path": "src/d.rs"}, "workspace_roots": ["/w"]});
        let i = parse_input(&cursor);
        assert_eq!((i.kind, i.session.as_str(), i.cwd), (Some(Kind::Edit), "c1", Some(PathBuf::from("/w"))));
        let shell = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "kontext trust"}});
        assert_eq!(parse_input(&shell).kind, Some(Kind::Shell));
        let start = json!({"hook_event_name": "SessionStart", "source": "compact", "session_id": "x"});
        let i = parse_input(&start);
        assert_eq!((i.kind, i.source.as_str()), (Some(Kind::Start), "compact"));
    }

    #[test]
    fn session_keys_stay_inside_the_sessions_dir() {
        assert_eq!(session_key("../../etc/passwd"), "etcpasswd");
        assert_eq!(session_key(""), "default");
        assert_eq!(session_key("3f2a-b_c"), "3f2a-b_c");
    }
}
