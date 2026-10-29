//! Wire agent clients to the kontext MCP server and, optionally, to kontext's agent-host hooks
//! (`--hooks`), the maintenance skills, or the plugin manifests generated from the same tables.
use crate::app::App;
use crate::util;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;

pub const TARGETS: &[&str] = &["claude", "claude-hooks", "skills", "codex", "cursor", "opencode", "agents-md"];

const AGENTS_START: &str = "<!-- kontext:start -->";
const AGENTS_END: &str = "<!-- kontext:end -->";

/// How `kontext connect` was asked to write.
#[derive(Debug, Clone, Copy, Default)]
pub struct Opts {
    /// Write the config (default: print what to add).
    pub write: bool,
    /// Also install the agent-host hooks that deliver the brief and the governing entries.
    pub hooks: bool,
    /// Replace files the team edited (skills).
    pub force: bool,
}

pub fn agents_block() -> String {
    format!(
        "{AGENTS_START}\n## Team context (kontext)\n\n\
- Start a task with `ctx_brief` (MCP server `kontext`): decisions, conventions, pitfalls and the module map, within a small token budget. Pass `focus` with the paths you will touch.\n\
- Need more? `ctx_search` (decisions, docs, commit history, connected memory systems) and `ctx_read <uri>` for detail. `ctx_why <path[:line]>` explains why code is the way it is.\n\
- Decided something durable, or hit a non-obvious gotcha? `ctx_capture` it (kind: decision | convention | learning | incident; paths you touched; the command that proved it as `verify`). Personal notes: `visibility: private`.\n\
- Before committing: `ctx_prepare_commit` — it lists relevant inbox candidates, validates knowledge files and promotes the ones that belong to the change so they ship in the same commit and get reviewed with it.\n\
- Working in another worktree, a submodule or a sibling repository? Pass `dir` (a path inside it) so the tools answer from there.\n\
- Shared knowledge lives in `.ai/` and changes only through commits and review — never edit it to win an argument; propose a superseding decision instead.\n{AGENTS_END}\n"
    )
}

// ------------------------------------------------------------------------------------ hook tables

/// Marks the hook entries kontext manages, so a rewrite replaces them instead of adding more.
const HOOK_MARKER: &str = "kontext:managed";

/// The hook command a host runs; a machine without kontext runs nothing.
pub fn hook_command(host: &str) -> String {
    format!("command -v kontext >/dev/null 2>&1 && kontext hook --host {host} 2>/dev/null || true # {HOOK_MARKER}")
}

/// One hook a host runs: the event, its matcher (`None`: every occurrence), the timeout in seconds,
/// and whether it is the `kontext trust` guard (installed with every `--write`) or delivers context
/// (installed with `--hooks`).
#[derive(Debug, Clone, Copy)]
pub struct HookSpec {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub timeout: u32,
    pub guard: bool,
}

const fn spec(event: &'static str, matcher: Option<&'static str>, timeout: u32, guard: bool) -> HookSpec {
    HookSpec { event, matcher, timeout, guard }
}

/// Claude Code (`.claude/settings.json`, the plugin's `hooks/hooks.json`).
pub const CLAUDE_HOOKS: &[HookSpec] = &[
    spec("SessionStart", Some("startup|resume|clear|compact"), 15, false),
    spec("SubagentStart", None, 15, false),
    spec("PreToolUse", Some("Edit|Write|MultiEdit"), 5, false),
    spec("PreToolUse", Some("Bash"), 5, true),
];

/// Codex (`.codex/hooks.json`, Claude Code's schema; runs with `[features] codex_hooks = true`).
pub const CODEX_HOOKS: &[HookSpec] = &[
    spec("SessionStart", Some("startup|resume|clear|compact"), 15, false),
    spec("PreToolUse", Some("apply_patch|Edit|Write"), 5, false),
    spec("PreToolUse", Some("Bash"), 5, true),
];

/// Cursor (`.cursor/hooks.json`, version 1). Edits are answered after the fact (`postToolUse`);
/// the hook itself skips tools that name no file.
pub const CURSOR_HOOKS: &[HookSpec] =
    &[spec("sessionStart", None, 15, false), spec("postToolUse", None, 5, false), spec("beforeShellExecution", None, 5, true)];

