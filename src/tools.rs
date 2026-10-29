//! The agent-facing tool surface (used by the MCP server and by `kontext call`).
use crate::app::App;
use crate::init::deepen;
use crate::model;
use crate::ops::{self, CaptureReq, PrepareReq, SearchReq};
use crate::util;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

/// The server's instructions. Clients may show only the start (and load the tools lazily, behind a
/// tool search): the first 512 characters stand alone — what kontext holds, that its tools are
/// `ctx_*`, that ctx_brief comes first and when to look for the others. The whole stays under 4,096.
pub const INSTRUCTIONS: &str = "kontext holds this repository's reviewed team knowledge: decisions, conventions, learnings, incidents and the module map, plus connected memory and history. \
Its tools are named ctx_* (if they are not loaded, search your tools for \"ctx_\"). Call ctx_brief first in every task, with `focus` set to the paths or topic you will work on. \
Look for ctx_search, ctx_read and ctx_why whenever a task touches a past decision, a convention or a pitfall. \
When you and the user settle something durable (a decision, a convention, a non-obvious pitfall), record it with ctx_capture — short: what, why, consequences, and the paths it governs; a learning or decision you proved with a command carries that command as `verify`. \
Before committing, run ctx_prepare_commit and promote the candidates that belong to the change so they are reviewed together with it. \
Shared knowledge changes only through commits and review; supersede a decision instead of rewriting it. \
Another worktree, submodule or repository? Pass `dir` (a path inside it).";

/// Instructions for a repository without a knowledge store (kontext registered for every
/// repository of a user, but set up only in some). In every session's prompt: keep it short.
pub const NOT_SET_UP_INSTRUCTIONS: &str = "kontext is installed, but this repository has no team knowledge store yet. \
Its tools are named ctx_* (if they are not loaded, search your tools for \"ctx_\"). ctx_brief with `focus`, ctx_search and ctx_why still answer from the repository's docs, history and private notes: call ctx_brief first. \
Capture team knowledge or bootstrap it (ctx_capture, ctx_init) only when the user asks; private notes (visibility private) are fine. \
Another worktree, submodule or repository? Pass `dir` (a path inside it).";

/// Instructions when the server started outside any git repository (a client launched from the
/// home directory): every call needs `dir`, and nothing promises answers it cannot give.
pub const NO_REPO_INSTRUCTIONS: &str = "kontext found no git repository where this server started, so its ctx_* tools answer only with `dir`: \
pass a path inside the project (call ctx_brief with `dir` first). \
Starting the server from the project directory (or with -C <dir>) makes `dir` unnecessary.";

/// The server's instructions for `app`: what it has, and the submodules that keep team knowledge
/// of their own (an agent in camp-bot works in `extractor/` and its decisions live there).
pub fn instructions(app: &App) -> String {
    let mut s = if app.is_set_up() { INSTRUCTIONS.to_string() } else { NOT_SET_UP_INSTRUCTIONS.to_string() };
    let nested: Vec<String> = crate::route::nested_stores(app).into_iter().map(|n| format!("{}/", n.rel)).collect();
    if !nested.is_empty() {
        s.push_str(&format!(
            " {} keep{} team knowledge of {}: start with ctx_brief and focus paths there.",
            nested.join(", "),
            if nested.len() == 1 { "s" } else { "" },
            if nested.len() == 1 { "its own" } else { "their own" }
        ));
    }
    s
}

const NOT_SET_UP: &str = "kontext is not set up in this repository (no knowledge store), so nothing is written into it. \
Ask the user: `kontext init` sets it up (or ctx_init with bootstrap=true when they asked you to). Private notes (visibility: private) still work.";

