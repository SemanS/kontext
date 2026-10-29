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
        // the root's own project dir, and those of its subdirectories (`<slug>-apps-web`), whose
        // threads are confirmed by the directory they ran in
        let mut copies: Vec<(PathBuf, bool)> = Vec::new();
        for d in claude_dirs() {
            let Ok(projects) = std::fs::read_dir(d.join("projects")) else { continue };
            for proj in projects.flatten() {
                let name = proj.file_name().to_string_lossy().to_string();
                let exact = name == slug;
                if !exact && !name.starts_with(&format!("{slug}-")) {
                    continue;
                }
                let Ok(files) = std::fs::read_dir(proj.path()) else { continue };
                copies
                    .extend(files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).map(|p| (p, exact)));
            }
        }
        // a session copied between accounts: keep its largest copy
        copies.sort_by_key(|(p, _)| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
        for (p, exact) in copies {
            let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let mut found = Found { agent: Agent::Claude, id, path: p, cwd: Some(root.clone()), workspace: None };
            if !exact {
                // `<slug>-x` is also the slug of a sibling such as `<root>-x`: the thread's cwd decides
                match thread_cwd(&found) {
                    Some(cwd) if within(&cwd, roots) => found.cwd = Some(cwd),
                    _ => continue,
                }
            }
            if seen.insert(format!("claude:{}", found.id)) {
                out.push((mtime(&found.path), found));
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

/// A thread named by id or file must have run in this repository (one of `roots`), or its
/// knowledge would land in the wrong inbox and its transcript go to this repository's model;
/// `force` takes it anyway. A thread that does not say where it ran passes.
pub fn check_belongs(f: &Found, roots: &[String], force: bool) -> Result<()> {
    let Some(cwd) = f.cwd.clone().or_else(|| thread_cwd(f)) else { return Ok(()) };
    if force || within(&cwd, roots) {
        return Ok(());
    }
    bail!("thread {} ran in {cwd}, outside this repository: distill it there, or pass --force to take it here", f.label())
}

/// The redaction patterns for text about to leave this machine, from every config layer; an error
/// when any of them does not compile (nothing is sent until it is fixed).
#[allow(dead_code)] // wired by the integrator
pub fn redaction(loaded: &crate::config::LoadedConfig) -> Result<Vec<regex::Regex>> {
    crate::secrets::redaction(loaded)
}

/// [`render`] with the configuration's redaction patterns, refusing when one does not compile:
/// what ctx_threads, distill and autopilot call.
#[allow(dead_code)] // wired by the integrator
pub fn render_checked(t: &Thread, repo_root: &Path, loaded: &crate::config::LoadedConfig) -> Result<String> {
    Ok(render(t, repo_root, &redaction(loaded)?))
}

// ------------------------------------------------------------------- what distill has seen
// wired into distill.rs by the integrator

/// Per clone, what `kontext distill` already sent to a model: by thread (`claude:<id>`), the size
/// it had and the hashes of the parts processed. Kept in `.git/kontext/distilled.json`, never
/// committed.
#[allow(dead_code)] // wired by the integrator
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Distilled {
    #[serde(default)]
    pub threads: std::collections::BTreeMap<String, DistilledThread>,
}

#[allow(dead_code)] // wired by the integrator
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DistilledThread {
    /// The transcript's size in bytes when it was last distilled.
    pub size: u64,
    /// Hashes of the rendered parts processed.
    pub parts: Vec<String>,
    pub at: String,
}

/// The key a thread is recorded under.
#[allow(dead_code)] // wired by the integrator
pub fn distilled_key(f: &Found) -> String {
    format!("{}:{}", f.agent.as_str(), f.id)
}

/// The hash a rendered part is recorded under.
#[allow(dead_code)] // wired by the integrator
pub fn part_hash(part: &str) -> String {
    util::short_hash(part)
}

#[allow(dead_code)] // wired by the integrator
fn distilled_path(repo: &crate::repo::Repo) -> PathBuf {
    repo.state_dir().join("distilled.json")
}

#[allow(dead_code)] // wired by the integrator
impl Distilled {
    /// This clone's record (empty when there is none or it does not parse).
    pub fn load(repo: &crate::repo::Repo) -> Distilled {
        std::fs::read_to_string(distilled_path(repo)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, repo: &crate::repo::Repo) -> Result<()> {
        let path = distilled_path(repo);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        util::write_atomic(&path, &serde_json::to_string_pretty(self)?)
    }

    /// The thread has not grown since it was distilled: nothing new to send.
    pub fn unchanged(&self, key: &str, size: u64) -> bool {
        self.threads.get(key).is_some_and(|t| t.size == size)
    }

    /// Whether this part of the thread was processed already.
    pub fn seen(&self, key: &str, part: &str) -> bool {
        let h = part_hash(part);
        self.threads.get(key).is_some_and(|t| t.parts.contains(&h))
    }

    /// Record parts of a thread as processed, at its current size.
    pub fn record(&mut self, key: &str, size: u64, parts: &[String]) {
        let t = self.threads.entry(key.to_string()).or_default();
        t.size = size;
        t.at = util::now_iso();
        for h in parts.iter().map(|p| part_hash(p)) {
            if !t.parts.contains(&h) {
                t.parts.push(h);
            }
        }
    }
}

/// How a prompt names the repository: its path without the host (`team/app` for
/// `gitlab.client.internal/team/app`), so an internal git host does not leave the machine.
#[allow(dead_code)] // wired by the integrator
pub fn prompt_repo_name(repo_id: &str) -> String {
    let id = repo_id.trim_matches('/');
    match id.split_once('/') {
        Some((host, rest)) if host.contains('.') || host.contains(':') || host == "local" => rest.to_string(),
        _ => id.to_string(),
    }
}

/// The `k` titles of `known` that share the most words with `text`: what a prompt needs to avoid
/// duplicates, instead of the whole index.
#[allow(dead_code)] // wired by the integrator
pub fn relevant_titles(known: &[String], text: &str, k: usize) -> Vec<String> {
    let words = |s: &str| -> HashSet<String> {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 3).map(|w| w.to_lowercase()).collect()
    };
    let have = words(text);
    let mut scored: Vec<(usize, &String)> =
        known.iter().map(|t| (words(t).intersection(&have).count(), t)).filter(|(n, _)| *n > 0).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    scored.into_iter().take(k).map(|(_, t)| t.clone()).collect()
}

/// Titles of the candidates someone dropped from the inbox, for distill's `known` list: what was
/// dismissed is not proposed again.
#[allow(dead_code)] // wired by the integrator
pub fn dismissed_titles(repo: &crate::repo::Repo) -> Vec<String> {
    let dir = crate::inbox::Inbox::open(repo).dir.join(".dismissed");
    let Ok(files) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<String> = files
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "md"))
        .filter_map(|f| std::fs::read_to_string(f.path()).ok())
        .map(|text| crate::store::Entry::parse("inbox/dismissed.md", &text, "learning").title)
        .filter(|t| !t.trim().is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
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
        || t.starts_with("<recommended_plugins>")
        || t.starts_with("<artifact-content-authored-by-others")
}

/// The value of `<name>…</name>` in a harness block.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let a = text.find(&open)? + open.len();
    let b = text[a..].find(&close)? + a;
    Some(text[a..b].trim())
}

/// Claude Code reports a finished background task (a subagent, a shell) as a `<task-notification>`
/// in a user turn: ids, an output file, a note, usage and the whole result. Nobody typed it, and in
/// threads that fan work out it was most of every part, so it becomes one line of what happened.
fn task_notifications(text: &str) -> (Vec<String>, String) {
    const OPEN: &str = "<task-notification>";
    const CLOSE: &str = "</task-notification>";
    let mut lines = Vec::new();
    let mut rest = String::new();
    let mut s = text;
    while let Some(a) = s.find(OPEN) {
        let Some(len) = s[a..].find(CLOSE) else { break };
        let block = &s[a..a + len + CLOSE.len()];
        rest.push_str(&s[..a]);
        let summary = tag(block, "summary").map(util::one_line).unwrap_or_else(|| "background task".into());
        let status =
            tag(block, "status").filter(|st| !st.is_empty() && *st != "completed").map(|st| format!(" ({st})")).unwrap_or_default();
        let result = tag(block, "result").map(|r| util::truncate_chars(&util::one_line(r), 240)).filter(|r| !r.is_empty());
        lines.push(match result {
            Some(r) => format!("{summary}{status}: {r}"),
            None => format!("{summary}{status}"),
        });
        s = &s[a + len + CLOSE.len()..];
    }
    rest.push_str(s);
    (lines, rest)
}

/// A user turn of a Claude Code transcript: harness notifications become tool lines, injected
/// context is dropped, the rest is what the user wrote.
fn claude_user_text(turns: &mut Vec<Turn>, text: &str) {
    let (notes, rest) = task_notifications(text);
    for n in notes {
        push(turns, Role::Tool, n);
    }
    // a `!` command the user ran: the command is a tool line, its output is dropped
    if let Some(cmd) = tag(&rest, "bash-input") {
        push(turns, Role::Tool, format!("$ {}", util::truncate_chars(&util::one_line(cmd), 200)));
        let (open, close) = ("<bash-input>", "</bash-input>");
        if let (Some(a), Some(b)) = (rest.find(open), rest.find(close)) {
            let mut rest = rest.clone();
            rest.replace_range(a..b + close.len(), "");
            if !injected(&rest) {
                push(turns, Role::User, strip_blocks(&rest));
            }
            return;
        }
    }
    if !injected(&rest) {
        push(turns, Role::User, strip_blocks(&rest));
    }
}

/// Drop `<system-reminder>…</system-reminder>` and similar blocks from a message, and the output of
/// shell commands the user ran with `!` (`<bash-stdout>`, `<bash-stderr>`): tool output never reaches
/// the model.
fn strip_blocks(text: &str) -> String {
    let mut s = text.to_string();
    for tag in ["system-reminder", "local-command-stdout", "local-command-stderr", "bash-stdout", "bash-stderr"] {
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
        // a /compact summary repeats earlier turns and nobody typed it
        if e["isSidechain"] == true || e["isMeta"] == true || e["isCompactSummary"] == true || e["isVisibleInTranscriptOnly"] == true {
            continue;
        }
        let msg = &e["message"];
        match e["type"].as_str() {
            Some("user") => match &msg["content"] {
                Value::String(s) => claude_user_text(&mut t.turns, s),
                Value::Array(blocks) => {
                    for b in blocks {
                        if b["type"] == "text"
                            && let Some(s) = b["text"].as_str()
                        {
                            claude_user_text(&mut t.turns, s);
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
    // Claude Code's per-session temp dirs (scratchpads, task output) carry no meaning for a reader
    static SESSION_TMP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?:/private)?/tmp/claude-\d+/[^/\s]+/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/").unwrap()
    });
    let s = SESSION_TMP.replace_all(&s, "<tmp>/").to_string();
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

    fn loaded(redact: &[&str]) -> crate::config::LoadedConfig {
        let mut cfg = crate::config::Config::default();
        cfg.secrets.redact = redact.iter().map(|s| s.to_string()).collect();
        crate::config::LoadedConfig { cfg, warnings: Vec::new(), shared_path: None, untrusted_adapters: None, layers: Vec::new() }
    }

    #[test]
    fn shell_output_and_compaction_summaries_never_reach_the_model() {
        let jsonl = [
            r#"{"type":"user","sessionId":"s1","cwd":"/r/app","message":{"role":"user","content":"<bash-input>env | grep DB</bash-input>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<bash-stdout>DB_URL=postgres://u:hunter2pass@db/prod</bash-stdout><bash-stderr></bash-stderr>"}}"#,
            r#"{"type":"user","isCompactSummary":true,"isVisibleInTranscriptOnly":true,"message":{"role":"user","content":"This session is being continued from a previous conversation. Summary: the customer Jane Roe…"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"Use the read replica for reports."}}"#,
        ]
        .join("\n");
        let t = parse_claude(&jsonl);
        let turns: Vec<(Role, &str)> = t.turns.iter().map(|x| (x.role, x.text.as_str())).collect();
        assert_eq!(turns, vec![(Role::Tool, "$ env | grep DB"), (Role::User, "Use the read replica for reports.")]);
    }

    #[test]
    fn threads_refuses_invalid_redact_pattern() {
        let t = Thread {
            agent: Agent::Text,
            id: "x".into(),
            cwd: None,
            branch: None,
            title: None,
            started: None,
            turns: vec![Turn { role: Role::User, text: "Meeting with Acme Corp about INTEG-42.".into() }],
        };
        let err = render_checked(&t, Path::new("/r"), &loaded(&["\\bAcme Corp(\\b", "INTEG-\\d+"])).unwrap_err();
        assert!(format!("{err:#}").contains("does not compile"), "{err:#}");
        let ok = render_checked(&t, Path::new("/r"), &loaded(&["\\bAcme Corp\\b", "INTEG-\\d+"])).unwrap();
        assert!(!ok.contains("Acme") && !ok.contains("INTEG-42"), "{ok}");
    }

    #[test]
    fn a_thread_from_another_repository_needs_force() {
        let roots = vec!["/r/app".to_string()];
        let here =
            Found { agent: Agent::Claude, id: "a".into(), path: PathBuf::from("/nope"), cwd: Some("/r/app/web".into()), workspace: None };
        let there = Found { cwd: Some("/r/other".into()), ..here.clone() };
        assert!(check_belongs(&here, &roots, false).is_ok());
        let err = check_belongs(&there, &roots, false).unwrap_err().to_string();
        assert!(err.contains("ran in /r/other") && err.contains("--force"), "{err}");
        assert!(check_belongs(&there, &roots, true).is_ok());
        let unknown = Found { cwd: None, agent: Agent::Text, ..here };
        assert!(check_belongs(&unknown, &roots, false).is_ok(), "a file that does not say where it ran");
    }

    #[test]
    fn prompts_name_the_repository_without_its_host_and_only_related_titles() {
        assert_eq!(prompt_repo_name("gitlab.client.internal/team/app"), "team/app");
        assert_eq!(prompt_repo_name("local/app-1234abcd"), "app-1234abcd");
        assert_eq!(prompt_repo_name("app"), "app");
        let known = vec!["Money is integer cents".to_string(), "Routes live in one file".to_string(), "Cents round half-even".to_string()];
        assert_eq!(
            relevant_titles(&known, "We store money as cents everywhere", 5),
            vec!["Money is integer cents", "Cents round half-even"]
        );
        assert!(relevant_titles(&known, "nothing in common", 5).is_empty());
    }

    #[test]
    fn distill_remembers_threads_and_parts() {
        let mut d = Distilled::default();
        assert!(!d.unchanged("claude:a", 10) && !d.seen("claude:a", "part one"));
        d.record("claude:a", 10, &["part one".to_string()]);
        assert!(d.unchanged("claude:a", 10) && !d.unchanged("claude:a", 20));
        assert!(d.seen("claude:a", "part one") && !d.seen("claude:a", "part two"));
        d.record("claude:a", 20, &["part one".to_string(), "part two".to_string()]);
        assert_eq!(d.threads["claude:a"].parts.len(), 2, "a part is recorded once");
        let back: Distilled = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }

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
    fn harness_notifications_become_one_line() {
        let note = "<task-notification>\n<task-id>af8bbe36</task-id>\n<tool-use-id>toolu_01</tool-use-id>\n<output-file>/private/tmp/claude-501/-Users-me-app/d80322cb-9c09-4576-b45e-ed00b3c5f286/tasks/af8bbe36.output</output-file>\n<status>completed</status>\n<summary>Agent \"Batch validator 14/21\" finished</summary>\n<note>A task-notification fires each time this agent stops.</note>\n<result>All 11 batches validated: valid JSON, no duplicate ids.\n\n**Summary**\n- Batch 45: 4 nodes</result>\n<usage><subagent_tokens>168403</subagent_tokens></usage>\n</task-notification>";
        let jsonl = [
            serde_json::json!({"type":"user","sessionId":"s1","cwd":"/r/app","message":{"role":"user","content":"Validate every batch."}}).to_string(),
            serde_json::json!({"type":"user","message":{"role":"user","content":note}}).to_string(),
            serde_json::json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":format!("{note}\nAnd keep going.")}]}}).to_string(),
            serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"node /private/tmp/claude-501/-Users-me-app/d80322cb-9c09-4576-b45e-ed00b3c5f286/scratchpad/check.mjs"}}]}}).to_string(),
        ]
        .join("\n");
        let t = parse_claude(&jsonl);
        let users: Vec<&str> = t.turns.iter().filter(|x| x.role == Role::User).map(|x| x.text.as_str()).collect();
        assert_eq!(users, vec!["Validate every batch.", "And keep going."], "{:?}", t.turns);
        let text = render(&t, Path::new("/r/app"), &[]);
        assert!(text.contains("→ Agent \"Batch validator 14/21\" finished: All 11 batches validated: valid JSON"), "{text}");
        for noise in ["task-id", "toolu_01", "subagent_tokens", "fires each time", "claude-501"] {
            assert!(!text.contains(noise), "{noise} left in: {text}");
        }
        assert!(text.contains("$ node <tmp>/scratchpad/check.mjs"), "{text}");
        let failed =
            task_notifications("<task-notification><status>failed</status><summary>Shell \"tests\" exited</summary></task-notification>").0;
        assert_eq!(failed, vec!["Shell \"tests\" exited (failed)".to_string()]);
    }

    #[test]
    fn slugs_and_parts() {
        assert_eq!(claude_slug("/Users/me/.superset/worktrees/app/calm-river"), "-Users-me--superset-worktrees-app-calm-river");
        let text = "a\n\n".repeat(10) + &"b\n\n".repeat(10);
        let p = parts(&text, 12);
        assert!(p.len() > 2 && p.iter().all(|x| x.len() <= 12) && p.concat() == text);
    }
}
