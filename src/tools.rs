//! The agent-facing tool surface (used by the MCP server and by `kontext call`).
use crate::app::App;
use crate::init::deepen;
use crate::model;
use crate::ops::{self, CaptureReq, PrepareReq, SearchReq};
use crate::util;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "kontext is this repository's shared, reviewed team context — decisions, conventions, learnings, incidents and the module map — plus any connected memory, history and code systems. \
Start each task with ctx_brief (pass `focus` with the paths or topic you will work on). Use ctx_search / ctx_read for detail and ctx_why to learn why code is the way it is. \
When you and the user settle something durable (a decision, a convention, a non-obvious pitfall), record it with ctx_capture — short: what, why, consequences, and the paths it governs. \
Before committing, run ctx_prepare_commit and promote the candidates that belong to the change so they are reviewed together with it. \
Shared knowledge changes only through commits and review; supersede a decision instead of rewriting it.";

fn s(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
fn arr(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

/// MCP tool annotations. Clients use them to decide what needs approval: Codex, for one, runs
/// read-only tools without asking and asks (or, with `approval_policy = "never"`, refuses) the rest.
fn annotations(name: &str) -> Value {
    let (title, read_only, destructive) = match name {
        "ctx_brief" => ("Team brief", true, false),
        "ctx_search" => ("Search team context", true, false),
        "ctx_read" => ("Read a context URI", true, false),
        "ctx_why" => ("Why is this code the way it is", true, false),
        "ctx_log" => ("Decision timeline", true, false),
        "ctx_capture" => ("Capture knowledge", false, false),
        // `drop` discards only local, never-shared inbox drafts — not destructive in the MCP sense,
        // and Codex refuses destructive tools outright under `approval_policy = "never"`
        "ctx_inbox" => ("Knowledge inbox", false, false),
        "ctx_prepare_commit" => ("Prepare the commit", false, false),
        "ctx_init" => ("Knowledge bootstrap: next tasks", false, false),
        "ctx_init_submit" => ("Knowledge bootstrap: submit a task", false, false),
        _ => ("", false, false),
    };
    let mut a = json!({"readOnlyHint": read_only, "openWorldHint": false});
    if !title.is_empty() {
        a["title"] = json!(title);
    }
    if !read_only {
        a["destructiveHint"] = json!(destructive);
        a["idempotentHint"] = json!(false);
    }
    a
}

pub fn definitions() -> Vec<Value> {
    let mut defs = tool_list();
    for d in &mut defs {
        let name = d["name"].as_str().unwrap_or("").to_string();
        d["annotations"] = annotations(&name);
    }
    defs
}

fn tool_list() -> Vec<Value> {
    vec![
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
            "description": "Search team knowledge (decisions, conventions, learnings, incidents, module docs), repository docs and commit history, plus connected memory systems. Returns compact hits with URIs — open one with ctx_read.",
            "inputSchema": {"type": "object", "properties": {
                "query": s("What you are looking for, in plain words or identifiers"),
                "kinds": arr("Restrict local results: decision, convention, learning, incident, architecture, doc, commit"),
                "sources": arr("Restrict sources: local and/or adapter names (default: all)"),
                "limit": {"type": "integer", "description": "Max hits (default 10)"},
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
            "description": "Record something durable: a decision made, a convention, a non-obvious learning or pitfall, an incident. Keep it short (what, why, consequences) and list the paths it governs. It lands in the local inbox (not shared) until promoted into a commit; promote=true writes it into the repo's knowledge store right away. visibility=private keeps it personal (mirrored only to personal memory adapters).",
            "inputSchema": {"type": "object", "properties": {
                "kind": {"type": "string", "enum": ["decision", "convention", "learning", "incident"], "description": "What it is (pitfalls are learnings)"},
                "title": s("Short statement, e.g. 'Use Tantivy for local recall'"),
                "body": s("Markdown: context, the decision/lesson, consequences. A few lines."),
                "summary": s("One line (optional; derived from the body otherwise)"),
                "paths": arr("Repo paths or globs this governs, e.g. apps/api/src/billing/**"),
                "tags": arr("Optional tags"),
                "visibility": {"type": "string", "enum": ["team", "private"]},
                "supersedes": arr("Ids of entries this replaces (they get marked superseded)"),
                "status": s("Decisions: proposed | accepted (default)"),
                "promote": {"type": "boolean", "description": "Write into the repo store now instead of the inbox"}
            }, "required": ["kind", "title", "body"]}
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
            "name": "ctx_inbox",
            "description": "Local knowledge candidates captured in this clone (not shared yet): list, show, promote (write into the repo store and stage), or drop.",
            "inputSchema": {"type": "object", "properties": {
                "action": {"type": "string", "enum": ["list", "show", "promote", "drop"]},
                "ids": arr("Candidate ids for show/promote/drop")
            }}
        }),
        json!({
            "name": "ctx_prepare_commit",
            "description": "Run before committing. Summarises the staged change, lists recorded knowledge it touches, inbox candidates to promote into this commit (promote=[ids] writes and stages them; drop=[ids] discards), validation and secret-scan results for knowledge files, module docs to refresh, and the commit trailers the hook will add.",
            "inputSchema": {"type": "object", "properties": {
                "promote": arr("Inbox ids to promote and stage"),
                "drop": arr("Inbox ids to discard")
            }}
        }),
        json!({
            "name": "ctx_init",
            "description": "Step-by-step knowledge bootstrap for this repository. Returns progress and the next task(s): describe the project, summarise a module, or distill decisions from git history. Do the task by reading what it lists, then call ctx_init_submit. Repeat until complete; stopping any time is fine — progress is kept.",
            "inputSchema": {"type": "object", "properties": {
                "count": {"type": "integer", "description": "How many tasks to return (default 1, max 5)"}
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
                    "consequences": {"type": "string"}, "paths": {"type": "array", "items": {"type": "string"}},
                    "commits": {"type": "array", "items": {"type": "string"}}, "date": {"type": "string"}
                }, "required": ["title", "decision"]}},
                "learnings": {"type": "array", "description": "Optional non-obvious gotchas", "items": {"type": "object", "properties": {
                    "title": {"type": "string"}, "body": {"type": "string"}, "paths": {"type": "array", "items": {"type": "string"}}
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
    match name {
        "ctx_brief" => {
            let focus = list_arg(args, "focus");
            let budget = int_arg(args, "budget_tokens").unwrap_or(app.cfg().brief.budget_tokens);
            let with_adapters = args.get("adapters").and_then(|v| v.as_bool()).unwrap_or(true);
            Ok(ops::brief(app, &focus, budget, with_adapters))
        }
        "ctx_search" => {
            let query = str_arg(args, "query").ok_or_else(|| anyhow!("query is required"))?;
            let req = SearchReq {
                query,
                kinds: list_arg(args, "kinds"),
                sources: list_arg(args, "sources"),
                limit: int_arg(args, "limit").unwrap_or(10),
            };
            let (hits, notes) = ops::search(app, &req)?;
            let mut out = model::render(&hits, int_arg(args, "budget_tokens").unwrap_or(1200));
            if !notes.is_empty() {
                out.push_str(&format!("\n(notes: {})\n", notes.join("; ")));
            }
            Ok(out)
        }
        "ctx_read" => {
            let uri = str_arg(args, "uri").ok_or_else(|| anyhow!("uri is required"))?;
            let level = int_arg(args, "level").unwrap_or(1).min(2) as u8;
            ops::read(app, &uri, level)
        }
        "ctx_capture" => {
            let req = CaptureReq {
                kind: str_arg(args, "kind").ok_or_else(|| anyhow!("kind is required"))?,
                title: str_arg(args, "title").ok_or_else(|| anyhow!("title is required"))?,
                body: str_arg(args, "body").unwrap_or_default(),
                summary: str_arg(args, "summary"),
                paths: list_arg(args, "paths"),
                tags: list_arg(args, "tags"),
                visibility: str_arg(args, "visibility"),
                supersedes: list_arg(args, "supersedes"),
                status: str_arg(args, "status"),
                promote: args.get("promote").and_then(|v| v.as_bool()).unwrap_or(false),
                origin: origin.to_string(),
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
            for n in out.notes {
                text.push_str(&format!("\nnote: {n}"));
            }
            Ok(text)
        }
        "ctx_why" => {
            let target = str_arg(args, "target").ok_or_else(|| anyhow!("target is required"))?;
            ops::why(app, &target, int_arg(args, "limit").unwrap_or(12))
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
                                "- `{}` [{}{}] {}{}\n",
                                e.id,
                                e.kind,
                                if e.visibility.as_deref() == Some("private") { ", private" } else { "" },
                                e.l0(150),
                                crate::inbox::Origin::of(&app.repo, e).note()
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
                    let done = ops::promote(app, &ids, true)?;
                    Ok(done.iter().map(|(id, rel)| format!("- {id} → {rel} (staged)\n")).collect())
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
        "ctx_prepare_commit" => ops::prepare_commit(app, &PrepareReq { promote: list_arg(args, "promote"), drop: list_arg(args, "drop") }),
        "ctx_init" => {
            let count = int_arg(args, "count").unwrap_or(1).clamp(1, 5);
            let (inv, hist) = crate::init::load_cached(app)?;
            let state = crate::init::load_state(app);
            let (entries, _) = app.entries();
            if !entries.iter().any(|e| e.kind == "architecture" && e.id == "overview") {
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

pub fn prompts() -> Vec<Value> {
    vec![
        json!({"name": "kontext-init", "description": "Bootstrap this repository's team knowledge, one small task at a time", "arguments": [{"name": "tasks", "description": "How many tasks to do now (default: until done)", "required": false}]}),
        json!({"name": "kontext-commit", "description": "Prepare the current change for commit: promote relevant knowledge, capture what is missing"}),
        json!({"name": "kontext-reflect", "description": "Review this session and capture the durable decisions and lessons (at most three)"}),
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
1. Call ctx_init. It returns progress and the next task (purpose, mod:<module>, dec:<area> or refresh:<module>).\n\
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
5. Propose the commit message; the prepare-commit-msg hook adds the knowledge trailers."
                .into(),
        ),
        "kontext-reflect" => (
            "Reflect and capture".into(),
            "Review what we did in this session. Identify at most three things worth remembering beyond this task: decisions we made (and alternatives rejected), conventions we settled, or pitfalls that cost time. \
For each, call ctx_capture with a short title, a body of a few lines (what, why, consequences) and the paths it governs. Use visibility=private for things only I need. Skip anything already covered — check with ctx_search first."
                .into(),
        ),
        other => bail!("unknown prompt '{other}'"),
    })
}

pub fn clip(s: String) -> String {
    if s.len() > 90_000 { util::truncate_chars(&s, 90_000) } else { s }
}