fn s(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
fn arr(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}
/// A list of strings, or one comma-separated string.
fn strs() -> Value {
    json!({"type": ["array", "string"], "items": {"type": "string"}})
}

/// MCP tool annotations. Clients use them to decide what needs approval: Codex, for one, runs
/// read-only tools without asking and asks (or, with `approval_policy = "never"`, refuses) the rest.
fn annotations(name: &str) -> Value {
    let (title, read_only, destructive) = match name {
        "ctx_api" => ("Structured knowledge API", false, false),
        "ctx_brief" => ("Team brief", true, false),
        "ctx_search" => ("Search team context", true, false),
        "ctx_read" => ("Read a context URI", true, false),
        "ctx_why" => ("Why is this code the way it is", true, false),
        "ctx_log" => ("Decision timeline", true, false),
        "ctx_threads" => ("Read agent threads", true, false),
        "ctx_capture" => ("Capture knowledge", false, false),
        // `drop` discards only local, never-shared inbox drafts — not destructive in the MCP sense,
        // and Codex refuses destructive tools outright under `approval_policy = "never"`
        "ctx_inbox" => ("Knowledge inbox", false, false),
        // a challenge is a local inbox note; it changes no knowledge by itself
        "ctx_challenge" => ("Challenge a knowledge entry", false, false),
        // sets one line of the entry's own file and stages it for review
        "ctx_confirm" => ("Confirm a knowledge entry still holds", false, false),
        // runs a shell command from the repository (pinned commands only): it can change anything
        // the user can and reach the network, so clients ask first (decided: destructive, open-world)
        "ctx_verify" => ("Run a knowledge entry's verify command", false, true),
        "ctx_drift" => ("Knowledge a change left behind", true, false),
        "ctx_prepare_commit" => ("Prepare the commit", false, false),
        "ctx_init" => ("Knowledge bootstrap: next tasks", false, false),
        "ctx_init_submit" => ("Knowledge bootstrap: submit a task", false, false),
        _ => ("", false, false),
    };
    let mut a = json!({"readOnlyHint": read_only, "openWorldHint": matches!(name, "ctx_api" | "ctx_verify")});
    if !title.is_empty() {
        a["title"] = json!(title);
    }
    if !read_only {
        a["destructiveHint"] = json!(destructive);
        a["idempotentHint"] = json!(false);
    }
    a
}

/// Tools that act on one repository and take `dir` for another (every `ctx_` tool accepts it).
const TAKES_DIR: &[&str] = &[
    "ctx_brief",
    "ctx_search",
    "ctx_read",
    "ctx_why",
    "ctx_log",
    "ctx_capture",
    "ctx_challenge",
    "ctx_confirm",
    "ctx_verify",
    "ctx_inbox",
    "ctx_prepare_commit",
    "ctx_drift",
    "ctx_threads",
    "ctx_init",
];

/// Tools whose `dir` stays inside this repository, its worktrees and its submodules: they run
/// commands or change the local inbox and index, so another checkout on disk is not theirs to touch.
const CONFINED_DIR: &[&str] = &["ctx_verify", "ctx_inbox", "ctx_prepare_commit"];

pub fn definitions() -> Vec<Value> {
    let mut defs = tool_list();
    for d in &mut defs {
        let name = d["name"].as_str().unwrap_or("").to_string();
        d["annotations"] = annotations(&name);
        if TAKES_DIR.contains(&name.as_str()) {
            d["inputSchema"]["properties"]["dir"] = if CONFINED_DIR.contains(&name.as_str()) {
                s("Path in a worktree or submodule of this repo")
            } else {
                s("Path in another worktree, submodule or repo")
            };
        }
        if let Some(schema) = output_schema(&name) {
            d["outputSchema"] = schema;
        }
        match name.as_str() {
            // the brief is what every session needs first: never defer it behind a tool search
            "ctx_brief" => d["_meta"] = json!({"anthropic/alwaysLoad": true}),
            "ctx_read" | "ctx_api" => d["_meta"] = json!({"anthropic/maxResultSizeChars": 200_000}),
            _ => {}
        }
    }
    defs
}

/// Whether `name` is one of kontext's own tools.
pub fn is_tool(name: &str) -> bool {
    tool_list().iter().any(|t| t["name"] == name)
}

fn tool_list() -> Vec<Value> {
    vec![
        json!({
            "name": "ctx_api",
            "description": "Versioned structured knowledge API for applications. Read, query, extract and stage knowledge through the same operations as kontext api. Inspect status for capabilities. Knowledge files and staging stay local; configured LLM and event adapters may access external services. Staging does not prove review or publish knowledge.",
            "inputSchema": {"type": "object", "properties": {
                "operation": {"type": "string", "enum": crate::api::OPERATIONS},
                "input": {"type": "object", "description": "Operation-specific input object"},
                "dir": s("Path inside the target repository or worktree")
            }, "required": ["operation"]},
            "outputSchema": crate::api::output_schema(),
            "annotations": {"title": "Structured knowledge API", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true}
        }),
        json!({
            "name": "ctx_brief",
            "description": "Team context for this repository in one small, token-budgeted block: active decisions, conventions, learnings and pitfalls, the module map, agent rules and pending knowledge work. Call at the start of a task; pass `focus` (paths you will touch, or a topic) to rank what matters.",
            "inputSchema": {"type": "object", "properties": {
                "focus": arr("Paths (files/dirs) or topic words to prioritise"),
                "budget_tokens": {"type": "integer", "description": "Approximate size limit (default from config, ~1400)"},
                "adapters": {"type": "boolean", "description": "Include sections from connected adapters with a brief op (default true)"}
            }}
        }),
        json!({
            "name": "ctx_search",
            "description": "Search team knowledge (decisions, conventions, learnings, incidents, module docs), repository docs, commit history, local inbox notes, submodules with their own knowledge and connected memory systems. Compact hits with URIs; open one with ctx_read.",
            "inputSchema": {"type": "object", "properties": {
                "query": s("What you are looking for, in plain words or identifiers"),
                "kinds": arr("Restrict local results: decision, convention, learning, incident, architecture, doc, commit"),
                "sources": arr("Restrict sources: local and/or adapter names (default: all)"),
                "limit": {"type": "integer", "description": "Max hits (default 10)"},
                "adapters": {"type": "boolean", "description": "Ask the connected search adapters even when local hits suffice (true) or never (false); default from [search] adapters"},
                "budget_tokens": {"type": "integer", "description": "Approximate size limit (default 1200)"}
            }, "required": ["query"]}
        }),
        json!({
            "name": "ctx_read",
            "description": "Read a hit by URI: kx:<id> (knowledge entry), file:<path>[#section], git:<sha>, inbox:<id>, or an adapter URI such as viking://… Levels: 0 = one line, 1 = overview (default), 2 = full.",
            "inputSchema": {"type": "object", "properties": {
                "uri": s("URI from ctx_search / ctx_brief, or a plain repo path"),
                "level": {"type": "integer", "enum": [0, 1, 2]}
            }, "required": ["uri"]}
        }),
        json!({
            "name": "ctx_capture",
            "description": "Record something durable: a decision, a convention, a non-obvious learning or pitfall, an incident. Short (what, why, consequences), with the paths it governs: it goes to the repository that owns them. A learning or decision you proved with a command (a test, a grep) carries that command as `verify`, so ctx_verify can prove it again. It waits in the local inbox until promoted into a commit (promote=true writes it into the store now); visibility=private keeps it personal.",
            "inputSchema": {"type": "object", "properties": {
                "kind": {"type": "string", "enum": ["decision", "convention", "learning", "incident"], "description": "What it is (pitfalls are learnings)"},
                "title": s("Short statement, e.g. 'Use Tantivy for local recall'"),
                "body": s("Markdown: context, the decision/lesson, consequences. A few lines. For a decision, an optional `## Alternatives` section names the options considered and why each was rejected."),
                "summary": s("One line (optional; derived from the body otherwise)"),
                "paths": arr("Repo paths or globs this governs, e.g. apps/api/src/billing/**"),
                "tags": arr("Optional tags"),
                "visibility": {"type": "string", "enum": ["team", "private"]},
                "supersedes": arr("Ids of entries it replaces (marked superseded)"),
                "extends": arr("Ids of entries it builds on, e.g. a decision it adds to (they stay as they are)"),
                "commits": arr("Commits it came from (short shas)"),
                "source": s("Where it was found, e.g. thread claude:4f1c2a9b (kept in the inbox only)"),
                "status": s("Decisions: proposed | accepted (default)"),
                "review_by": s("When the entry should be looked at again, YYYY-MM-DD (a temporary rule, a workaround); optional"),
                "verify": s("The command that proved it: one line, at most 500 characters, run with sh from the repository root, e.g. `cargo test -p billing rounding`. Optional"),
                "evidence": {"type": "array", "description": "Sources it rests on: {ref, quote?}. ref is pr:<id>, issue:<id>, task:<id>, code:<repo>@<rev>:<path>#L<a>-<b> or url:<url>; a quote (≤200 chars) of a code ref is checked against that revision", "items": {"type": "object", "properties": {"ref": {"type": "string"}, "quote": {"type": "string"}}, "required": ["ref"]}},
                "promote": {"type": "boolean", "description": "Write into the repo store now instead of the inbox"}
            }, "required": ["kind", "title", "body"]}
        }),
        json!({
            "name": "ctx_challenge",
            "description": "Say that a recorded entry no longer holds, and why. It waits in the local inbox as a challenge; nothing changes until review: ctx_prepare_commit then offers to retire the entry (retire=[id]) or to record a replacement that supersedes it. Never edit the entry in place.",
            "inputSchema": {"type": "object", "properties": {
                "id": s("The entry: kx:<id>, an id or its unique prefix"),
                "why": s("Why it no longer holds: what changed, with a source if there is one"),
                "visibility": {"type": "string", "enum": ["team", "private"]}
            }, "required": ["id", "why"]}
        }),
        json!({
            "name": "ctx_drift",
            "description": "Recorded knowledge a change left behind: active entries whose `paths` cover files changed in base..head (without head: up to HEAD plus uncommitted and untracked changes, which is how a jj working copy looks) and whose own file was not edited or confirmed in the same range. Deterministic; no model. Before committing or pushing, check each one listed.",
            "inputSchema": {"type": "object", "properties": {
                "base": s("Start of the range, a git revision (default: where HEAD left origin's default branch)"),
                "head": s("End of the range, a git revision (default: the working tree)"),
                "kinds": arr("Kinds to check (default decision, convention, learning, incident)")
            }}
        }),
        json!({
            "name": "ctx_confirm",
            "description": "Say that a recorded entry still holds after the code it governs changed: sets its `checked` time to now (UTC), appends an `agent` line (with HEAD and your reason) to the verification ledger and stages both, so the confirmation is reviewed with the change. Freshness and drift count from it. Only when you checked the entry against the code; if it no longer holds, use ctx_challenge or record a replacement. An agent's confirmation is shown as a stamp: when the entry names a `verify` command, ctx_verify proves more.",
            "inputSchema": {"type": "object", "properties": {
                "id": s("The entry: kx:<id>, an id or its unique prefix"),
                "ids": arr("Several entries at once (more than five are recorded as a bulk stamp)"),
                "because": s("One line: what you checked it against")
            }}
        }),
        json!({
            "name": "ctx_verify",
            "description": "Run an entry's `verify` command (one shell command from the repository root, with a timeout); pass or fail is recorded with HEAD in the verification ledger and staged. Only commands the user pinned run; an unpinned one is returned to you, not run. Returns the outcome and the output's end. No `verify` command: read the entry and ctx_confirm it.",
            "inputSchema": {"type": "object", "properties": {
                "id": s("The entry: kx:<id>, an id or its unique prefix"),
                "because": s("One line: why you ran it")
            }, "required": ["id"]}
        }),
        json!({
            "name": "ctx_why",
            "description": "Explain why code is the way it is: knowledge covering a path, its module summary, decision-shaped commits (with blame for path:line), and history from connected session/memory adapters. Also accepts a commit sha, or a symbol/topic.",
            "inputSchema": {"type": "object", "properties": {
                "target": s("path, path:line, path:start-end, commit sha, or symbol"),
                "limit": {"type": "integer"}
            }, "required": ["target"]}
        }),
        json!({
            "name": "ctx_log",
            "description": "The decision timeline (or another kind): date, id, title, status, supersession and the commit that introduced it.",
            "inputSchema": {"type": "object", "properties": {
                "kind": s("decision (default), convention, learning, incident"),
                "all": {"type": "boolean", "description": "Include superseded/rejected entries"},
                "limit": {"type": "integer"}
            }}
        }),
        json!({
            "name": "ctx_threads",
            "description": "This repository's agent threads on this machine (Claude Code, Codex, by Superset workspace) as compact, redacted transcripts to distill knowledge from (prompt kontext-distill). Without `thread`: the recent ones; with it: that transcript, in parts.",
            "inputSchema": {"type": "object", "properties": {
                "thread": s("claude:<id>, codex:<id>, superset:<workspace id or worktree name> (every thread of that workspace), a transcript file, or `last`"),
                "part": {"type": "integer", "description": "Part of a long transcript to return (1-based, default 1)"},
                "budget_chars": {"type": "integer", "description": "Characters per part (default 20000, at most 80000)"},
                "budget_tokens": {"type": "integer", "description": "Or tokens per part (about 4 characters each)"}
            }}
        }),
        json!({
            "name": "ctx_inbox",
            "description": "Local knowledge candidates captured in this clone (not shared yet): list, show, promote (write into the repo store and stage), drop, or verify (set the command that proved a draft, `verify`, on one id; an empty string clears it; promotion keeps it).",
            "inputSchema": {"type": "object", "properties": {
                "action": {"type": "string", "enum": ["list", "show", "promote", "drop", "verify"]},
                "ids": arr("Candidate ids for show/promote/drop/verify (verify: one id)"),
                "verify": s("For action=verify: the command that proved the draft, one line of at most 500 characters run with sh from the repository root; \"\" clears it")
            }}
        }),
        json!({
            "name": "ctx_prepare_commit",
            "description": "Run before committing: the staged change (with nothing staged, the working tree's changes against HEAD, which is how a jj change looks), recorded knowledge it touches (confirm what still holds with ctx_confirm), inbox candidates to ship with it (promote=[ids] writes and stages them, drop=[ids] discards), checks of knowledge files, module docs to refresh, and the commit trailers (and whether a hook adds them).",
            "inputSchema": {"type": "object", "properties": {
                "promote": arr("Inbox ids to promote and stage"),
                "drop": arr("Inbox ids to discard"),
                "retire": arr("Entries challenged in this worktree to retire (status deprecated, the challenge as the reason, staged)")
            }}
        }),
        json!({
            "name": "ctx_init",
            "description": "Knowledge bootstrap, one task at a time: progress and the next task(s) (describe the project, summarise a module, distill decisions from history). Do it, then ctx_init_submit; stopping keeps progress. Without a knowledge store it starts one only with bootstrap=true, and only when the user asked to set kontext up.",
            "inputSchema": {"type": "object", "properties": {
                "count": {"type": "integer", "description": "How many tasks to return (default 1, max 5)"},
                "bootstrap": {"type": "boolean", "description": "Start the knowledge store (.ai/) if the repository has none — only when the user asked for it"}
            }}
        }),
        json!({
            "name": "ctx_init_submit",
            "description": "Submit the result of a ctx_init task. Writes module summaries, the project purpose or decision records into the repo's knowledge store (reviewed later in the diff).",
            "inputSchema": {"type": "object", "properties": {
                "task_id": s("Id from ctx_init, e.g. mod:apps-web, dec:libs-billing, purpose"),
                "summary": s("One line, at most 200 characters: what it is for"),
                "overview": s("Markdown, 5-15 lines"),
                "decisions": {"type": "array", "description": "For dec:* tasks (at most 3)", "items": {"type": "object", "properties": {
                    "title": {"type": "string"}, "summary": {"type": "string"}, "context": {"type": "string"}, "decision": {"type": "string"},
                    "alternatives": {"type": "string"}, "consequences": {"type": "string"}, "paths": strs(),
                    "commits": strs(), "date": {"type": "string"}, "status": {"type": "string", "description": "proposed | accepted (default)"},
                    "tags": strs()
                }, "required": ["title", "decision"]}},
                "learnings": {"type": "array", "description": "Optional non-obvious gotchas", "items": {"type": "object", "properties": {
                    "title": {"type": "string"}, "body": {"type": "string"}, "paths": strs(), "tags": strs()
                }, "required": ["title", "body"]}},
                "skip": {"type": "boolean", "description": "Skip this task"},
                "reason": s("Why it was skipped")
            }, "required": ["task_id"]}
        }),
    ]
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.trim().is_empty())
}

