//! Agent threads — Claude Code and Codex transcripts, and the Superset workspaces that group
//! them — read into a compact, redacted transcript that team knowledge can be distilled from.
//! Raw transcripts never leave this machine; only what `kontext distill` or an agent captures does.
use crate::util;
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Text,
}

impl Agent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Text => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Agent,
    Tool,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

/// A thread found on this machine (not read yet).
#[derive(Debug, Clone)]
pub struct Found {
    pub agent: Agent,
    pub id: String,
    pub path: PathBuf,
    pub cwd: Option<String>,
    /// Superset workspace it belongs to, when found through one.
    pub workspace: Option<String>,
}

impl Found {
    pub fn label(&self) -> String {
        format!("{}:{}", self.agent.as_str(), short(&self.id))
    }
}

#[derive(Debug, Clone)]
pub struct Thread {
    pub agent: Agent,
    pub id: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub title: Option<String>,
    pub started: Option<String>,
    pub turns: Vec<Turn>,
}

pub fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

// ------------------------------------------------------------------------------ where they live

/// Claude Code config dirs with a `projects/` dir: `$CLAUDE_CONFIG_DIR`, `~/.claude` and
/// `~/.claude-*` (accounts that wrappers such as Superset switch between).
pub fn claude_dirs() -> Vec<PathBuf> {
    let home = util::home_dir();
    let mut v: Vec<PathBuf> = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()).map(PathBuf::from).into_iter().collect();
    v.push(home.join(".claude"));
    if let Ok(rd) = std::fs::read_dir(&home) {
        let mut accounts: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(".claude-")))
            .collect();
        accounts.sort();
        v.extend(accounts);
    }
    let mut seen = HashSet::new();
    v.retain(|d| d.join("projects").is_dir() && seen.insert(d.canonicalize().unwrap_or_else(|_| d.clone())));
    v
}

/// Codex homes (`$CODEX_HOME`, `~/.codex`) with their rollout dirs.
fn codex_roots() -> Vec<PathBuf> {
    let mut homes: Vec<PathBuf> = std::env::var_os("CODEX_HOME").filter(|d| !d.is_empty()).map(PathBuf::from).into_iter().collect();
    homes.push(util::home_dir().join(".codex"));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for h in homes {
        if !seen.insert(h.canonicalize().unwrap_or_else(|_| h.clone())) {
            continue;
        }
        for sub in ["sessions", "archived_sessions"] {
            if h.join(sub).is_dir() {
                out.push(h.join(sub));
            }
        }
    }
    out
}

/// Every Codex rollout file (`rollout-<time>-<id>.jsonl`), newest first.
fn codex_rollouts() -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 4 {
                walk(&p, depth + 1, out);
            } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for r in codex_roots() {
        walk(&r, 0, &mut out);
    }
    // the file name starts with the time, so names sort chronologically
    out.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    out
}

/// Claude Code names a project dir after its path: every character but letters and digits becomes `-`.
pub fn claude_slug(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH)
}

/// A Claude Code session by id (or id prefix); the same session can sit in several accounts —
/// the largest copy wins.
fn find_claude(id: &str) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = Vec::new();
    for d in claude_dirs() {
        let Ok(projects) = std::fs::read_dir(d.join("projects")) else { continue };
        for proj in projects.flatten() {
            let exact = proj.path().join(format!("{id}.jsonl"));
            if exact.is_file() {
                hits.push(exact);
                continue;
            }
            if id.len() >= 8
                && let Ok(files) = std::fs::read_dir(proj.path())
            {
                hits.extend(files.flatten().map(|f| f.path()).filter(|p| {
                    p.extension().is_some_and(|x| x == "jsonl") && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with(id))
                }));
            }
        }
    }
    hits.into_iter().max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

fn find_codex(id: &str) -> Option<PathBuf> {
    codex_rollouts().into_iter().find(|p| {
        let stem = p.file_stem().and_then(|n| n.to_str()).unwrap_or("");
        // `rollout-<time>-<uuid>`: the id is the last 36 characters
        let uuid = &stem[stem.len().saturating_sub(36)..];
        uuid == id || (id.len() >= 8 && uuid.starts_with(id))
    })
}