fn hook_entry(host: &str, s: &HookSpec) -> Value {
    let cmd = hook_command(host);
    if host == "cursor" {
        let mut e = json!({"command": cmd, "timeout": s.timeout});
        if let Some(m) = s.matcher {
            e["matcher"] = json!(m);
        }
        return e;
    }
    let mut e = json!({"hooks": [{"type": "command", "command": cmd, "timeout": s.timeout}]});
    if let Some(m) = s.matcher {
        e["matcher"] = json!(m);
    }
    e
}

/// The `hooks` object for a host with every hook of its table.
pub fn hooks_value(host: &str, specs: &[HookSpec]) -> Value {
    let mut hooks = serde_json::Map::new();
    for s in specs {
        let list = hooks.entry(s.event.to_string()).or_insert_with(|| json!([]));
        list.as_array_mut().unwrap().push(hook_entry(host, s));
    }
    Value::Object(hooks)
}

fn is_ours(h: &Value) -> bool {
    h.to_string().contains(HOOK_MARKER)
}

/// The SessionStart hook older kontext versions installed; the hook command replaces it.
fn is_legacy(h: &Value) -> bool {
    h.to_string().contains("kontext brief --format claude-hook")
}

/// Merge a host's hooks into its settings: kontext's entry for each (event, matcher) is replaced in
/// place or added, the team's own hooks stay. Context hooks only with `context`.
fn upsert_hooks(v: &mut Value, host: &str, specs: &[HookSpec], context: bool) {
    if host == "cursor" && v.get("version").is_none() {
        v["version"] = json!(1);
    }
    let hooks = obj(v, "hooks");
    for s in specs.iter().filter(|s| s.guard || context) {
        let list = hooks.entry(s.event.to_string()).or_insert_with(|| json!([]));
        if !list.is_array() {
            *list = json!([]);
        }
        let arr = list.as_array_mut().unwrap();
        if s.event == "SessionStart" {
            arr.retain(|h| !is_legacy(h));
        }
        let entry = hook_entry(host, s);
        let same = |h: &Value| is_ours(h) && h.get("matcher").and_then(Value::as_str) == s.matcher;
        match arr.iter().position(same) {
            Some(i) => arr[i] = entry,
            None => arr.push(entry),
        }
    }
}

// ------------------------------------------------------------------------------------ codex

/// Read tools and the ones that write only this clone's inbox and working tree: approved.
pub const CODEX_APPROVE: &[&str] = &[
    "ctx_brief",
    "ctx_search",
    "ctx_read",
    "ctx_why",
    "ctx_log",
    "ctx_threads",
    "ctx_drift",
    "ctx_capture",
    "ctx_challenge",
    "ctx_confirm",
    "ctx_inbox",
    "ctx_prepare_commit",
    "ctx_init_submit",
];

/// `ctx_verify` runs the `verify:` command an entry names, `ctx_init` can bootstrap `.ai/`, and
/// `ctx_api` reaches every operation: Codex asks before each call.
pub const CODEX_PROMPT: &[&str] = &["ctx_verify", "ctx_api", "ctx_init"];

const CODEX_HEAD: &str = "[mcp_servers.kontext]\ncommand = \"kontext\"\nargs = [\"mcp\"]\n# Read tools and the tools that write only this clone's inbox and working tree are approved\n# below. ctx_verify runs the `verify:` command an entry names, ctx_init can bootstrap .ai/ and\n# ctx_api reaches every operation, so those ask, as do tools kontext exposes from adapters.\ndefault_tools_approval_mode = \"prompt\"\n";

fn codex_tool_table(tool: &str) -> String {
    let mode = if CODEX_PROMPT.contains(&tool) { "prompt" } else { "approve" };
    format!("[mcp_servers.kontext.tools.{tool}]\napproval_mode = \"{mode}\"\n")
}

fn codex_tools() -> impl Iterator<Item = &'static str> {
    CODEX_APPROVE.iter().chain(CODEX_PROMPT).copied()
}

fn codex_snippet() -> String {
    let mut s = CODEX_HEAD.to_string();
    for t in codex_tools() {
        s.push('\n');
        s.push_str(&codex_tool_table(t));
    }
    s
}