fn list_arg(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).filter(|s| !s.trim().is_empty()).collect(),
        Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        _ => Vec::new(),
    }
}

fn int_arg(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map(|n| n as usize)
}

pub fn call(app: &App, name: &str, args: &Value, origin: &str) -> Result<String> {
    if name == "ctx_api" {
        return Ok(api_call(app, args).to_string());
    }
    // parameter aliases (`reason` for `why`, `max` for `limit`, …): never a rename, both work
    let normalized = normalize_args(name, args);
    let args = &normalized;
    // `dir`: the tool runs in another worktree, submodule or repository
    if let Some(dir) = str_arg(args, "dir") {
        if CONFINED_DIR.contains(&name) {
            confine_dir(app, name, &dir)?;
        }
        let target = crate::route::open_dir(app, &dir)?;
        let mut rest = args.clone();
        if let Some(o) = rest.as_object_mut() {
            o.remove("dir");
        }
        if target.repo.root == app.repo.root {
            return call(app, name, &rest, origin);
        }
        let out = call(&target, name, &rest, origin)?;
        return Ok(format!("{}\n{out}", crate::route::banner(app, &target)));
    }
    match name {
        "ctx_brief" => {
            let focus = list_arg(args, "focus");
            let budget = int_arg(args, "budget_tokens").unwrap_or(app.cfg().brief.budget_tokens);
            let with_adapters = args.get("adapters").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(ops::brief(app, &focus, budget, with_adapters))
        }
        "ctx_search" => search_full(app, args).map(|o| o.text),
        "ctx_read" => {
            let uri = str_arg(args, "uri").ok_or_else(|| anyhow!("uri is required"))?;
            deny_private_paths(app, &uri)?;
            let level = int_arg(args, "level").unwrap_or(1).min(2) as u8;
            ops::read(app, &uri, level)
        }
        "ctx_capture" => {
            // knowledge belongs to the repository that owns the paths it governs: a capture about
            // `extractor/…` made from camp-bot goes into the extractor's inbox
            let routing = crate::route::owner_of(app, &list_arg(args, "paths"));
            let (target, paths, path_notes) = (routing.owner, routing.paths, routing.notes);
            let home = app;
            let app: &App = target.as_deref().unwrap_or(app);
            if !app.is_set_up() && str_arg(args, "visibility").as_deref() != Some("private") {
                bail!("{NOT_SET_UP}");
            }
            let req = CaptureReq {
                kind: str_arg(args, "kind").ok_or_else(|| anyhow!("kind is required"))?,
                title: str_arg(args, "title").ok_or_else(|| anyhow!("title is required"))?,
                body: str_arg(args, "body").unwrap_or_default(),
                summary: str_arg(args, "summary"),
                paths,
                tags: list_arg(args, "tags"),
                visibility: str_arg(args, "visibility"),
                supersedes: list_arg(args, "supersedes"),
                extends: list_arg(args, "extends"),
                status: str_arg(args, "status"),
                promote: args.get("promote").and_then(|v| v.as_bool()).unwrap_or(false),
                origin: origin.to_string(),
                source: str_arg(args, "source"),
                commits: list_arg(args, "commits"),
                review_by: str_arg(args, "review_by"),
                verify: str_arg(args, "verify"),
                evidence: args
                    .get("evidence")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|i| {
                                let r = i.as_str().or_else(|| i.get("ref").and_then(Value::as_str))?;
                                Some((r.to_string(), i.get("quote").and_then(Value::as_str).map(String::from)))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            let out = ops::capture(app, req)?;
            let mut text = if out.promoted {
                format!("Written to {} (id {}). It is in the working tree — stage and commit it with the change.", out.location, out.id)
            } else if out.visibility == "private" {
                format!("Saved privately as {} (local inbox; mirrored to personal memory adapters if configured).", out.location)
            } else {
                format!(
                    "Captured as {} in the local inbox. Before committing, ctx_prepare_commit promotes it into the change (or promote it now with ctx_inbox action=promote).",
                    out.location
                )
            };
            if target.is_some() {
                let dir = crate::route::dir_arg(home, app);
                let commit = if out.visibility == "private" {
                    String::new()
                } else {
                    format!("\nIt ships with a commit there: ctx_prepare_commit with `dir: \"{dir}\"`.")
                };
                text = format!(
                    "Captured in {} ({}), the repository that owns these paths. {text}{commit}",
                    app.project_name(),
                    crate::route::label(home, app)
                );
            }
            for n in out.notes.into_iter().chain(path_notes) {
                text.push_str(&format!("\nnote: {n}"));
            }
            Ok(text)
        }
        "ctx_challenge" => {
            let id = str_arg(args, "id").ok_or_else(|| anyhow!("id is required"))?;
            let why = str_arg(args, "why").ok_or_else(|| anyhow!("why is required"))?;
            let c = ops::challenge(app, &id, &why, str_arg(args, "visibility").as_deref())?;
            Ok(format!(
                "Recorded the challenge against {} as inbox:{} (local, not shared). Resolve it before committing: ctx_prepare_commit offers to retire the entry or to record a replacement that supersedes it.",
                c.get_extra("against").unwrap_or_default(),
                c.id
            ))
        }
        "ctx_why" => {
            let target = str_arg(args, "target").ok_or_else(|| anyhow!("target is required"))?;
            ops::why(app, &target, int_arg(args, "limit").unwrap_or(12))
        }
        "ctx_threads" => {
            use crate::threads;
            let roots = app.repo.worktree_roots();
            let Some(spec) = str_arg(args, "thread") else {
                let mut out = String::from("Agent threads of this repository on this machine, newest first:\n");
                let recent = threads::in_roots(&roots, 20);
                for f in &recent {
                    if let Ok(t) = threads::load(f) {
                        out.push_str(&format!(
                            "- {} · {} · {} · {} turns · {}\n",
                            f.label(),
                            t.started.as_deref().unwrap_or("").chars().take(10).collect::<String>(),
                            t.branch.as_deref().unwrap_or("-"),
                            t.turns.len(),
                            util::truncate_chars(&util::one_line(t.title.as_deref().unwrap_or("")), 80)
                        ));
                    }
                }
                if recent.is_empty() {
                    out.push_str("(none)\n");
                }
                if let Ok(ws) = threads::superset_workspace(None) {
                    out.push_str(&format!(
                        "\nCurrent Superset workspace: {} ({}) — thread=\"superset:{}\" reads the threads its terminals ran.\n",
                        util::truncate_chars(&util::one_line(&ws.name), 80),
                        ws.branch,
                        threads::short(&ws.id)
                    ));
                }
                out.push_str("\nRead one with ctx_threads thread=\"<label>\".");
                return Ok(out);
            };
            if origin == "agent" {
                threads_spec_allowed(&spec, &roots)?;
            }
            let found: Vec<threads::Found> = match spec.strip_prefix("superset:") {
                Some(ws) => {
                    let w = threads::superset_workspace(Some(ws).filter(|w| !w.is_empty()))?;
                    let (ts, _) = threads::workspace_threads(&w);
                    ts.into_iter().filter(|f| threads::thread_cwd(f).is_none_or(|c| threads::within(&c, &roots))).collect()
                }
                None => vec![threads::resolve(None, &spec, &roots)?],
            };
            if found.is_empty() {
                bail!("no thread of this repository found for '{spec}'");
            }
            let extra = crate::secrets::patterns(&app.cfg().secrets.redact);
            let mut text = String::new();
            for f in &found {
                let t = threads::load(f)?;
                text.push_str(&threads::render(&t, &app.repo.root, &extra));
                text.push_str("\n\n");
            }
            // a part has to fit one tool result: clients cut long ones (Codex at about 10k tokens)
            let budget = int_arg(args, "budget_chars").unwrap_or(20_000).clamp(4_000, 80_000);
            let parts = threads::parts(&text, budget);
            let n = parts.len().max(1);
            let i = int_arg(args, "part").unwrap_or(1).clamp(1, n);
            let more = if i < n { format!(" — call ctx_threads again with part={} for the rest", i + 1) } else { String::new() };
            Ok(format!("{}\n[part {i} of {n}{more}]", parts.get(i - 1).map(String::as_str).unwrap_or("")))
        }
        "ctx_log" => {
            let kind = str_arg(args, "kind").unwrap_or_else(|| "decision".into());
            let kind = ops::normalize_kind(app, &kind).map(|(k, _)| k).unwrap_or(kind);
            let rows =
                ops::log_rows(app, &kind, args.get("all").and_then(|v| v.as_bool()).unwrap_or(false), int_arg(args, "limit").unwrap_or(40));
            Ok(ops::render_log(&rows))
        }
        "ctx_inbox" => {
            let action = str_arg(args, "action").unwrap_or_else(|| "list".into());
            let ids = list_arg(args, "ids");
            let inbox = crate::inbox::Inbox::open(&app.repo);
            match action.as_str() {
                "list" => {
                    let items = inbox.list();
                    if items.is_empty() {
                        return Ok("Inbox is empty.".into());
                    }
                    Ok(items
                        .iter()
                        .map(|e| {
                            format!(
                                "- `{}` [{}{}] {}{}{}\n",
                                e.id,
                                e.kind,
                                if e.visibility.as_deref() == Some("private") { ", private" } else { "" },
                                e.l0(150),
                                crate::inbox::Origin::of(&app.repo, e).note(),
                                e.get_extra("source").map(|s| format!(" · from {s}")).unwrap_or_default()
                            )
                        })
                        .collect())
                }
                "show" => {
                    let id = ids.first().ok_or_else(|| anyhow!("ids is required"))?;
                    Ok(inbox.get(id).ok_or_else(|| anyhow!("no inbox entry '{id}'"))?.markdown())
                }
                "promote" => {
                    if ids.is_empty() {
                        bail!("ids is required");
                    }
                    if !app.is_set_up() {
                        bail!("{NOT_SET_UP}");
                    }
                    let done = ops::promote(app, &ids, true)?;
                    Ok(done.iter().map(|(id, rel)| format!("- {id} → {rel} (staged)\n")).collect())
                }
                "verify" => {
                    let [id] = ids.as_slice() else { bail!("action=verify takes one id") };
                    let out = crate::api_write::inbox_verify(app, &json!({"id": id, "verify": args.get("verify")}))?;
                    Ok(match out["verify"].as_str() {
                        Some(c) => {
                            format!("{} now names `{c}` as its verify command; promotion keeps it.", out["id"].as_str().unwrap_or(id))
                        }
                        None => format!("{} has no verify command now.", out["id"].as_str().unwrap_or(id)),
                    })
                }
                "drop" => {
                    for id in &ids {
                        let e = inbox.get(id).ok_or_else(|| anyhow!("no inbox entry '{id}'"))?;
                        inbox.remove(&e.id)?;
                    }
                    Ok(format!("Dropped {}.", ids.join(", ")))
                }
                other => bail!("unknown action '{other}'"),
            }
        }
        "ctx_drift" => drift_full(app, args).map(|o| o.text),
        "ctx_confirm" => {
            let mut ids = list_arg(args, "ids");
            if let Some(id) = str_arg(args, "id") {
                ids.insert(0, id);
            }
            if ids.is_empty() {
                bail!("id is required");
            }
            let because = str_arg(args, "because");
            let mut lines = Vec::new();
            let mut failed = Vec::new();
            for (id, r) in ids.iter().zip(crate::api_write::confirm_ids(app, &ids, "agent", because.as_deref())) {
                match r {
                    Ok(out) => lines.push(format!(
                        "Confirmed {} ({}): checked {} in {} (staged).",
                        out["id"].as_str().unwrap_or_default(),
                        out["ledger"]["kind"].as_str().unwrap_or("agent"),
                        out["checked"].as_str().unwrap_or_default(),
                        out["path"].as_str().unwrap_or_default()
                    )),
                    Err(e) => failed.push(format!("{id}: {e:#}")),
                }
            }
            if lines.is_empty() {
                bail!("{}", failed.join("; "));
            }
            lines.push("Commit it with the change that prompted the look.".into());
            if !failed.is_empty() {
                lines.push(format!("Not confirmed: {}", failed.join("; ")));
            }
            Ok(lines.join("\n"))
        }
        "ctx_verify" => {
            let id = str_arg(args, "id").ok_or_else(|| anyhow!("id is required"))?;
            // TODO(agent B): call `crate::api_write::verify_with_transport(app, &req, Transport::Mcp)`
            // when origin == "agent" (an unpinned command is returned, not run), once it lands.
            let out = crate::api_write::verify(app, &json!({"id": id, "because": str_arg(args, "because")}))?;
            let passed = out["passed"] == json!(true);
            let how = match (out["timed_out"].as_bool(), out["exit_code"].as_i64()) {
                (Some(true), _) => "timed out".to_string(),
                (_, Some(code)) => format!("exited {code}"),
                _ => "was killed".to_string(),
            };
            let mut text = format!(
                "{} {}: `{}` {how} at {} (ledger line staged; commit it with your change).",
                if passed { "Passed" } else { "FAILED" },
                out["id"].as_str().unwrap_or_default(),
                out["command"].as_str().unwrap_or_default(),
                out["commit"].as_str().map(|c| &c[..c.len().min(8)]).unwrap_or("no commit")
            );
            let tail = out["output_tail"].as_str().unwrap_or_default().trim_end();
            if !tail.is_empty() {
                text.push_str(&format!("\nOutput (end):\n{tail}"));
            }
            if !passed {
                text.push_str(
                    "\nThe entry does not hold as written, or its command is broken: fix the code, or correct or supersede the entry.",
                );
            }
            Ok(text)
        }
        "ctx_prepare_commit" => {
            let promote = list_arg(args, "promote");
            if !promote.is_empty() && !app.is_set_up() {
                bail!("{NOT_SET_UP}");
            }
            ops::prepare_commit(app, &PrepareReq { promote, drop: list_arg(args, "drop"), retire: list_arg(args, "retire") })
        }
        "ctx_init" => {
            let count = int_arg(args, "count").unwrap_or(1).clamp(1, 5);
            let (inv, hist) = crate::init::load_cached(app)?;
            let state = crate::init::load_state(app);
            let (entries, _) = app.entries();
            if !entries.iter().any(|e| e.kind == "architecture" && e.id == "overview") {
                // a bootstrap on another branch (waiting for review) would only conflict with a second one
                let hint = ops::no_knowledge_hint(app);
                if hint.starts_with("Team knowledge exists") {
                    return Ok(format!("{hint}\nNothing was bootstrapped."));
                }
                // starting a store is the user's call, not a side effect of an agent's task
                if !app.is_set_up() && !args.get("bootstrap").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Ok("kontext is not set up in this repository. Bootstrapping writes `.ai/` into the working tree (uncommitted) for the team to review — do it only when the user asked for it, by calling ctx_init with bootstrap=true.".into());
                }
                // deterministic bootstrap first (no hooks, nothing committed), then hand out the first task
                let log = crate::init::bootstrap(&app.repo.root)?;
                let fresh = App::open(&app.repo.root)?;
                let (inv, hist) = crate::init::load_cached(&fresh)?;
                let p = deepen::plan(&fresh, &inv, &hist, &crate::init::load_state(&fresh));
                let mut out = format!(
                    "Bootstrapped `.ai/` (uncommitted — review it with the user):\n{log}\n\nKnowledge bootstrap: {}/{} tasks done, {} pending.\n\n",
                    p.done,
                    p.total,
                    p.pending.len()
                );
                for t in p.pending.iter().take(count) {
                    out.push_str(&deepen::render_task(&fresh, t, false, 0));
                    out.push('\n');
                }
                out.push_str("\n(Git hooks were not installed; suggest `kontext hooks install` to the user.)\n");
                return Ok(out);
            }
            let p = deepen::plan(app, &inv, &hist, &state);
            if p.pending.is_empty() {
                return Ok(format!(
                    "Bootstrap complete ({}/{}). Nothing left to do — suggest reviewing `git diff -- {}` and committing it.",
                    p.done,
                    p.total,
                    app.cfg().store.dir
                ));
            }
            let mut out = format!("Knowledge bootstrap: {}/{} tasks done, {} pending.\n\n", p.done, p.total, p.pending.len());
            for t in p.pending.iter().take(count) {
                out.push_str(&deepen::render_task(app, t, false, 0));
                out.push('\n');
            }
            Ok(out)
        }
        "ctx_init_submit" => {
            let sub: deepen::Submission = serde_json::from_value(args.clone()).map_err(|e| anyhow!("invalid submission: {e}"))?;
            deepen::submit(app, sub)
        }
        other => bail!("unknown tool '{other}'"),
    }
}

/// Structured tool transport deliberately bypasses text clipping and routing banners.
pub fn api_call(app: &App, args: &Value) -> Value {
    let Some(operation) = args.get("operation").and_then(Value::as_str) else {
        return crate::api::failure(crate::api::error("invalid_input", "operation is required"));
    };
    let mut input = args.get("input").cloned().unwrap_or_else(|| json!({}));
    if let Some(dir) = args.get("dir") {
        let Some(object) = input.as_object_mut() else {
            return crate::api::failure(crate::api::error("invalid_input", "input must be a JSON object"));
        };
        object.insert("dir".into(), dir.clone());
    }
    crate::api::execute(app, operation, &input)
}

pub fn prompts() -> Vec<Value> {
    vec![
        json!({"name": "kontext-init", "description": "Bootstrap this repository's team knowledge, one small task at a time", "arguments": [{"name": "tasks", "description": "How many tasks to do now (default: until done)", "required": false}]}),
        json!({"name": "kontext-commit", "description": "Prepare the current change for commit: promote relevant knowledge, capture what is missing"}),
        json!({"name": "kontext-reflect", "description": "Review this session and capture the durable decisions and lessons (at most three)"}),
        json!({"name": "kontext-distill", "description": "Distill durable team knowledge from another agent thread or a Superset workspace", "arguments": [{"name": "thread", "description": "claude:<id>, codex:<id>, superset:<workspace> or a transcript file (default: choose from the list)", "required": false}]}),
    ]
}

pub fn prompt_text(name: &str, args: &Value) -> Result<(String, String)> {
    Ok(match name {
        "kontext-init" => {
            let n = args.get("tasks").and_then(|v| v.as_str()).unwrap_or("all remaining");
            (
                "Bootstrap team knowledge".into(),
                format!(
                    "Bootstrap this repository's shared knowledge with the kontext tools. Do {n} task(s):\n\
1. Call ctx_init with bootstrap=true (I am asking you to set it up). It returns progress and the next task (purpose, mod:<module>, dec:<area> or refresh:<module>).\n\
2. Do exactly what the task asks: read the listed docs and key files (skim — stop when you understand), or inspect the listed commits with `git show`.\n\
3. Call ctx_init_submit with concise, concrete results: summaries are one line about purpose; overviews 5–15 lines about responsibilities, flows, invariants and pitfalls; decisions only when a commit really encodes a durable choice.\n\
4. Repeat. Never invent facts — if unsure, say so in the text or skip the task with a reason.\n\
When finished (or when I stop you), summarise what was written and suggest committing `.ai/` on a branch for team review."
                ),
            )
        }
        "kontext-commit" => (
            "Prepare commit".into(),
            "Prepare the current change for commit with kontext:\n\
1. Make sure the intended files are staged, then call ctx_prepare_commit.\n\
2. For each inbox candidate that belongs to this change, confirm with me, then promote it (ctx_prepare_commit promote=[ids]); drop stale ones.\n\
3. If the change embodies a decision or a non-obvious lesson that is not recorded, draft it with ctx_capture (short: what, why, consequences, paths) and promote it.\n\
4. Fix any validation or secret-scan errors it reports.\n\
5. Propose the commit message with the knowledge trailers the report lists (the prepare-commit-msg hook adds them when it is installed)."
                .into(),
        ),
        "kontext-reflect" => (
            "Reflect and capture".into(),
            "Review what we did in this session. Identify at most three things worth remembering beyond this task: decisions we made (and alternatives rejected), conventions we settled, or pitfalls that cost time. \
For each, call ctx_capture with a short title, a body of a few lines (what, why, consequences) and the paths it governs. Use visibility=private for things only I need. Skip anything already covered — check with ctx_search first."
                .into(),
        ),
        "kontext-distill" => {
            let thread = args.get("thread").and_then(|v| v.as_str()).filter(|t| !t.trim().is_empty());
            let first = match thread {
                Some(t) => format!("Call ctx_threads with thread=\"{t}\"."),
                None => "Call ctx_threads without arguments: it lists this repository's recent threads and the current Superset workspace. Ask me which one I mean if it is not obvious.".to_string(),
            };
            (
                "Distill a thread into team knowledge".into(),
                format!(
                    "Distill durable team knowledge from an agent thread.\n\
1. {first}\n\
2. Read all of it: when a reply says there are more parts, call ctx_threads again with the next part.\n\
3. Pick at most five things the team should still know later — decisions (a direction taken or an alternative rejected, and why), conventions, non-obvious pitfalls, incidents. Skip progress notes, debugging chatter and plans that were not carried out.\n\
4. For each: check with ctx_search that it is not recorded yet, then ctx_capture it — kind, a short title, a few lines (what, why, consequences), the paths it governs, the commits it came from, and source set to the thread's label.\n\
5. Tell me what you captured. It waits in the local inbox until ctx_prepare_commit promotes it with a commit; nothing is shared before that."
                ),
            )
        }
        other => bail!("unknown prompt '{other}'"),
    })
}

pub fn clip(s: String) -> String {
    if s.len() > 90_000 { util::truncate_chars(&s, 90_000) } else { s }
}

// ------------------------------------------------------------------- structured tool results

/// A tool's answer as MCP returns it: the text block (unchanged), and for the read tools a
/// `structuredContent` object matching the tool's `outputSchema` plus `resource_link` blocks.
#[derive(Debug, Default)]
pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
    pub links: Vec<Value>,
}