/// The first line of a Codex rollout: its `session_meta` (id, cwd, git, timestamp).
fn codex_meta(p: &Path) -> Option<Value> {
    use std::io::BufRead;
    let f = std::fs::File::open(p).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(f).read_line(&mut line).ok()?;
    let v: Value = serde_json::from_str(&line).ok()?;
    (v["type"] == "session_meta").then(|| v["payload"].clone())
}

/// Where a thread ran, from its transcript (Superset binds threads to a terminal, not a directory).
pub fn thread_cwd(f: &Found) -> Option<String> {
    use std::io::BufRead;
    match f.agent {
        Agent::Codex => codex_meta(&f.path).and_then(|m| m["cwd"].as_str().map(str::to_string)),
        Agent::Claude => {
            let file = std::fs::File::open(&f.path).ok()?;
            std::io::BufReader::new(file)
                .lines()
                .take(200)
                .map_while(Result::ok)
                .find_map(|l| serde_json::from_str::<Value>(&l).ok().and_then(|v| v["cwd"].as_str().map(str::to_string)))
        }
        Agent::Text => None,
    }
}

pub fn within(cwd: &str, roots: &[String]) -> bool {
    roots.iter().any(|r| cwd == r || cwd.starts_with(&format!("{}/", r.trim_end_matches('/'))))
}

/// Threads started in any of `roots` (a repository's worktrees), newest first.
pub fn in_roots(roots: &[String], limit: usize) -> Vec<Found> {
    let mut out: Vec<(std::time::SystemTime, Found)> = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let slug = claude_slug(root.trim_end_matches('/'));
        let mut copies: Vec<PathBuf> = Vec::new();
        for d in claude_dirs() {
            let Ok(files) = std::fs::read_dir(d.join("projects").join(&slug)) else { continue };
            copies.extend(files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")));
        }
        // a session copied between accounts: keep its largest copy
        copies.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
        for p in copies {
            let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if seen.insert(format!("claude:{id}")) {
                out.push((mtime(&p), Found { agent: Agent::Claude, id, path: p, cwd: Some(root.clone()), workspace: None }));
            }
        }
    }
    for p in codex_rollouts().into_iter().take(5000) {
        let Some(meta) = codex_meta(&p) else { continue };
        let cwd = meta["cwd"].as_str().unwrap_or("").to_string();
        if cwd.is_empty() || !within(&cwd, roots) {
            continue;
        }
        let id =
            meta["id"].as_str().map(str::to_string).unwrap_or_else(|| p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string());
        if seen.insert(format!("codex:{id}")) {
            out.push((mtime(&p), Found { agent: Agent::Codex, id, path: p, cwd: Some(cwd), workspace: None }));
        }
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.0));
    out.into_iter().take(limit).map(|(_, f)| f).collect()
}

/// A thread named on the command line: `claude:<id>`, `codex:<id>`, `last` per agent, or a file.
pub fn resolve(agent: Option<Agent>, spec: &str, roots: &[String]) -> Result<Found> {
    let spec = spec.trim();
    if spec == "last" || spec.is_empty() {
        let recent = in_roots(roots, 200);
        return recent.into_iter().find(|f| agent.is_none_or(|a| a == f.agent)).ok_or_else(|| {
            anyhow!("no {} thread found for this repository on this machine", agent.map(|a| a.as_str()).unwrap_or("agent"))
        });
    }
    let (agent, id) = match (agent, spec.split_once(':')) {
        (None, Some(("claude", id))) => (Some(Agent::Claude), id),
        (None, Some(("codex", id))) => (Some(Agent::Codex), id),
        (a, _) => (a, spec),
    };
    let path = PathBuf::from(id);
    if agent.is_none() && (path.is_file() || id == "-") {
        return Ok(Found { agent: detect(&path), id: file_id(&path), path, cwd: None, workspace: None });
    }
    let try_claude = || find_claude(id).map(|p| Found { agent: Agent::Claude, id: file_id(&p), path: p, cwd: None, workspace: None });
    let try_codex = || {
        find_codex(id).map(|p| {
            let id = codex_meta(&p).and_then(|m| m["id"].as_str().map(str::to_string)).unwrap_or_else(|| id.to_string());
            Found { agent: Agent::Codex, id, path: p, cwd: None, workspace: None }
        })
    };
    let found = match agent {
        Some(Agent::Claude) => try_claude(),
        Some(Agent::Codex) => try_codex(),
        _ => try_claude().or_else(try_codex),
    };
    found
        .ok_or_else(|| anyhow!("no thread '{spec}' on this machine (Claude Code: {}; Codex: {})", claude_dirs().len(), codex_roots().len()))
}