/// `~/.codex/config.toml` with kontext wired in, or None when it already is. An existing entry
/// keeps its own choices; missing per-tool approvals are added after it.
fn codex_config(current: &str) -> Option<String> {
    let lines: Vec<&str> = current.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == "[mcp_servers.kontext]") else {
        let sep = if current.is_empty() || current.ends_with("\n\n") {
            ""
        } else if current.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        return Some(format!("{current}{sep}{}", codex_snippet()));
    };
    let is_header = |l: &str| l.trim_start().starts_with('[');
    let ours = |l: &str| l.trim().starts_with("[mcp_servers.kontext.");
    // the server's own keys, then its subtables
    let own_end = lines[start + 1..].iter().position(|l| is_header(l)).map_or(lines.len(), |i| start + 1 + i);
    let mut end = own_end;
    while end < lines.len() && ours(lines[end]) {
        end = lines[end + 1..].iter().position(|l| is_header(l)).map_or(lines.len(), |i| end + 1 + i);
    }
    let has_default = lines[start + 1..own_end].iter().any(|l| l.trim_start().starts_with("default_tools_approval_mode"));
    let present: Vec<String> = lines[start..end].iter().filter(|l| ours(l)).map(|l| l.trim().to_string()).collect();
    let missing: Vec<&str> = codex_tools().filter(|t| !present.contains(&format!("[mcp_servers.kontext.tools.{t}]"))).collect();
    if has_default && missing.is_empty() {
        return None;
    }
    let mut out: Vec<String> = lines[..=start].iter().map(|l| l.to_string()).collect();
    if !has_default {
        out.push("default_tools_approval_mode = \"prompt\"".into());
    }
    out.extend(lines[start + 1..end].iter().map(|l| l.to_string()));
    if !missing.is_empty() {
        while out.last().is_some_and(|l| l.trim().is_empty()) {
            out.pop();
        }
        for t in &missing {
            out.push(String::new());
            out.extend(codex_tool_table(t).lines().map(str::to_string));
        }
        if end < lines.len() {
            out.push(String::new());
        }
    }
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    Some(out.join("\n") + if current.ends_with('\n') || end == lines.len() { "\n" } else { "" })
}

/// The approval mode an existing kontext entry sets for every tool without a table of its own.
fn codex_default_mode(config: &str) -> Option<String> {
    let start = config.lines().position(|l| l.trim() == "[mcp_servers.kontext]")?;
    config
        .lines()
        .skip(start + 1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .find_map(|l| l.trim().strip_prefix("default_tools_approval_mode"))
        .map(|rest| rest.trim_start_matches([' ', '=']).trim().trim_matches('"').to_string())
}

// ------------------------------------------------------------------------------------ files

fn merge_json_file(path: &Path, patch: impl FnOnce(&mut Value)) -> Result<bool> {
    let mut v: Value = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => {
            serde_json::from_str(&crate::init::scan::strip_jsonc(&t)).with_context(|| format!("{} is not valid JSON", path.display()))?
        }
        _ => json!({}),
    };
    let before = v.clone();
    patch(&mut v);
    if v == before {
        return Ok(false);
    }
    util::write_atomic(path, &(serde_json::to_string_pretty(&v)? + "\n"))?;
    Ok(true)
}

fn obj<'a>(v: &'a mut Value, key: &str) -> &'a mut serde_json::Map<String, Value> {
    if !v.get(key).is_some_and(|x| x.is_object()) {
        v[key] = json!({});
    }
    v[key].as_object_mut().unwrap()
}

/// Write a host's hooks file; the line `connect` prints about it.
fn write_hooks(root: &Path, rel: &str, host: &str, specs: &[HookSpec], context: bool) -> Result<String> {
    let changed = merge_json_file(&root.join(rel), |v| upsert_hooks(v, host, specs, context))?;
    let what = if context { "hooks (context on session start and edits, `kontext trust` guard)" } else { "`kontext trust` guard" };
    Ok(format!("{rel}: {} {what}", if changed { "wrote" } else { "already has the" }))
}

fn hooks_note(host: &str, specs: &[HookSpec], rel: &str) -> String {
    format!(
        "With --hooks, kontext also writes into {rel}:\n{}",
        serde_json::to_string_pretty(&json!({"hooks": hooks_value(host, specs)})).unwrap_or_default()
    )
}

const MCP_SERVER: &str = "{ \"mcpServers\": { \"kontext\": { \"command\": \"kontext\", \"args\": [\"mcp\"] } } }";

pub fn connect(app: &App, target: &str, write: bool) -> Result<String> {
    connect_with(app, target, Opts { write, ..Default::default() })
}