impl ToolOutput {
    fn text(text: String) -> ToolOutput {
        ToolOutput { text, ..ToolOutput::default() }
    }
}

/// `outputSchema` of the tools that return `structuredContent` (JSON Schema 2020-12, the MCP
/// default dialect). Kept to the required fields (tools/list is in every session's context);
/// results carry more, and the schemas allow it:
/// - ctx_brief `{text, entries: [full id], focus, budget_tokens}`
/// - ctx_search `{query, hits: [{uri, title, source, snippet, kind?, score, date?, status?}], notes}`
/// - ctx_read `{uri, text, level, entry?}`; ctx_why `{target, text, entries}`
/// - ctx_inbox `{action, text, items?: [{id, kind, title, visibility, source}]}` (items for list)
/// - ctx_drift `{base, head, entries: [{id, kind, title, path, changed, commits, date?, checked?}], text}`
pub fn output_schema(name: &str) -> Option<Value> {
    let st = || json!({"type": "string"});
    let ids = || json!({"type": "array", "items": {"type": "string"}});
    let objs = || json!({"type": "array", "items": {"type": "object"}});
    let obj = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
    Some(match name {
        "ctx_brief" => obj(json!({"text": st(), "entries": ids()}), &["text", "entries"]),
        "ctx_search" => obj(json!({"hits": objs(), "notes": ids()}), &["hits"]),
        "ctx_read" => obj(json!({"uri": st(), "text": st()}), &["uri", "text"]),
        "ctx_why" => obj(json!({"text": st(), "entries": ids()}), &["text", "entries"]),
        "ctx_inbox" => obj(json!({"action": st(), "text": st()}), &["action", "text"]),
        "ctx_drift" => obj(json!({"entries": objs(), "text": st()}), &["entries", "text"]),
        _ => return None,
    })
}

