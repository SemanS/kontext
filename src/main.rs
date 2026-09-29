//! kontext — team context bridge for coding agents.
//!
//! One Rust binary that is at the same time an MCP server (`kontext mcp`), a CLI and a set of git
//! hooks. Shared truth is short Markdown in the repository, reviewed through normal commits; local
//! history, indexes and candidates are derived and private; everything else (semantic memory,
//! session archives, code intelligence, LLMs, chat) is plugged in through configured adapters.
mod adapters;
mod app;
mod config;
mod connect;
mod distill;
mod events;
mod glob;
mod hooks;
mod inbox;
mod index;
mod init;
mod jpath;
mod mcp;
mod model;
mod ops;
mod presets;
mod repo;
mod secrets;
mod signals;
mod store;
mod template;
mod threads;
mod tools;
mod util;

use anyhow::{Context, Result, anyhow, bail};
use app::App;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "kontext",
    version,
    about = "Team context bridge for coding agents: git-native decision memory, one MCP server, pluggable adapters."
)]
struct Cli {
    /// Run as if kontext was started in <DIR>
    #[arg(short = 'C', long = "dir", global = true, value_name = "DIR")]
    dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Bootstrap team knowledge for this repository: scan → history → render → wire → deepen
    Init(InitArgs),
    /// Run the MCP server on stdio (configure clients with the command `kontext mcp`)
    Mcp,
    /// Print the team brief — what an agent sees first
    Brief {
        /// Paths or topic words to prioritise
        #[arg(long, short)]
        focus: Vec<String>,
        /// Approximate token budget
        #[arg(long)]
        budget: Option<usize>,
        /// text | json | claude-hook (SessionStart additionalContext)
        #[arg(long, default_value = "text")]
        format: String,
        /// Skip adapters with a brief op
        #[arg(long)]
        no_adapters: bool,
    },
    /// Search knowledge, docs, commit history and adapters
    Search {
        /// Words, identifiers or paths to look for
        query: Vec<String>,
        /// Restrict local hits: decision, convention, learning, incident, architecture, doc, commit (repeatable)
        #[arg(long, short)]
        kind: Vec<String>,
        /// Sources: local and/or adapter names (repeatable; default: all)
        #[arg(long, short)]
        source: Vec<String>,
        /// Maximum number of hits
        #[arg(long, short = 'n', default_value_t = 10)]
        limit: usize,
        /// Print hits as JSON
        #[arg(long)]
        json: bool,
    },
    /// Read a URI: kx:<id>, file:<path>[#section], git:<sha>, inbox:<id>, adapter URIs (viking://…)
    Read {
        /// kx:<id>, file:<path>[#section], git:<sha>, inbox:<id>, a plain repo path, or an adapter URI
        uri: String,
        /// 0 = one line, 1 = overview, 2 = full
        #[arg(long, short, default_value_t = 2)]
        level: u8,
    },
    /// Record a decision, convention, learning or incident (local inbox unless --promote)
    Capture(CaptureArgs),
    /// Local knowledge candidates (not shared until promoted and committed)
    Inbox {
        #[command(subcommand)]
        action: Option<InboxCmd>,
    },
    /// Promote inbox candidates into the store and stage them
    Promote {
        /// Inbox ids (see `kontext inbox`)
        ids: Vec<String>,
        /// Write the files but do not `git add` them
        #[arg(long)]
        no_stage: bool,
    },
    /// Distill team knowledge from agent threads (Claude Code, Codex, Superset workspaces, files) into the inbox
    Distill {
        /// Thread files: a Claude Code or Codex transcript (.jsonl) or any text, e.g. a copied Slack thread; `-` reads stdin
        files: Vec<PathBuf>,
        /// Claude Code session id or prefix, or `last` (this repository's newest)
        #[arg(long)]
        claude: Vec<String>,
        /// Codex session id or prefix, or `last`
        #[arg(long)]
        codex: Vec<String>,
        /// Superset workspace: id or prefix, worktree name, path or branch (alone: the current workspace)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        superset: Option<String>,
        /// List the threads instead of distilling them
        #[arg(long)]
        list: bool,
        /// Adapter with an `llm` op (default: init.llm)
        #[arg(long)]
        llm: Option<String>,
        /// Most entries kept per thread
        #[arg(long, default_value_t = 5)]
        max: usize,
        /// Most transcript parts (about 45k characters each) read per thread
        #[arg(long, default_value_t = 12)]
        max_parts: usize,
        /// Parallel llm calls
        #[arg(long, default_value_t = 3)]
        jobs: usize,
        /// Print what would be captured instead of capturing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Decision timeline (or another kind)
    Log {
        /// Kind to list: decision, convention, learning, incident
        #[arg(long, short, default_value = "decision")]
        kind: String,
        /// Include superseded / rejected entries
        #[arg(long)]
        all: bool,
        /// Maximum number of rows
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,
        /// Print rows as JSON
        #[arg(long)]
        json: bool,
    },
    /// Why is this path / path:line / commit / symbol the way it is?
    Why {
        /// path, path:line, path:start-end, commit sha, or a symbol / topic
        target: String,
        /// Maximum number of items per section
        #[arg(long, short = 'n', default_value_t = 12)]
        limit: usize,
    },
    /// The report an agent gets from ctx_prepare_commit
    PrepareCommit {
        /// Inbox ids to promote into this commit (written and staged; repeatable)
        #[arg(long)]
        promote: Vec<String>,
        /// Inbox ids to discard (repeatable)
        #[arg(long)]
        drop: Vec<String>,
    },
    /// Validate knowledge files and scan them for secrets
    Check {
        /// Check exactly what is staged (what the pre-commit hook checks)
        #[arg(long)]
        staged: bool,
    },
    /// Git hook entry point (called by the installed hooks)
    Hook {
        /// pre-commit, prepare-commit-msg, post-commit, post-merge or post-rewrite
        name: String,
        /// Arguments git passed to the hook
        args: Vec<String>,
    },
    /// Install, remove or inspect git hooks
    Hooks {
        #[command(subcommand)]
        action: HooksCmd,
    },
    /// Adapters: list (default), test, inspect, add a preset
    Adapters {
        #[command(subcommand)]
        action: Option<AdaptersCmd>,
    },
    /// List bundled and user presets, or print one
    Presets {
        /// Print this preset's TOML
        name: Option<String>,
    },
    /// Wire an agent client: claude, claude-hooks, codex, cursor, opencode, agents-md
    Connect {
        /// claude, claude-hooks, codex, cursor, opencode or agents-md
        target: String,
        /// Write the config (default: print what to add)
        #[arg(long)]
        write: bool,
    },
    /// Trust the adapters declared in this repository's shared config (after reviewing them)
    Trust,
    /// Repository identity, config layers, store, index, inbox, init progress, adapters, hooks
    Status,
    /// Rebuild the local search index from scratch
    Reindex,
    /// Queued adapter deliveries
    Outbox {
        #[command(subcommand)]
        action: Option<OutboxCmd>,
    },
    /// Queue `sync` events for knowledge at HEAD (--all mirrors every entry once)
    Sync {
        /// Mirror every entry once (backfill), not only changes since the last sync
        #[arg(long)]
        all: bool,
    },
    /// Call an agent tool locally, e.g. `kontext call ctx_brief '{"focus":["src"]}'`
    Call {
        /// Tool name, e.g. ctx_brief
        tool: String,
        /// Arguments as a JSON object
        args: Option<String>,
    },
}

#[derive(Args)]
struct InitArgs {
    #[command(subcommand)]
    action: Option<InitCmd>,
    /// Do not install git hooks
    #[arg(long)]
    no_hooks: bool,
    /// Also wire agent clients (comma-separated: claude,claude-hooks,cursor,opencode,agents-md)
    #[arg(long, value_delimiter = ',')]
    connect: Vec<String>,
    /// Run pending deepen tasks with an llm adapter
    #[arg(long)]
    deepen: bool,
    /// Adapter with an `llm` op (default: init.llm)
    #[arg(long)]
    llm: Option<String>,
    /// Parallel llm calls
    #[arg(long, default_value_t = 2)]
    jobs: usize,
    /// Maximum tasks per run
    #[arg(long, default_value_t = 50)]
    max: usize,
}

#[derive(Subcommand)]
enum InitCmd {
    /// Phase status and deepen progress
    Status,
    /// Print the next deepen task(s)
    Next {
        /// How many tasks to print
        #[arg(long, short = 'n', default_value_t = 1)]
        count: usize,
        /// Attach file excerpts (for pasting into a chat model)
        #[arg(long)]
        inline: bool,
    },
    /// Submit a task result as JSON (file or `-` for stdin)
    Submit {
        /// JSON file with {task_id, summary, overview, decisions, learnings}, or `-` for stdin
        file: String,
    },
    /// Skip a task
    Skip {
        /// Task id, e.g. mod:apps-web
        task_id: String,
        /// Why it is skipped
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Args)]
struct CaptureArgs {
    /// decision, convention, learning (pitfall), incident
    #[arg(long, short)]
    kind: String,
    /// Short statement, e.g. "Prices are integer cents"
    #[arg(long, short)]
    title: String,
    /// Body text (or --file, or stdin)
    #[arg(long, short)]
    body: Option<String>,
    /// Read the body from a file (`-` for stdin)
    #[arg(long)]
    file: Option<PathBuf>,
    /// One-line summary (derived from the body when omitted)
    #[arg(long)]
    summary: Option<String>,
    /// Paths or globs this governs (comma-separated)
    #[arg(long, short, value_delimiter = ',')]
    paths: Vec<String>,
    /// Tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
    /// Ids of entries this replaces (they get marked superseded)
    #[arg(long, value_delimiter = ',')]
    supersedes: Vec<String>,
    /// proposed | accepted (decisions default to accepted)
    #[arg(long)]
    status: Option<String>,
    /// Keep it personal (never promoted; mirrored to personal adapters)
    #[arg(long)]
    private: bool,
    /// Write into the store now (working tree) instead of the inbox
    #[arg(long)]
    promote: bool,
}

#[derive(Subcommand)]
enum InboxCmd {
    /// List candidates (default)
    List,
    /// Print one candidate
    Show { id: String },
    /// Discard candidates
    Drop { ids: Vec<String> },
}

#[derive(Subcommand)]
enum HooksCmd {
    /// Install hooks (into core.hooksPath if set, else .git/hooks)
    Install {
        /// Target directory, e.g. a tracked `.githooks`
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Remove kontext's hook blocks (restores chained originals)
    Uninstall {
        /// Target directory, e.g. a tracked `.githooks`
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Show which hooks carry the kontext block
    Status,
}

#[derive(Subcommand)]
enum AdaptersCmd {
    /// Active and inactive adapters, with the reason (default)
    List,
    /// Health check and a sample call of each op
    Test {
        /// Only this adapter
        name: Option<String>,
        /// Query used for search/code samples
        #[arg(long, default_value = "architecture")]
        query: String,
    },
    /// List the MCP tools (and schemas) an adapter offers — useful for writing op mappings
    Inspect {
        /// Adapter name
        name: String,
    },
    /// Add a preset to your config (user scope by default)
    Add {
        /// Preset name (see `kontext presets`)
        preset: String,
        /// user | repo | local
        #[arg(long, default_value = "user")]
        scope: String,
        /// Set a preset variable, e.g. --var user=alice (repeatable)
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        /// Replace an adapter of the same name
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum OutboxCmd {
    /// Queued events and their last error (default)
    List,
    /// Deliver queued events now
    Flush {
        /// Print nothing (used by hooks)
        #[arg(long)]
        quiet: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("kontext: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run_distill(
    app: &App,
    files: Vec<PathBuf>,
    claude: Vec<String>,
    codex: Vec<String>,
    superset: Option<String>,
    list: bool,
    opt: distill::Options,
) -> Result<()> {
    use threads::Agent;
    let roots = app.repo.worktree_roots();
    let mut found: Vec<threads::Found> = Vec::new();
    for c in &claude {
        found.push(threads::resolve(Some(Agent::Claude), c, &roots)?);
    }
    for c in &codex {
        found.push(threads::resolve(Some(Agent::Codex), c, &roots)?);
    }
    for f in &files {
        found.push(threads::resolve(None, &f.to_string_lossy(), &roots)?);
    }
    if let Some(q) = &superset {
        let ws = threads::superset_workspace(Some(q.as_str()).filter(|q| !q.is_empty()))?;
        // a workspace of another repository would put its knowledge into the wrong inbox
        if std::path::Path::new(&ws.path).is_dir() {
            let other = repo::Repo::discover(std::path::Path::new(&ws.path)).ok();
            if other.as_ref().is_some_and(|o| o.common_dir != app.repo.common_dir) {
                bail!(
                    "Superset workspace {} works in another repository ({}) — run kontext distill there",
                    threads::short(&ws.id),
                    ws.path
                );
            }
        }
        let (ts, missing) = threads::workspace_threads(&ws);
        // a terminal of the workspace may have run an agent in another directory
        let (ts, elsewhere): (Vec<_>, Vec<_>) =
            ts.into_iter().partition(|f| threads::thread_cwd(f).is_none_or(|cwd| threads::within(&cwd, &roots)));
        if !elsewhere.is_empty() {
            let list: Vec<String> =
                elsewhere.iter().map(|f| format!("{} ({})", f.label(), threads::thread_cwd(f).unwrap_or_default())).collect();
            println!("skipped, ran outside this repository: {}", list.join(", "));
        }
        println!(
            "Superset workspace {} · {} · {} · {} thread(s) here{}",
            threads::short(&ws.id),
            util::truncate_chars(&util::one_line(&ws.name), 70),
            if ws.branch.is_empty() { ws.kind.clone() } else { ws.branch.clone() },
            ts.len(),
            if missing.is_empty() {
                String::new()
            } else {
                format!(", {} without a transcript (ephemeral runs or other agents)", missing.len())
            }
        );
        found.extend(ts);
    }
    if list || found.is_empty() {
        let items = if found.is_empty() { threads::in_roots(&roots, 30) } else { found };
        if items.is_empty() {
            println!("No Claude Code or Codex threads of this repository on this machine.");
            return Ok(());
        }
        println!("{:<18} {:<10} {:<22} {:>6}  title", "thread", "started", "branch", "turns");
        for f in &items {
            let Ok(t) = threads::load(f) else { continue };
            println!(
                "{:<18} {:<10} {:<22} {:>6}  {}",
                f.label(),
                t.started.as_deref().unwrap_or("").chars().take(10).collect::<String>(),
                util::truncate_chars(t.branch.as_deref().unwrap_or(""), 22),
                t.turns.len(),
                util::truncate_chars(&util::one_line(t.title.as_deref().unwrap_or("")), 70)
            );
        }
        if !list {
            println!("\nDistill one: kontext distill --claude <id> | --codex <id> | --superset [workspace] | <file>");
        }
        return Ok(());
    }
    let dry = opt.dry_run;
    println!("Distilling {} thread(s){}…", found.len(), if dry { " (dry run)" } else { "" });
    let outcomes = distill::distill(app, &found, &opt, &|l| println!("{l}"))?;
    let total: usize = outcomes.iter().map(|o| o.captured.len()).sum();
    for o in &outcomes {
        if !o.captured.is_empty() {
            println!("\n## {}", o.thread);
        }
        for (c, loc) in &o.captured {
            println!(
                "\n[{}] {}{}\n{}",
                c.kind,
                c.title,
                if c.paths.is_empty() { String::new() } else { format!("  ({})", c.paths.join(", ")) },
                if dry { c.body.trim().to_string() } else { format!("→ {loc}") }
            );
        }
    }
    if dry {
        println!("\n{total} entr{} found; nothing was captured (--dry-run).", if total == 1 { "y" } else { "ies" });
    } else if total > 0 {
        println!(
            "\n{total} entr{} in the inbox — review them (`kontext inbox`), then promote each with the commit it belongs to (`kontext prepare-commit --promote <id>` or ctx_prepare_commit); drop the rest (`kontext inbox drop <id>`).",
            if total == 1 { "y" } else { "ies" }
        );
    } else {
        println!("\nNothing durable found (or all of it is recorded already).");
    }
    Ok(())
}

fn open(dir: &Option<PathBuf>) -> Result<App> {
    let d = dir.clone().unwrap_or_else(|| PathBuf::from("."));
    App::open(&d)
}

fn read_body(body: Option<String>, file: Option<PathBuf>) -> Result<String> {
    if let Some(b) = body {
        return Ok(b);
    }
    if let Some(f) = file {
        if f.as_os_str() == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            return Ok(s);
        }
        return std::fs::read_to_string(&f).with_context(|| format!("read {}", f.display()));
    }
    if !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(s);
    }
    Ok(String::new())
}

fn run(cli: Cli) -> Result<i32> {
    let dir = cli.dir.clone();
    match cli.cmd {
        Cmd::Init(a) => {
            let app = open(&dir)?;
            match a.action {
                Some(InitCmd::Status) => {
                    println!("kontext init · {}\n{}", app.repo.id, init::status_text(&app)?);
                }
                Some(InitCmd::Next { count, inline }) => {
                    let (inv, hist) = init::load_cached(&app)?;
                    let p = init::deepen::plan(&app, &inv, &hist, &init::load_state(&app));
                    println!("Progress: {}/{} tasks done, {} pending.\n", p.done, p.total, p.pending.len());
                    for t in p.pending.iter().take(count.max(1)) {
                        println!("{}", init::deepen::render_task(&app, t, inline, app.cfg().init.task_inline_chars));
                    }
                }
                Some(InitCmd::Submit { file }) => {
                    let text = if file == "-" {
                        let mut s = String::new();
                        std::io::stdin().read_to_string(&mut s)?;
                        s
                    } else {
                        std::fs::read_to_string(&file)?
                    };
                    let v = init::deepen::extract_json_object(&text).ok_or_else(|| anyhow!("input is not a JSON object"))?;
                    let sub: init::deepen::Submission = serde_json::from_value(v)?;
                    println!("{}", init::deepen::submit(&app, sub)?);
                }
                Some(InitCmd::Skip { task_id, reason }) => {
                    let sub = init::deepen::Submission { task_id, skip: true, reason, ..Default::default() };
                    println!("{}", init::deepen::submit(&app, sub)?);
                }
                None => {
                    let root = app.repo.root.clone();
                    drop(app);
                    let opts = init::InitOpts {
                        hooks: !a.no_hooks,
                        connect: a.connect,
                        deepen: a.deepen,
                        llm: a.llm,
                        jobs: a.jobs,
                        max_tasks: a.max,
                    };
                    init::run(&root, &opts, &|line| println!("{line}"))?;
                }
            }
        }
        Cmd::Mcp => {
            let d = dir.clone().or_else(|| std::env::var_os("KONTEXT_DIR").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
            mcp::serve(d)?;
        }
        Cmd::Brief { focus, budget, format, no_adapters } => {
            let app = open(&dir)?;
            let text = ops::brief(&app, &focus, budget.unwrap_or(app.cfg().brief.budget_tokens), !no_adapters);
            match format.as_str() {
                "json" => println!("{}", json!({"brief": text})),
                "claude-hook" => {
                    println!("{}", json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": text}}))
                }
                _ => print!("{text}"),
            }
        }
        Cmd::Search { query, kind, source, limit, json } => {
            let app = open(&dir)?;
            let q = query.join(" ");
            if q.trim().is_empty() {
                bail!("give a query");
            }
            let (hits, notes) = ops::search(&app, &ops::SearchReq { query: q, kinds: kind, sources: source, limit })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({"hits": hits, "notes": notes}))?);
            } else {
                print!("{}", model::render(&hits, 4000));
                for n in notes {
                    eprintln!("note: {n}");
                }
            }
        }
        Cmd::Read { uri, level } => {
            let app = open(&dir)?;
            println!("{}", ops::read(&app, &uri, level.min(2))?);
        }
        Cmd::Capture(a) => {
            let app = open(&dir)?;
            let body = read_body(a.body, a.file)?;
            let out = ops::capture(
                &app,
                ops::CaptureReq {
                    kind: a.kind,
                    title: a.title,
                    summary: a.summary,
                    body,
                    paths: a.paths,
                    tags: a.tags,
                    visibility: Some(if a.private { "private".into() } else { "team".into() }),
                    supersedes: a.supersedes,
                    status: a.status,
                    promote: a.promote,
                    origin: "cli".into(),
                    ..Default::default()
                },
            )?;
            println!("{} {}", if out.promoted { "wrote" } else { "captured" }, out.location);
            for n in out.notes {
                println!("note: {n}");
            }
        }
        Cmd::Inbox { action } => {
            let app = open(&dir)?;
            let inbox = inbox::Inbox::open(&app.repo);
            match action.unwrap_or(InboxCmd::List) {
                InboxCmd::List => {
                    let items = inbox.list();
                    if items.is_empty() {
                        println!("Inbox is empty.");
                    }
                    for e in items {
                        println!(
                            "{}  [{}{}]  {}{}{}",
                            e.id,
                            e.kind,
                            if e.visibility.as_deref() == Some("private") { ", private" } else { "" },
                            e.l0(120),
                            inbox::Origin::of(&app.repo, &e).note(),
                            e.get_extra("source").map(|s| format!(" · from {s}")).unwrap_or_default()
                        );
                    }
                }
                InboxCmd::Show { id } => println!("{}", inbox.get(&id).ok_or_else(|| anyhow!("no inbox entry '{id}'"))?.markdown()),
                InboxCmd::Drop { ids } => {
                    for id in ids {
                        let e = inbox.get(&id).ok_or_else(|| anyhow!("no inbox entry '{id}'"))?;
                        inbox.remove(&e.id)?;
                        println!("dropped {}", e.id);
                    }
                }
            }
        }
        Cmd::Promote { ids, no_stage } => {
            let app = open(&dir)?;
            if ids.is_empty() {
                bail!("give inbox ids (see `kontext inbox`)");
            }
            for (id, rel) in ops::promote(&app, &ids, !no_stage)? {
                println!("{id} → {rel}{}", if no_stage { "" } else { " (staged)" });
            }
        }
        Cmd::Log { kind, all, limit, json } => {
            let app = open(&dir)?;
            let kind = ops::normalize_kind(&app, &kind).map(|(k, _)| k).unwrap_or(kind);
            let rows = ops::log_rows(&app, &kind, all, limit);
            if json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                print!("{}", ops::render_log(&rows));
            }
        }
        Cmd::Distill { files, claude, codex, superset, list, llm, max, max_parts, jobs, dry_run } => {
            let app = open(&dir)?;
            run_distill(
                &app,
                files,
                claude,
                codex,
                superset,
                list,
                distill::Options { llm, max_per_thread: max, max_parts, jobs, dry_run },
            )?;
        }
        Cmd::Why { target, limit } => {
            let app = open(&dir)?;
            print!("{}", ops::why(&app, &target, limit)?);
        }
        Cmd::PrepareCommit { promote, drop } => {
            let app = open(&dir)?;
            print!("{}", ops::prepare_commit(&app, &ops::PrepareReq { promote, drop })?);
        }
        Cmd::Check { staged } => {
            let app = open(&dir)?;
            let rep = if staged { ops::check_staged(&app)? } else { ops::check_all(&app) };
            let text = ops::render_check(&rep);
            print!("{text}");
            println!(
                "{} file(s) checked, {} error(s), {} warning(s)",
                rep.checked,
                rep.errors(),
                rep.issues.len() + rep.secrets.len() - rep.errors()
            );
            if rep.errors() > 0 {
                return Ok(1);
            }
        }
        Cmd::Hook { name, args } => {
            let app = match open(&dir) {
                Ok(a) => a,
                Err(e) => {
                    // never break git because kontext cannot start
                    eprintln!("kontext: hook {name} skipped: {e:#}");
                    return Ok(0);
                }
            };
            return hooks::run(&app, &name, &args);
        }
        Cmd::Hooks { action } => {
            let app = open(&dir)?;
            match action {
                HooksCmd::Install { dir } => {
                    // installing by hand is the explicit choice that `init --no-hooks` waited for
                    let report = hooks::install(&app, dir)?;
                    hooks::set_opted_out(&app.repo, false)?;
                    println!("{}", report.summary());
                }
                HooksCmd::Uninstall { dir } => {
                    let removed = hooks::uninstall(&app, dir)?;
                    println!(
                        "{}",
                        if removed.is_empty() { "nothing to remove".to_string() } else { format!("removed: {}", removed.join(", ")) }
                    );
                }
                HooksCmd::Status => print!("{}", hooks::status(&app)?),
            }
        }
        Cmd::Adapters { action } => {
            let app = open(&dir)?;
            adapters_cmd(&app, action.unwrap_or(AdaptersCmd::List))?;
        }
        Cmd::Presets { name } => match name {
            Some(n) => print!("{}", presets::get(&n).ok_or_else(|| anyhow!("no preset '{n}'"))?),
            None => {
                for (n, t) in presets::all() {
                    println!("{n:<16} {}", presets::describe(&t));
                }
                println!("\nAdd one with `kontext adapters add <preset>` (user scope) — or copy it into any config layer.");
            }
        },
        Cmd::Connect { target, write } => {
            let app = open(&dir)?;
            println!("{}", connect::connect(&app, &target, write)?);
        }
        Cmd::Trust => {
            let app = open(&dir)?;
            match (&app.loaded.untrusted_adapters, &app.loaded.shared_path) {
                (Some(hash), Some(path)) => {
                    let text = std::fs::read_to_string(path)?;
                    let t: toml::Table = text.parse()?;
                    if let Some(a) = t.get("adapters") {
                        println!("Adapters declared in {}:\n\n{}", path.display(), toml::to_string_pretty(a)?);
                    }
                    let p = config::trust(&app.repo, hash)?;
                    println!("Trusted (recorded in {}). They run on this machine until the table changes.", p.display());
                }
                _ => println!("Nothing to trust: the shared config declares no untrusted adapters."),
            }
        }
        Cmd::Status => {
            let app = open(&dir)?;
            status(&app)?;
        }
        Cmd::Reindex => {
            let app = open(&dir)?;
            let idx_dir = app.repo.worktree_state_dir().join("index");
            let _ = std::fs::remove_dir_all(&idx_dir);
            let n = app.with_index(|i| Ok(i.num_docs()))?;
            println!("index rebuilt: {n} documents");
        }
        Cmd::Outbox { action } => {
            let app = open(&dir)?;
            let ob = events::Outbox::open(&app.repo);
            match action.unwrap_or(OutboxCmd::List) {
                OutboxCmd::List => {
                    let evs = ob.list();
                    if evs.is_empty() {
                        println!("Outbox is empty.");
                    }
                    for e in evs {
                        println!(
                            "{} {} {} attempts={} {}",
                            e.at,
                            e.event,
                            e.payload.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            e.attempts,
                            e.last_error.unwrap_or_default()
                        );
                    }
                }
                OutboxCmd::Flush { quiet } => {
                    let rep = ob.flush(app.registry())?;
                    if !quiet {
                        for d in &rep.delivered {
                            println!("delivered {d}");
                        }
                        for f in &rep.failed {
                            println!("failed {f}");
                        }
                        println!("{} delivered, {} failed, {} remaining", rep.delivered.len(), rep.failed.len(), rep.remaining);
                    }
                }
            }
        }
        Cmd::Sync { all } => {
            let app = open(&dir)?;
            let n = hooks::sync_events(&app, all)?;
            println!("queued {n} sync event(s)");
            if n > 0 {
                let rep = events::Outbox::open(&app.repo).flush(app.registry())?;
                println!("{} delivered, {} failed, {} remaining", rep.delivered.len(), rep.failed.len(), rep.remaining);
            }
        }
        Cmd::Call { tool, args } => {
            let app = open(&dir)?;
            let v: Value = match args {
                Some(a) => serde_json::from_str(&a).context("args must be a JSON object")?,
                None => json!({}),
            };
            println!("{}", tools::call(&app, &tool, &v, "cli")?);
        }
    }
    Ok(0)
}

fn adapters_cmd(app: &App, action: AdaptersCmd) -> Result<()> {
    match action {
        AdaptersCmd::List => {
            let reg = app.registry();
            if reg.adapters.is_empty() && reg.skipped.is_empty() && reg.errors.is_empty() {
                println!("No adapters configured. See `kontext presets`, then `kontext adapters add <preset>`.");
            }
            for a in &reg.adapters {
                println!("✓ {:<16} {}", a.name, a.describe());
            }
            for (n, why) in &reg.skipped {
                println!("· {n:<16} inactive: {why}");
            }
            for e in &reg.errors {
                println!("✗ {e}");
            }
            for w in &app.loaded.warnings {
                println!("! {w}");
            }
        }
        AdaptersCmd::Test { name, query } => {
            let reg = app.registry();
            let targets: Vec<_> = reg.adapters.iter().filter(|a| name.as_ref().is_none_or(|n| &a.name == n)).cloned().collect();
            if targets.is_empty() {
                bail!("no active adapter{}", name.map(|n| format!(" named '{n}'")).unwrap_or_default());
            }
            for a in targets {
                println!("== {}", a.name);
                match a.health() {
                    Ok(h) => println!("  health  ✓ {}", util::one_line(&h)),
                    Err(e) => println!("  health  ✗ {e:#}"),
                }
                if a.has_op("search") {
                    match a.search(&query, 3) {
                        Ok(hits) => println!(
                            "  search  ✓ {} hit(s){}",
                            hits.len(),
                            hits.first().map(|h| format!(": {} <{}>", util::truncate_chars(&h.title, 80), h.uri)).unwrap_or_default()
                        ),
                        Err(e) => println!("  search  ✗ {e:#}"),
                    }
                }
                if a.has_op("code") {
                    match a.code(&query, 3) {
                        Ok(hits) => println!(
                            "  code    ✓ {} hit(s){}",
                            hits.len(),
                            hits.first().map(|h| format!(": {} {}", util::truncate_chars(&h.title, 60), h.uri)).unwrap_or_default()
                        ),
                        Err(e) => println!("  code    ✗ {e:#}"),
                    }
                }
                if a.has_op("history") {
                    match a.history("README.md", 3) {
                        Ok(hits) => println!("  history ✓ {} hit(s)", hits.len()),
                        Err(e) => println!("  history ✗ {e:#}"),
                    }
                }
                for op in ["store", "llm", "brief"] {
                    if a.has_op(op) {
                        println!("  {op:<7} configured (not called by test)");
                    }
                }
            }
        }
        AdaptersCmd::Inspect { name } => {
            let a = app.registry().get(&name).ok_or_else(|| anyhow!("no active adapter '{name}'"))?;
            let tools = a.raw_tools()?;
            if tools.is_empty() {
                println!("'{name}' exposes no MCP tools (driver {}).", a.cfg.driver);
            }
            for t in tools {
                let props = t
                    .input_schema
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|o| {
                        o.iter()
                            .map(|(k, v)| format!("{k}:{}", v.get("type").and_then(|t| t.as_str()).unwrap_or("?")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                println!("{}\n    ({props})\n    {}", t.name, util::truncate_chars(&util::one_line(&t.description), 200));
            }
        }
        AdaptersCmd::Add { preset, scope, vars, force } => {
            let vars: Vec<(String, String)> = vars
                .iter()
                .map(|kv| {
                    kv.split_once('=')
                        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
                        .ok_or_else(|| anyhow!("--var expects KEY=VALUE, got '{kv}'"))
                })
                .collect::<Result<_>>()?;
            let scope = match scope.as_str() {
                "user" => presets::Scope::User,
                "repo" | "shared" => presets::Scope::Repo,
                "local" => presets::Scope::Local,
                other => bail!("unknown scope '{other}' (user, repo, local)"),
            };
            let (path, names) = presets::add(app, &preset, scope, force, &vars)?;
            println!("added {} to {}", names.join(", "), path.display());
            println!("review the vars/when there, then check with `kontext adapters test`.");
        }
    }
    Ok(())
}

fn status(app: &App) -> Result<()> {
    println!("kontext {}", env!("CARGO_PKG_VERSION"));
    println!(
        "repo      {}  (name {}, branch {}, worktree {})",
        app.repo.id,
        app.repo.name,
        app.repo.branch.as_deref().unwrap_or("-"),
        app.repo.worktree_key
    );
    let layers: Vec<String> = app.loaded.layers.iter().map(|p| p.display().to_string()).collect();
    println!("config    defaults{}", if layers.is_empty() { String::new() } else { format!(" + {}", layers.join(" + ")) });
    let (entries, errors) = app.entries();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for e in &entries {
        *counts.entry(e.kind.as_str()).or_default() += 1;
    }
    let kinds: Vec<String> =
        app.cfg().kind_names().iter().map(|k| format!("{} {k}", counts.get(k.as_str()).copied().unwrap_or(0))).collect();
    println!(
        "store     {} (decisions in {}): {}{}",
        app.cfg().store.dir,
        app.cfg().kind_path("decision"),
        kinds.join(", "),
        if errors.is_empty() { String::new() } else { format!(" · {} unreadable", errors.len()) }
    );
    match app.with_index(|i| Ok(i.num_docs())) {
        Ok(n) => println!("index     {n} documents (entries, docs, commits)"),
        Err(e) => println!("index     ✗ {e:#}"),
    }
    let inbox = inbox::Inbox::open(&app.repo).list();
    let private = inbox.iter().filter(|e| e.visibility.as_deref() == Some("private")).count();
    println!("inbox     {} team, {private} private", inbox.len() - private);
    print!("init\n{}", init::status_text(app).unwrap_or_default());
    let reg = app.registry();
    let active: Vec<String> = reg.adapters.iter().map(|a| a.name.clone()).collect();
    let skipped: Vec<String> = reg.skipped.iter().map(|(n, w)| format!("{n} ({w})")).collect();
    println!(
        "adapters  active: {}{}",
        if active.is_empty() { "none".into() } else { active.join(", ") },
        if skipped.is_empty() { String::new() } else { format!(" · inactive: {}", skipped.join(", ")) }
    );
    print!("hooks     {}", hooks::status(app).unwrap_or_default().replace('\n', "\n          "));
    println!("\noutbox    {} queued", events::Outbox::open(&app.repo).list().len());
    for w in app.warnings() {
        println!("warning   {w}");
    }
    Ok(())
}