pub fn connect_with(app: &App, target: &str, opts: Opts) -> Result<String> {
    let root = &app.repo.root;
    let write = opts.write;
    match target {
        "claude" => {
            if !write {
                let mut s = format!(
                    "Add to .mcp.json (project scope, shared with the team):\n{MCP_SERVER}\nor for yourself only: claude mcp add --scope user kontext -- kontext mcp\n`--write` also adds a PreToolUse hook on Bash to .claude/settings.json that refuses an agent's `kontext trust`."
                );
                if opts.hooks {
                    s.push('\n');
                    s.push_str(&hooks_note("claude", CLAUDE_HOOKS, ".claude/settings.json"));
                }
                return Ok(s);
            }
            let changed = merge_json_file(&root.join(".mcp.json"), |v| {
                obj(v, "mcpServers").insert("kontext".into(), json!({"command": "kontext", "args": ["mcp"]}));
            })?;
            let hooks = write_hooks(root, ".claude/settings.json", "claude", CLAUDE_HOOKS, opts.hooks)?;
            Ok(format!(
                "claude: {} .mcp.json (server `kontext`; Claude Code asks each teammate to approve it once)\n{hooks}",
                if changed { "wrote" } else { "already configured in" }
            ))
        }
        "claude-hooks" => {
            if !write {
                return Ok(hooks_note("claude", CLAUDE_HOOKS, ".claude/settings.json")
                    .replace("With --hooks, kontext also writes", "kontext writes"));
            }
            let line = write_hooks(root, ".claude/settings.json", "claude", CLAUDE_HOOKS, true)?;
            Ok(format!("claude-hooks: {line}"))
        }
        "skills" => crate::skills::connect(root, write, opts.force),
        "cursor" => {
            if !write {
                let mut s = format!(
                    "Add to .cursor/mcp.json: {MCP_SERVER}\n`--write` also adds a beforeShellExecution hook to .cursor/hooks.json that refuses an agent's `kontext trust`."
                );
                if opts.hooks {
                    s.push('\n');
                    s.push_str(&hooks_note("cursor", CURSOR_HOOKS, ".cursor/hooks.json"));
                }
                return Ok(s);
            }
            let changed = merge_json_file(&root.join(".cursor").join("mcp.json"), |v| {
                obj(v, "mcpServers").insert("kontext".into(), json!({"command": "kontext", "args": ["mcp"]}));
            })?;
            let hooks = write_hooks(root, ".cursor/hooks.json", "cursor", CURSOR_HOOKS, opts.hooks)?;
            Ok(format!("cursor: {} .cursor/mcp.json\n{hooks}", if changed { "wrote" } else { "already configured in" }))
        }
        "opencode" => {
            let path = root.join("opencode.json");
            if !write {
                return Ok("Add to opencode.json: { \"mcp\": { \"kontext\": { \"type\": \"local\", \"command\": [\"kontext\", \"mcp\"], \"enabled\": true } } }".into());
            }
            let changed = merge_json_file(&path, |v| {
                if v.get("$schema").is_none() {
                    v["$schema"] = json!("https://opencode.ai/config.json");
                }
                obj(v, "mcp").insert("kontext".into(), json!({"type": "local", "command": ["kontext", "mcp"], "enabled": true}));
            })?;
            Ok(format!("opencode: {} opencode.json", if changed { "wrote" } else { "already configured in" }))
        }
        "codex" => connect_codex(root, opts),
        "agents-md" => connect_agents_md(root, write),
        // kontext's own repository: the plugin manifests, generated from the tables above
        "plugin" => {
            let files = plugin_files();
            if !write {
                let map: serde_json::Map<String, Value> = files.into_iter().map(|(p, t)| (p.to_string(), Value::String(t))).collect();
                return Ok(serde_json::to_string_pretty(&Value::Object(map))?);
            }
            if !root.join("skills").join("kontext-audit").join("SKILL.md").is_file() {
                bail!("the plugin files are generated in kontext's own repository (its skills/ directory is missing here)");
            }
            let mut wrote = Vec::new();
            for (rel, text) in files {
                let p = root.join(rel);
                if std::fs::read_to_string(&p).ok().as_deref() != Some(text.as_str()) {
                    util::write_atomic(&p, &text)?;
                    wrote.push(rel);
                }
            }
            Ok(if wrote.is_empty() { "plugin: files are current".into() } else { format!("plugin: wrote {}", wrote.join(", ")) })
        }
        other => bail!("unknown target '{other}' (use one of: {})", TARGETS.join(", ")),
    }
}