/// Run a tool for MCP: like [`call`], plus structured content for the tools that declare an
/// `outputSchema`. `origin` is "agent" over MCP.
pub fn call_full(app: &App, name: &str, args: &Value, origin: &str) -> Result<ToolOutput> {
    if output_schema(name).is_none() {
        return call(app, name, args, origin).map(ToolOutput::text);
    }
    let args = &normalize_args(name, args);
    if let Some(dir) = str_arg(args, "dir") {
        if CONFINED_DIR.contains(&name) {
            confine_dir(app, name, &dir)?;
        }
        let target = crate::route::open_dir(app, &dir)?;
        let mut rest = args.clone();
        if let Some(o) = rest.as_object_mut() {
            o.remove("dir");
        }
        if target.repo.root == app.repo.root {
            return call_full(app, name, &rest, origin);
        }
        let mut out = call_full(&target, name, &rest, origin)?;
        out.text = format!("{}\n{}", crate::route::banner(app, &target), out.text);
        return Ok(out);
    }
    match name {
        "ctx_search" => search_full(app, args),
        "ctx_drift" => drift_full(app, args),
        _ => {
            let text = call(app, name, args, origin)?;
            Ok(structure(app, name, args, text))
        }
    }
}

fn search_full(app: &App, args: &Value) -> Result<ToolOutput> {
    let query = str_arg(args, "query").ok_or_else(|| anyhow!("query is required"))?;
    let req = SearchReq {
        query: query.clone(),
        kinds: list_arg(args, "kinds"),
        sources: list_arg(args, "sources"),
        limit: int_arg(args, "limit").unwrap_or(10),
        adapters: args.get("adapters").and_then(Value::as_bool),
    };
    let (hits, notes) = ops::search(app, &req)?;
    let mut out = model::render(&hits, int_arg(args, "budget_tokens").unwrap_or(1200));
    if !notes.is_empty() {
        out.push_str(&format!("\n(notes: {})\n", notes.join("; ")));
    }
    let links = hits
        .iter()
        .filter_map(|h| {
            let id = h.uri.strip_prefix("kx:").filter(|id| !id.contains('/'))?;
            Some(entry_link(id, &h.title))
        })
        .collect();
    let structured = json!({"query": query, "hits": hits, "notes": notes});
    Ok(ToolOutput { text: out, structured: Some(structured), links })
}