fn file_id(p: &Path) -> String {
    p.file_stem().and_then(|s| s.to_str()).unwrap_or("stdin").to_string()
}

/// A file's format from its first line.
fn detect(p: &Path) -> Agent {
    if p.as_os_str() == "-" {
        return Agent::Text;
    }
    use std::io::BufRead;
    let Ok(f) = std::fs::File::open(p) else { return Agent::Text };
    let mut first = String::new();
    let _ = std::io::BufReader::new(f).read_line(&mut first);
    match serde_json::from_str::<Value>(&first) {
        Ok(v) if v["type"] == "session_meta" || v.get("payload").is_some() => Agent::Codex,
        Ok(v) if v.get("sessionId").is_some() || v.get("message").is_some() || v.get("type").is_some() => Agent::Claude,
        _ => Agent::Text,
    }
}

// ------------------------------------------------------------------------------------- reading

pub fn load(f: &Found) -> Result<Thread> {
    let text = if f.path.as_os_str() == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
        s
    } else {
        std::fs::read_to_string(&f.path).map_err(|e| anyhow!("cannot read {}: {e}", f.path.display()))?
    };
    let mut t = match f.agent {
        Agent::Claude => parse_claude(&text),
        Agent::Codex => parse_codex(&text),
        Agent::Text => Thread {
            agent: Agent::Text,
            id: String::new(),
            cwd: None,
            branch: None,
            title: None,
            started: None,
            turns: vec![Turn { role: Role::User, text: text.trim().to_string() }],
        },
    };
    if t.id.is_empty() {
        t.id = f.id.clone();
    }
    if t.cwd.is_none() {
        t.cwd = f.cwd.clone();
    }
    Ok(t)
}

/// Text that tools inject into a user turn and nobody typed: instructions, environment, hooks.
fn injected(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("# AGENTS.md instructions")
        || t.starts_with("<environment_context>")
        || t.starts_with("<user_instructions>")
        || t.starts_with("<permissions")
        || t.starts_with("<skills_instructions>")
        || t.starts_with("<collaboration_mode>")
        || t.starts_with("<local-command-")
        || t.starts_with("<command-")
        || t.starts_with("Caveat: The messages below were generated")
        || t.starts_with("[Request interrupted")
}

/// Drop `<system-reminder>…</system-reminder>` and similar blocks from a message.
fn strip_blocks(text: &str) -> String {
    let mut s = text.to_string();
    for tag in ["system-reminder", "local-command-stdout", "local-command-stderr"] {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        while let Some(a) = s.find(&open) {
            let Some(b) = s[a..].find(&close) else { break };
            s.replace_range(a..a + b + close.len(), "");
        }
    }
    s.trim().to_string()
}

fn push(turns: &mut Vec<Turn>, role: Role, text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    // Codex logs a message both as an event and as a response item
    if turns.last().is_some_and(|l| l.role == role && l.text == text) {
        return;
    }
    turns.push(Turn { role, text });
}