fn connect_codex(root: &Path, opts: Opts) -> Result<String> {
    // Codex (and wrappers that pick an account for it) honour CODEX_HOME
    let home = std::env::var_os("CODEX_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| util::home_dir().join(".codex"));
    let path = home.join("config.toml");
    let shown = path.display().to_string().replace(&util::home_dir().display().to_string(), "~");
    if !opts.write {
        let mut s = format!(
            "Add to {shown} (user level; run `kontext connect codex --write` to append it):\n{}\n`--write` also adds a PreToolUse hook on Bash to .codex/hooks.json that refuses an agent's `kontext trust` (Codex runs project hooks with `[features] codex_hooks = true`).",
            codex_snippet()
        );
        if opts.hooks {
            s.push('\n');
            s.push_str(&hooks_note("codex", CODEX_HOOKS, ".codex/hooks.json"));
        }
        return Ok(s);
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out = Vec::new();
    match codex_config(&current) {
        None => out.push(format!("codex: {shown} already has [mcp_servers.kontext] with per-tool approvals")),
        Some(next) => {
            let backup = path.exists();
            if backup {
                std::fs::copy(&path, path.with_extension(format!("toml.bak-kontext-{}", chrono::Local::now().format("%Y%m%d%H%M%S"))))?;
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            util::write_atomic(&path, &next)?;
            let note = if backup { " (backup kept next to it)" } else { "" };
            out.push(if current.contains("[mcp_servers.kontext]") {
                format!("codex: added per-tool approvals to [mcp_servers.kontext] in {shown}{note}")
            } else {
                format!("codex: appended [mcp_servers.kontext] to {shown}{note}")
            });
        }
    }
    let after = std::fs::read_to_string(&path).unwrap_or_default();
    if codex_default_mode(&after).as_deref() == Some("approve") {
        out.push(format!(
            "warning: {shown} keeps default_tools_approval_mode = \"approve\": tools kontext exposes from adapters run without asking, and a Codex without per-tool approval_mode also runs {} without asking; set it to \"prompt\" to be asked",
            CODEX_PROMPT.join(", ")
        ));
    }
    out.push(write_hooks(root, ".codex/hooks.json", "codex", CODEX_HOOKS, opts.hooks)?);
    if opts.hooks {
        out.push("Codex runs project hooks with `[features] codex_hooks = true` in its config.toml.".into());
    }
    Ok(out.join("\n"))
}

fn has_import(text: &str) -> bool {
    text.lines().any(|l| matches!(l.trim(), "@AGENTS.md" | "@./AGENTS.md"))
}

fn with_block(current: &str) -> String {
    match (current.find(AGENTS_START), current.find(AGENTS_END)) {
        (Some(s), Some(e)) if e > s => {
            format!("{}{}{}", &current[..s], agents_block(), current[e + AGENTS_END.len()..].trim_start_matches('\n'))
        }
        _ => {
            let sep = if current.is_empty() || current.ends_with("\n\n") {
                ""
            } else if current.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            format!("{current}{sep}{}", agents_block())
        }
    }
}

/// CLAUDE.md that imports AGENTS.md instead of carrying its own copy of the kontext block.
fn claude_md_importing(current: &str) -> String {
    let mut text = match (current.find(AGENTS_START), current.find(AGENTS_END)) {
        (Some(s), Some(e)) if e > s => {
            format!("{}{}", &current[..s], current[e + AGENTS_END.len()..].trim_start_matches('\n'))
        }
        _ => current.to_string(),
    };
    if !has_import(&text) {
        let t = text.trim_end_matches('\n');
        text = if t.is_empty() { "@AGENTS.md\n".to_string() } else { format!("{t}\n\n@AGENTS.md\n") };
    }
    text
}

/// AGENTS.md carries the kontext block; an existing CLAUDE.md imports it with `@AGENTS.md`.
fn connect_agents_md(root: &Path, write: bool) -> Result<String> {
    let agents = root.join("AGENTS.md");
    let claude = ["CLAUDE.md", "Claude.md"].iter().map(|f| (*f, root.join(f))).find(|(_, p)| p.is_file());
    // CLAUDE.md that is AGENTS.md (a link) needs nothing of its own
    let claude = claude.filter(|(_, p)| match (p.canonicalize(), agents.canonicalize()) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    });
    if !write {
        let mut s = format!("Append to AGENTS.md:\n\n{}", agents_block());
        if let Some((name, _)) = &claude {
            s.push_str(&format!("\nand add an `@AGENTS.md` line to {name} so Claude Code reads the same text."));
        }
        return Ok(s);
    }
    let mut out = Vec::new();
    let current = std::fs::read_to_string(&agents).unwrap_or_default();
    let next = with_block(&current);
    if next == current {
        out.push("agents-md: AGENTS.md already has the kontext block".to_string());
    } else {
        util::write_atomic(&agents, &next)?;
        out.push("agents-md: wrote the kontext block into AGENTS.md".to_string());
    }
    if let Some((name, p)) = claude {
        let current = std::fs::read_to_string(&p)?;
        let next = claude_md_importing(&current);
        if next != current {
            util::write_atomic(&p, &next)?;
            out.push(format!("agents-md: {name} imports AGENTS.md (`@AGENTS.md`) instead of a copy of the block"));
        }
    }
    Ok(out.join("\n"))
}

// ------------------------------------------------------------------------------------ plugins

/// The plugin files in kontext's own repository, generated from the tables above: a Claude Code
/// plugin and marketplace (`.claude-plugin/`, with `.mcp.json`, `hooks/hooks.json` and `skills/`
/// at the plugin root, where Claude Code finds them) and a Codex plugin (`.codex-plugin/`).
/// `kontext connect plugin --write` regenerates them; a test keeps them equal.
pub fn plugin_files() -> Vec<(&'static str, String)> {
    let pretty = |v: Value| serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    let version = env!("CARGO_PKG_VERSION");
    let description = env!("CARGO_PKG_DESCRIPTION");
    let author = json!({"name": "SemanS"});
    let homepage = env!("CARGO_PKG_HOMEPAGE");
    let repository = env!("CARGO_PKG_REPOSITORY");
    let keywords = json!(["mcp", "agents", "adr", "git", "knowledge"]);
    vec![
        (
            ".claude-plugin/plugin.json",
            pretty(json!({
                "name": "kontext",
                "version": version,
                "description": description,
                "author": author,
                "homepage": homepage,
                "repository": repository,
                "license": "MIT OR Apache-2.0",
                "keywords": keywords,
            })),
        ),
        (
            ".claude-plugin/marketplace.json",
            pretty(json!({
                "name": "kontext",
                "owner": author,
                "plugins": [{"name": "kontext", "source": "./", "description": description, "version": version}],
            })),
        ),
        (
            ".codex-plugin/plugin.json",
            pretty(json!({
                "name": "kontext",
                "version": version,
                "description": description,
                "author": author,
                "homepage": homepage,
                "repository": repository,
                "license": "MIT OR Apache-2.0",
                "keywords": keywords,
                "skills": "./skills/",
                "mcpServers": "./.mcp.json",
                "hooks": "./hooks/codex-hooks.json",
            })),
        ),
        (".mcp.json", pretty(json!({"mcpServers": {"kontext": {"command": "kontext", "args": ["mcp"]}}}))),
        (
            "hooks/hooks.json",
            pretty(json!({
                "description": "kontext: the brief on session start, the governing entries on edits, a guard on an agent's `kontext trust`",
                "hooks": hooks_value("claude", CLAUDE_HOOKS),
            })),
        ),
        (
            "hooks/codex-hooks.json",
            pretty(json!({
                "description": "kontext: the brief on session start, the governing entries on edits, a guard on an agent's `kontext trust`",
                "hooks": hooks_value("codex", CODEX_HOOKS),
            })),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_is_added_once() {
        let fresh = codex_config("model = \"o3\"\n").unwrap();
        assert!(fresh.starts_with("model = \"o3\"\n\n[mcp_servers.kontext]\n"));
        assert!(fresh.contains("default_tools_approval_mode = \"prompt\""));
        assert!(fresh.contains("[mcp_servers.kontext.tools.ctx_verify]\napproval_mode = \"prompt\""));
        assert!(fresh.contains("[mcp_servers.kontext.tools.ctx_brief]\napproval_mode = \"approve\""));
        assert!(!fresh.contains("write only the local inbox and the working tree;"), "the old claim is gone");
        assert!(fresh.contains("ctx_verify runs the `verify:` command"));
        assert_eq!(codex_config(&fresh), None);
        // an entry written by an older kontext keeps its choice and gets the per-tool approvals
        let old = "[mcp_servers.kontext]\ncommand = \"kontext\"\nargs = [\"mcp\"]\ndefault_tools_approval_mode = \"approve\"\n\n[mcp_servers.other]\nurl = \"x\"\n";
        let upgraded = codex_config(old).unwrap();
        assert!(upgraded.starts_with("[mcp_servers.kontext]\ncommand = \"kontext\"\nargs = [\"mcp\"]\ndefault_tools_approval_mode = \"approve\"\n\n[mcp_servers.kontext.tools.ctx_brief]\n"), "{upgraded}");
        assert!(upgraded.ends_with("approval_mode = \"prompt\"\n\n[mcp_servers.other]\nurl = \"x\"\n"), "{upgraded}");
        assert_eq!(codex_config(&upgraded), None);
        assert_eq!(codex_default_mode(&upgraded).as_deref(), Some("approve"));
        // one without any approval mode gets "prompt"
        let bare = "[mcp_servers.kontext]\ncommand = \"kontext\"\n";
        let next = codex_config(bare).unwrap();
        assert!(next.starts_with("[mcp_servers.kontext]\ndefault_tools_approval_mode = \"prompt\"\ncommand = \"kontext\"\n"), "{next}");
        assert_eq!(codex_config(&next), None);
        // a team's own per-tool choice is kept
        let custom = format!("{}[mcp_servers.kontext.tools.ctx_api]\napproval_mode = \"approve\"\n", CODEX_HEAD);
        let next = codex_config(&custom).unwrap();
        assert_eq!(next.matches("[mcp_servers.kontext.tools.ctx_api]").count(), 1);
        assert!(next.contains("[mcp_servers.kontext.tools.ctx_api]\napproval_mode = \"approve\""));
    }

    #[test]
    fn hooks_are_merged_once_and_keep_the_team_s_own() {
        let mut v = json!({"hooks": {
            "SessionStart": [{"hooks": [{"type": "command", "command": "command -v kontext >/dev/null 2>&1 && kontext brief --format claude-hook 2>/dev/null || true", "timeout": 15}]}],
            "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "./audit.sh"}]}]
        }});
        upsert_hooks(&mut v, "claude", CLAUDE_HOOKS, false);
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2, "the guard joins the team's hook: {v}");
        assert!(v["hooks"].get("SubagentStart").is_none(), "context hooks only with --hooks");
        upsert_hooks(&mut v, "claude", CLAUDE_HOOKS, true);
        let once = v.clone();
        upsert_hooks(&mut v, "claude", CLAUDE_HOOKS, true);
        assert_eq!(v, once, "a second write changes nothing");
        let start = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(start.len(), 1, "the legacy brief hook is replaced: {v}");
        assert_eq!(start[0]["matcher"], "startup|resume|clear|compact");
        assert!(start[0].to_string().contains("kontext hook --host claude"));
        assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 3);
        let mut c = json!({});
        upsert_hooks(&mut c, "cursor", CURSOR_HOOKS, true);
        assert_eq!(c["version"], 1);
        assert!(c["hooks"]["beforeShellExecution"][0]["command"].as_str().unwrap().contains("kontext hook --host cursor"));
    }

    #[test]
    fn claude_md_imports_agents_md() {
        assert_eq!(claude_md_importing("# Rules\n"), "# Rules\n\n@AGENTS.md\n");
        assert_eq!(claude_md_importing("# Rules\n\n@AGENTS.md\n"), "# Rules\n\n@AGENTS.md\n");
        let with_copy = format!("# Rules\n\n{}", agents_block());
        assert_eq!(claude_md_importing(&with_copy), "# Rules\n\n@AGENTS.md\n");
    }

    #[test]
    fn plugin_files_equal_the_generated_output() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let regen = std::env::var("KONTEXT_REGEN_PLUGINS").is_ok_and(|v| v == "1");
        for (rel, text) in plugin_files() {
            let p = root.join(rel);
            if regen {
                util::write_atomic(&p, &text).unwrap();
                continue;
            }
            let on_disk = std::fs::read_to_string(&p).unwrap_or_default();
            assert_eq!(
                on_disk, text,
                "{rel} is out of date: run `kontext connect plugin --write` (or KONTEXT_REGEN_PLUGINS=1 cargo test plugin_files)"
            );
        }
    }
}