fn drift_full(app: &App, args: &Value) -> Result<ToolOutput> {
    let report = crate::drift::drift(app, str_arg(args, "base").as_deref(), str_arg(args, "head").as_deref(), &list_arg(args, "kinds"))?;
    let text = crate::drift::render_text(&report, true);
    let mut structured = report.to_json();
    structured["text"] = json!(text);
    Ok(ToolOutput { text, structured: Some(structured), links: Vec::new() })
}

fn entry_link(id: &str, title: &str) -> Value {
    json!({"type": "resource_link", "uri": format!("kontext://entry/{id}"), "name": id, "title": title, "mimeType": "text/markdown"})
}

/// The local entries a text names as `[id]` or `<kx:id>` (short ids resolved), in order, once each.
fn named_entries<'e>(entries: &'e [crate::store::Entry], text: &str) -> Vec<&'e crate::store::Entry> {
    static REF: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\[([A-Za-z0-9][A-Za-z0-9._-]*)\]|<kx:([A-Za-z0-9][A-Za-z0-9._-]*)>").expect("valid regex")
    });
    let mut seen = std::collections::HashSet::new();
    REF.captures_iter(text)
        .filter_map(|c| c.get(1).or_else(|| c.get(2)))
        .filter_map(|m| crate::store::Store::find(entries, m.as_str()))
        .filter(|e| seen.insert(e.id.clone()))
        .collect()
}