pub fn parse_claude(text: &str) -> Thread {
    let mut t = Thread { agent: Agent::Claude, id: String::new(), cwd: None, branch: None, title: None, started: None, turns: Vec::new() };
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if e["type"] == "ai-title"
            && let Some(title) = e["aiTitle"].as_str()
        {
            t.title = Some(title.to_string());
            continue;
        }
        if t.id.is_empty()
            && let Some(s) = e["sessionId"].as_str()
        {
            t.id = s.to_string();
        }
        if t.cwd.is_none() {
            t.cwd = e["cwd"].as_str().map(str::to_string);
        }
        if t.branch.is_none() {
            t.branch = e["gitBranch"].as_str().filter(|b| !b.is_empty()).map(str::to_string);
        }
        if t.started.is_none() {
            t.started = e["timestamp"].as_str().map(str::to_string);
        }
        if e["isSidechain"] == true || e["isMeta"] == true {
            continue;
        }
        let msg = &e["message"];
        match e["type"].as_str() {
            Some("user") => match &msg["content"] {
                Value::String(s) if !injected(s) => push(&mut t.turns, Role::User, strip_blocks(s)),
                Value::Array(blocks) => {
                    for b in blocks {
                        if b["type"] == "text"
                            && let Some(s) = b["text"].as_str().filter(|s| !injected(s))
                        {
                            push(&mut t.turns, Role::User, strip_blocks(s));
                        }
                    }
                }
                _ => {}
            },
            Some("assistant") => {
                for b in msg["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => push(&mut t.turns, Role::Agent, b["text"].as_str().unwrap_or("").to_string()),
                        Some("tool_use") => {
                            if let Some(s) = claude_tool(b["name"].as_str().unwrap_or(""), &b["input"]) {
                                push(&mut t.turns, Role::Tool, s);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    t
}

/// One line per tool call that changes something or says something (reads and searches are noise).
fn claude_tool(name: &str, input: &Value) -> Option<String> {
    let s = |k: &str| input[k].as_str().unwrap_or("").to_string();
    Some(match name {
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => format!("edit {}", s("file_path")),
        "Bash" => format!("$ {}", util::truncate_chars(&util::one_line(&s("command")), 200)),
        "Task" | "Agent" => format!("subagent: {}", s("description")),
        "WebFetch" => format!("web {}", s("url")),
        "WebSearch" => format!("web search {}", s("query")),
        n if n.starts_with("mcp__") => {
            let short = n.trim_start_matches("mcp__").replacen("__", ".", 1);
            let what = ["title", "query", "target", "focus"]
                .iter()
                .find_map(|k| input.get(*k).filter(|v| !v.is_null()).map(|v| util::one_line(&v.to_string())));
            match what {
                Some(w) => format!("{short} {}", util::truncate_chars(&w, 120)),
                None => short,
            }
        }
        _ => return None,
    })
}

pub fn parse_codex(text: &str) -> Thread {
    let mut t = Thread { agent: Agent::Codex, id: String::new(), cwd: None, branch: None, title: None, started: None, turns: Vec::new() };
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let p = &e["payload"];
        match e["type"].as_str() {
            Some("session_meta") => {
                t.id = p["id"].as_str().unwrap_or("").to_string();
                t.cwd = p["cwd"].as_str().map(str::to_string);
                t.branch = p["git"]["branch"].as_str().map(str::to_string);
                t.started = p["timestamp"].as_str().or(e["timestamp"].as_str()).map(str::to_string);
            }
            Some("response_item") => match p["type"].as_str() {
                Some("message") => {
                    let role = match p["role"].as_str() {
                        Some("user") => Role::User,
                        Some("assistant") => Role::Agent,
                        _ => continue,
                    };
                    for c in p["content"].as_array().into_iter().flatten() {
                        if let Some(s) = c["text"].as_str().filter(|s| !injected(s)) {
                            push(&mut t.turns, role, s.to_string());
                        }
                    }
                }
                Some("function_call") | Some("custom_tool_call") | Some("local_shell_call") => {
                    for s in codex_tool(p) {
                        push(&mut t.turns, Role::Tool, s);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    if t.title.is_none() {
        t.title = t.turns.iter().find(|x| x.role == Role::User).map(|x| util::truncate_chars(&util::one_line(&x.text), 90));
    }
    t
}

/// Commands and edits inside a Codex tool call — plain, patch or code-mode (`tools.exec_command({cmd:…})`).
fn codex_tool(p: &Value) -> Vec<String> {
    static CMD: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"\bcmd\s*:\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|`([^`]*)`)"#).unwrap());
    static TOOL: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\btools\.(mcp__[A-Za-z0-9_]+)\s*\(").unwrap());
    static PATCH: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?m)^\*\*\* (?:Update|Add|Delete) File: (.+)$").unwrap());
    let name = p["name"].as_str().unwrap_or("");
    let raw = match (&p["arguments"], &p["input"]) {
        (Value::String(a), _) => a.clone(),
        (Value::Null, Value::String(i)) => i.clone(),
        (a, i) => {
            if a.is_null() {
                i.to_string()
            } else {
                a.to_string()
            }
        }
    };
    let mut out = Vec::new();
    for c in PATCH.captures_iter(&raw) {
        out.push(format!("edit {}", c[1].trim()));
    }
    for c in CMD.captures_iter(&raw) {
        let cmd = c.get(1).or(c.get(2)).or(c.get(3)).map(|m| m.as_str()).unwrap_or("");
        out.push(format!("$ {}", util::truncate_chars(&util::one_line(&cmd.replace("\\n", " ").replace("\\\"", "\"")), 200)));
    }
    for c in TOOL.captures_iter(&raw) {
        out.push(c[1].trim_start_matches("mcp__").replacen("__", ".", 1));
    }
    if out.is_empty() {
        // a plain shell call: {"command": ["bash", "-lc", "…"]} or {"cmd": "…"}
        let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let cmd = match (&v["command"], &v["cmd"]) {
            (Value::Array(a), _) => a.last().and_then(|x| x.as_str()).unwrap_or("").to_string(),
            (Value::String(s), _) | (_, Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        if !cmd.is_empty() {
            out.push(format!("$ {}", util::truncate_chars(&util::one_line(&cmd), 200)));
        } else if !name.is_empty() && name != "exec" {
            out.push(name.to_string());
        }
    }
    out
}

// ----------------------------------------------------------------------------------- rendering

/// The thread as text for a reader (an agent or an LLM): who said what, what was changed, with
/// paths relative to the repository, secrets redacted, and personal data (email addresses, phone
/// numbers, IBANs, cards, IP addresses) and the `extra` patterns masked.
pub fn render(t: &Thread, repo_root: &Path, extra: &[regex::Regex]) -> String {
    let mut s = format!("Thread {}:{}", t.agent.as_str(), short(&t.id));
    if let Some(title) = &t.title {
        s.push_str(&format!(" — {}", util::one_line(title)));
    }
    let mut meta = Vec::new();
    if let Some(b) = &t.branch {
        meta.push(format!("branch {b}"));
    }
    if let Some(d) = &t.started {
        meta.push(format!("started {}", d.chars().take(10).collect::<String>()));
    }
    meta.push(format!("{} turns", t.turns.len()));
    s.push_str(&format!(" ({})\n\n", meta.join(", ")));
    for turn in &t.turns {
        match turn.role {
            Role::User => s.push_str(&format!("User: {}\n\n", util::truncate_chars(&turn.text, 4000))),
            Role::Agent => s.push_str(&format!("Agent: {}\n\n", util::truncate_chars(&turn.text, 3000))),
            Role::Tool => s.push_str(&format!("  → {}\n", turn.text)),
        }
    }
    let mut prefixes: Vec<String> = t.cwd.iter().cloned().collect();
    prefixes.push(repo_root.to_string_lossy().to_string());
    for p in prefixes {
        let p = p.trim_end_matches('/');
        if p.len() > 1 {
            s = s.replace(&format!("{p}/"), "");
        }
    }
    let s = crate::secrets::redact(&s).0;
    crate::secrets::redact_personal(&s, extra).0
}

/// Split a rendered thread into parts of about `size` characters, at turn boundaries.
pub fn parts(text: &str, size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split_inclusive("\n\n") {
        if cur.len() + para.len() > size && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(para);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

// ------------------------------------------------------------------------------------ Superset

/// A Superset workspace (a worktree or a session dir) and the agent threads its terminals ran.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub kind: String,
    pub path: String,
    /// (agent, session id, transcript path) from Superset's terminal bindings
    pub sessions: Vec<(String, String, Option<String>)>,
}

/// Superset's host databases (`~/.superset/host/<organization>/host.db`), the current organization first.
fn superset_dbs() -> Vec<PathBuf> {
    let home = std::env::var_os("SUPERSET_HOME_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| util::home_dir().join(".superset"));
    let mut dbs: Vec<PathBuf> = std::fs::read_dir(home.join("host"))
        .map(|rd| rd.flatten().map(|e| e.path().join("host.db")).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    if let Ok(org) = std::env::var("SUPERSET_ORGANIZATION_ID") {
        dbs.sort_by_key(|p| !p.to_string_lossy().contains(&org));
    }
    dbs
}

fn sqlite_json(db: &Path, sql: &str) -> Result<Vec<Value>> {
    let out = std::process::Command::new("sqlite3")
        .arg("-readonly")
        .arg("-json")
        .arg(db)
        .arg(sql)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| anyhow!("Superset workspaces are read with the `sqlite3` command: {e}"))?;
    if !out.status.success() {
        bail!("sqlite3: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&text)?)
}

/// A workspace by id (or prefix), worktree dir name, path or branch; `None` means the current one
/// (`$SUPERSET_WORKSPACE_ID`, set in Superset's terminals).
pub fn superset_workspace(query: Option<&str>) -> Result<Workspace> {
    let q = match query.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => q.to_string(),
        None => std::env::var("SUPERSET_WORKSPACE_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("not in a Superset terminal: name the workspace, e.g. --superset <id | worktree name>"))?,
    };
    if !q.chars().all(|c| c.is_ascii_alphanumeric() || "-_./ ".contains(c)) {
        bail!("a Superset workspace is named by its id, worktree name, path or branch");
    }
    let dbs = superset_dbs();
    if dbs.is_empty() {
        bail!("no Superset data on this machine (~/.superset/host/*/host.db)");
    }
    let lit = q.replace('\'', "''");
    for db in &dbs {
        let rows = sqlite_json(
            db,
            &format!(
                "select id, name, branch, type, worktree_path from workspaces \
                 where id = '{lit}' or id like '{lit}%' or worktree_path = '{lit}' or worktree_path like '%/{lit}' or branch = '{lit}' \
                 order by (id = '{lit}') desc, last_activity_at desc limit 6"
            ),
        )?;
        if rows.is_empty() {
            continue;
        }
        let exact = rows.iter().any(|r| r["id"] == Value::String(q.clone()));
        if rows.len() > 1 && !exact {
            let list: Vec<String> = rows
                .iter()
                .map(|r| {
                    format!(
                        "{} {} ({})",
                        short(r["id"].as_str().unwrap_or("")),
                        r["worktree_path"].as_str().unwrap_or(""),
                        r["branch"].as_str().unwrap_or("")
                    )
                })
                .collect();
            bail!("'{q}' matches several Superset workspaces — use the id:\n  {}", list.join("\n  "));
        }
        let r = &rows[0];
        let id = r["id"].as_str().unwrap_or("").to_string();
        let sessions = sqlite_json(
            db,
            &format!(
                "select agent_id, agent_session_id, transcript_path from terminal_agent_bindings where workspace_id = '{}' order by started_at",
                id.replace('\'', "''")
            ),
        )?;
        let mut seen = HashSet::new();
        let sessions = sessions
            .iter()
            .filter_map(|s| {
                let agent = s["agent_id"].as_str()?.to_string();
                let sid = s["agent_session_id"].as_str().filter(|x| !x.is_empty())?.to_string();
                seen.insert(format!("{agent}:{sid}"))
                    .then(|| (agent, sid, s["transcript_path"].as_str().filter(|x| !x.is_empty()).map(str::to_string)))
            })
            .collect();
        return Ok(Workspace {
            id,
            name: r["name"].as_str().unwrap_or("").to_string(),
            branch: r["branch"].as_str().unwrap_or("").to_string(),
            kind: r["type"].as_str().unwrap_or("").to_string(),
            path: r["worktree_path"].as_str().unwrap_or("").to_string(),
            sessions,
        });
    }
    bail!("no Superset workspace '{q}'")
}

/// The threads of a workspace that are on this machine, and how many of its sessions are not
/// (ephemeral runs keep no transcript; other agents are not read yet).
pub fn workspace_threads(ws: &Workspace) -> (Vec<Found>, Vec<String>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for (agent, sid, transcript) in &ws.sessions {
        let agent_kind = match agent.as_str() {
            "claude" => Agent::Claude,
            "codex" => Agent::Codex,
            other => {
                missing.push(format!("{other}:{}", short(sid)));
                continue;
            }
        };
        let path = transcript.as_ref().map(PathBuf::from).filter(|p| p.is_file()).or_else(|| match agent_kind {
            Agent::Claude => find_claude(sid),
            _ => find_codex(sid),
        });
        match path {
            Some(p) if seen.insert(format!("{agent}:{sid}")) => found.push(Found {
                agent: agent_kind,
                id: sid.clone(),
                path: p,
                cwd: Some(ws.path.clone()),
                workspace: Some(ws.id.clone()),
            }),
            Some(_) => {}
            None => missing.push(format!("{agent}:{}", short(sid))),
        }
    }
    // threads started in the workspace without a binding (older runs, other terminals)
    for f in in_roots(std::slice::from_ref(&ws.path), 100) {
        if seen.insert(format!("{}:{}", f.agent.as_str(), f.id)) {
            found.push(Found { workspace: Some(ws.id.clone()), ..f });
        }
    }
    (found, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_transcripts() {
        let jsonl = [
            r#"{"type":"user","sessionId":"s1","cwd":"/r/app","gitBranch":"feat/x","timestamp":"2026-09-28T10:00:00Z","message":{"role":"user","content":"Switch the cache to Redis, the in-memory one loses data on deploy."}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"Moving the cache to Redis."},{"type":"tool_use","name":"Edit","input":{"file_path":"/r/app/src/cache.ts"}},{"type":"tool_use","name":"Grep","input":{"pattern":"cache"}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}"#,
            r#"{"type":"ai-title","aiTitle":"Redis cache"}"#,
        ]
        .join("\n");
        let t = parse_claude(&jsonl);
        assert_eq!((t.id.as_str(), t.branch.as_deref(), t.title.as_deref()), ("s1", Some("feat/x"), Some("Redis cache")));
        let roles: Vec<Role> = t.turns.iter().map(|x| x.role).collect();
        assert_eq!(roles, vec![Role::User, Role::Agent, Role::Tool], "{:?}", t.turns);
        let text = render(&t, Path::new("/r/app"), &[]);
        assert!(text.contains("→ edit src/cache.ts") && !text.contains("/r/app/") && !text.contains("hmm"), "{text}");
    }

    #[test]
    fn codex_transcripts() {
        let jsonl = [
            r#"{"type":"session_meta","payload":{"id":"019f","cwd":"/r/app","git":{"branch":"main"},"timestamp":"2026-09-28T10:00:00Z"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<permissions instructions>…"}]}}"#,
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /r/app"}]}}"##,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Page by cursor, not by offset: offsets shift while rows arrive."}]}}"#,
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"const r = await tools.exec_command({cmd:\"git commit -m 'fix: page by cursor'\"}); await tools.mcp__kontext__ctx_capture({kind:'decision'})"}}"#,
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: src/page.ts\n@@\n*** End Patch"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Paging now uses a cursor. token=ghp_abcdefghijklmnopqrstuvwxyz0123456789AB"}]}}"#,
        ]
        .join("\n");
        let t = parse_codex(&jsonl);
        assert_eq!((t.id.as_str(), t.branch.as_deref()), ("019f", Some("main")));
        let text = render(&t, Path::new("/r/app"), &[]);
        assert!(text.contains("User: Page by cursor") && !text.contains("AGENTS.md") && !text.contains("permissions"), "{text}");
        assert!(text.contains("→ $ git commit -m 'fix: page by cursor'") && text.contains("→ kontext.ctx_capture"), "{text}");
        assert!(text.contains("→ edit src/page.ts") && !text.contains("ghp_abcdef"), "{text}");
    }

    #[test]
    fn slugs_and_parts() {
        assert_eq!(claude_slug("/Users/me/.superset/worktrees/app/calm-river"), "-Users-me--superset-worktrees-app-calm-river");
        let text = "a\n\n".repeat(10) + &"b\n\n".repeat(10);
        let p = parts(&text, 12);
        assert!(p.len() > 2 && p.iter().all(|x| x.len() <= 12) && p.concat() == text);
    }
}