/// The structured form of a text tool's answer, built from the answer and the request.
fn structure(app: &App, name: &str, args: &Value, text: String) -> ToolOutput {
    let mut links = Vec::new();
    let structured = match name {
        "ctx_brief" | "ctx_why" => {
            let (entries, _) = app.entries();
            // TODO(agent C): once `ops::governing_lines` lands, add its governing section here.
            let named = named_entries(&entries, &text);
            links = named.iter().map(|e| entry_link(&e.id, &e.title)).collect();
            let ids: Vec<&str> = named.iter().map(|e| e.id.as_str()).collect();
            if name == "ctx_brief" {
                json!({"text": text, "focus": list_arg(args, "focus"),
                    "budget_tokens": int_arg(args, "budget_tokens").unwrap_or(app.cfg().brief.budget_tokens), "entries": ids})
            } else {
                json!({"target": str_arg(args, "target").unwrap_or_default(), "text": text, "entries": ids})
            }
        }
        "ctx_read" => {
            let uri = str_arg(args, "uri").unwrap_or_default();
            let mut v = json!({"uri": uri, "level": int_arg(args, "level").unwrap_or(1).min(2), "text": text});
            if !uri.starts_with("file:") && !uri.contains("://") && !uri.starts_with("git:") && !uri.starts_with("inbox:") {
                let (entries, _) = app.entries();
                if let Some(e) = crate::store::Store::find(&entries, &uri) {
                    v["entry"] = json!(e.id);
                }
            }
            v
        }
        "ctx_inbox" => {
            let action = str_arg(args, "action").unwrap_or_else(|| "list".into());
            let mut v = json!({"action": action, "text": text});
            if action == "list" {
                let items: Vec<Value> = crate::inbox::Inbox::open(&app.repo)
                    .list()
                    .iter()
                    .map(|e| {
                        json!({"id": e.id, "kind": e.kind, "title": e.title,
                            "visibility": e.visibility.as_deref().unwrap_or("team"), "source": e.get_extra("source")})
                    })
                    .collect();
                v["items"] = json!(items);
            }
            v
        }
        _ => return ToolOutput::text(text),
    };
    ToolOutput { text, structured: Some(structured), links }
}

// -------------------------------------------------------------------- aliases and boundaries

/// Accept the other names agents use for a parameter: `reason` = `why` = `because`, `id`/`ids`,
/// `focus`/`paths`, `limit`/`max`, `kind`/`kinds` (a string too), `budget_tokens` for ctx_threads.
/// The documented name always wins; nothing is renamed.
pub fn normalize_args(name: &str, args: &Value) -> Value {
    let Some(obj) = args.as_object() else { return args.clone() };
    let mut o = obj.clone();
    fn has(o: &serde_json::Map<String, Value>, k: &str) -> bool {
        o.get(k).is_some_and(|v| !v.is_null() && v != "")
    }
    fn alias(o: &mut serde_json::Map<String, Value>, canonical: &str, others: &[&str]) {
        if has(o, canonical) {
            return;
        }
        if let Some(v) = others.iter().find_map(|k| o.get(*k).filter(|v| !v.is_null() && *v != "").cloned()) {
            o.insert(canonical.to_string(), v);
        }
    }
    fn first_of(v: Option<&Value>) -> Option<Value> {
        match v {
            Some(Value::Array(a)) => a.first().cloned(),
            Some(Value::String(s)) => s.split(',').next().map(|x| json!(x.trim())),
            _ => None,
        }
    }
    match name {
        "ctx_challenge" => alias(&mut o, "why", &["reason", "because"]),
        "ctx_confirm" | "ctx_verify" => alias(&mut o, "because", &["reason", "why"]),
        "ctx_init_submit" => alias(&mut o, "reason", &["why", "because"]),
        _ => {}
    }
    // one id where the tool takes one, a list where it takes several
    if matches!(name, "ctx_challenge" | "ctx_verify")
        && !has(&o, "id")
        && let Some(v) = first_of(o.get("ids"))
    {
        o.insert("id".into(), v);
    }
    match name {
        "ctx_inbox" => alias(&mut o, "ids", &["id"]),
        "ctx_read" => alias(&mut o, "uri", &["id", "path"]),
        "ctx_brief" => alias(&mut o, "focus", &["paths"]),
        "ctx_search" | "ctx_drift" => alias(&mut o, "kinds", &["kind"]),
        _ => {}
    }
    if name == "ctx_log"
        && !has(&o, "kind")
        && let Some(v) = first_of(o.get("kinds"))
    {
        o.insert("kind".into(), v);
    }
    alias(&mut o, "limit", &["max"]);
    if name == "ctx_threads" && !has(&o, "budget_chars") {
        let tokens = o.get("budget_tokens").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
        if let Some(t) = tokens {
            o.insert("budget_chars".into(), json!(t.saturating_mul(4)));
        }
    }
    Value::Object(o)
}

/// `dir` of a tool that runs commands or changes local state: inside this repository (its
/// submodules and nested checkouts included) or one of its worktrees, never elsewhere on disk.
fn confine_dir(app: &App, tool: &str, dir: &str) -> Result<()> {
    let p = std::path::Path::new(dir.trim());
    let abs = if p.is_absolute() { p.to_path_buf() } else { app.repo.root.join(p) };
    // a path that does not exist is refused by `open_dir` with its own message
    let Ok(canon) = abs.canonicalize() else { return Ok(()) };
    let canon_of = |s: &std::path::Path| s.canonicalize().unwrap_or_else(|_| s.to_path_buf());
    let mut roots = vec![canon_of(&app.repo.root)];
    roots.extend(app.repo.worktree_roots().iter().map(|r| canon_of(std::path::Path::new(r))));
    if roots.iter().any(|r| canon.starts_with(r)) {
        return Ok(());
    }
    bail!("{tool} takes `dir` only inside this repository, its worktrees or its submodules; '{dir}' is outside them")
}

/// `ctx_read` never serves git internals or credentials: `.git/` (objects, config with remote
/// tokens, hooks), `.git-credentials` and `.ssh/`, also when a symlink points there.
fn deny_private_paths(app: &App, uri: &str) -> Result<()> {
    let u = uri.trim();
    if u.contains("://") || ["kx:", "git:", "inbox:"].iter().any(|s| u.starts_with(s)) {
        return Ok(());
    }
    let raw = u.strip_prefix("file:").unwrap_or(u);
    let path = raw.split('#').next().unwrap_or("");
    let private = |p: &str| {
        p.split(['/', '\\']).any(|seg| {
            let seg = seg.to_ascii_lowercase();
            seg == ".git" || seg == ".git-credentials" || seg == ".ssh"
        })
    };
    let resolved = app.repo.abs(path.trim_start_matches("./").trim_start_matches('/')).canonicalize().ok().and_then(|c| {
        let root = app.repo.root.canonicalize().ok()?;
        c.strip_prefix(root).ok().map(|r| r.to_string_lossy().to_string())
    });
    if private(path) || resolved.as_deref().is_some_and(private) {
        bail!("refusing to read '{path}': git internals and credentials are not context");
    }
    Ok(())
}

/// Which threads `ctx_threads` reads over MCP: named ones (`claude:`, `codex:`, `superset:`,
/// `last`, a bare session id) and transcript files of this repository's own threads. Never `-`:
/// stdin is the server's JSON-RPC stream.
fn threads_spec_allowed(spec: &str, roots: &[String]) -> Result<()> {
    let s = spec.trim();
    let hint = "name a thread as claude:<id>, codex:<id>, superset:<workspace> or last";
    if s == "-" {
        bail!("thread \"-\" (stdin) is not available over MCP: {hint}");
    }
    if s == "last" || ["claude:", "codex:", "superset:"].iter().any(|p| s.starts_with(p)) {
        return Ok(());
    }
    let path = std::path::Path::new(s);
    if !path.exists() && !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        // a bare session id: looked up among transcripts like claude:<id> / codex:<id>
        return Ok(());
    }
    let Ok(canon) = path.canonicalize() else { bail!("no transcript '{s}': {hint}") };
    let ours = crate::threads::in_roots(roots, 10_000).into_iter().any(|f| f.path.canonicalize().is_ok_and(|c| c == canon));
    if !ours {
        bail!("'{s}' is not a transcript of this repository's agent threads: {hint}");
    }
    Ok(())
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn tools_list_fits_its_budget() {
        let json = serde_json::to_string(&json!({"tools": definitions()})).unwrap();
        assert!(json.len() < 20_000, "tools/list for ctx_* is {} chars", json.len());
    }

    #[test]
    fn instructions_stand_alone_in_their_first_512_chars() {
        for text in [INSTRUCTIONS, NOT_SET_UP_INSTRUCTIONS, NO_REPO_INSTRUCTIONS] {
            let head: String = text.chars().take(512).collect();
            assert!(head.contains("ctx_brief"), "{head}");
            assert!(text.chars().count() < 4_096);
        }
        let head: String = INSTRUCTIONS.chars().take(512).collect();
        assert!(head.contains("decisions") && head.contains("search your tools"), "{head}");
    }

    #[test]
    fn verify_is_destructive_and_open_world_and_brief_always_loads() {
        let defs = definitions();
        let tool = |n: &str| defs.iter().find(|t| t["name"] == n).unwrap().clone();
        let verify = tool("ctx_verify");
        assert_eq!(verify["annotations"]["destructiveHint"], true);
        assert_eq!(verify["annotations"]["openWorldHint"], true);
        assert!(verify["description"].as_str().unwrap().contains("pinned"));
        assert_eq!(tool("ctx_brief")["_meta"]["anthropic/alwaysLoad"], true);
        assert_eq!(tool("ctx_read")["_meta"]["anthropic/maxResultSizeChars"], 200_000);
        assert_eq!(tool("ctx_api")["_meta"]["anthropic/maxResultSizeChars"], 200_000);
        for n in ["ctx_drift", "ctx_threads", "ctx_init"] {
            assert!(tool(n)["inputSchema"]["properties"]["dir"].is_object(), "{n} takes dir");
        }
        for n in ["ctx_brief", "ctx_search", "ctx_read", "ctx_why", "ctx_inbox", "ctx_drift"] {
            assert_eq!(tool(n)["outputSchema"]["type"], "object", "{n} has an outputSchema");
        }
        let submit = tool("ctx_init_submit");
        let props = &submit["inputSchema"]["properties"]["decisions"]["items"]["properties"];
        assert!(props["status"].is_object() && props["tags"].is_object());
        assert!(is_tool("ctx_brief") && !is_tool("ctx_nope"));
    }

    #[test]
    fn aliases_fill_the_documented_name_and_never_override_it() {
        let a = normalize_args("ctx_challenge", &json!({"id": "x", "reason": "moved"}));
        assert_eq!(a["why"], "moved");
        let a = normalize_args("ctx_challenge", &json!({"id": "x", "why": "kept", "reason": "other"}));
        assert_eq!(a["why"], "kept");
        assert_eq!(normalize_args("ctx_verify", &json!({"ids": ["a"], "why": "w"}))["id"], "a");
        assert_eq!(normalize_args("ctx_verify", &json!({"id": "a", "why": "w"}))["because"], "w");
        assert_eq!(normalize_args("ctx_inbox", &json!({"action": "show", "id": "c1"}))["ids"], "c1");
        assert_eq!(normalize_args("ctx_brief", &json!({"paths": ["src/"]}))["focus"], json!(["src/"]));
        assert_eq!(normalize_args("ctx_search", &json!({"query": "q", "max": 3}))["limit"], 3);
        assert_eq!(normalize_args("ctx_search", &json!({"query": "q", "kind": "decision"}))["kinds"], "decision");
        assert_eq!(normalize_args("ctx_log", &json!({"kinds": "learning,decision"}))["kind"], "learning");
        assert_eq!(normalize_args("ctx_threads", &json!({"budget_tokens": 5000}))["budget_chars"], 20_000);
        assert_eq!(normalize_args("ctx_read", &json!({"id": "kx:a"}))["uri"], "kx:a");
    }

    #[test]
    fn threads_over_mcp_refuse_stdin_and_foreign_files() {
        assert!(threads_spec_allowed("-", &[]).unwrap_err().to_string().contains("stdin"));
        assert!(threads_spec_allowed("claude:4f1c2a9b", &[]).is_ok());
        assert!(threads_spec_allowed("last", &[]).is_ok());
        let foreign = std::env::temp_dir().join(format!("kontext-not-a-thread-{}.jsonl", std::process::id()));
        std::fs::write(&foreign, "{}\n").unwrap();
        assert!(threads_spec_allowed(foreign.to_str().unwrap(), &["/nonexistent-root".into()]).is_err());
        let _ = std::fs::remove_file(foreign);
    }
}
