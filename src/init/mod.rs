//! `kontext init`: a staged, resumable bootstrap of team knowledge.
//!
//!   1. scan     — deterministic inventory (languages, manifests, stack, modules, dependencies)
//!   2. history  — decision-shaped commits, dependency swaps, churn, commit conventions
//!   3. render   — `.ai/` config, architecture overview, module docs, index (facts only)
//!   4. wire     — git hooks (and, on request, agent client config)
//!   5. deepen   — semantic summaries and decision records, one small task at a time, by the
//!      connected agent (`ctx_init`) or an `llm` adapter (`--deepen`)
pub mod deepen;
pub mod history;
pub mod render;
pub mod scan;
pub mod symbols;

use crate::app::App;
use crate::util;
use anyhow::Result;
use history::History;
use scan::Inventory;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhaseInfo {
    pub at: String,
    pub summary: String,
    pub ms: u64,
    pub head: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitState {
    pub version: u32,
    pub phases: BTreeMap<String, PhaseInfo>,
    /// Deepen tasks finished or skipped in this worktree (task id → date/note).
    pub done: BTreeMap<String, String>,
}

fn state_path(app: &App) -> PathBuf {
    app.repo.worktree_state_dir().join("init.json")
}
/// Finished/skipped deepen tasks are shared by all worktrees of the clone (Superset/agent worktrees).
fn done_path(app: &App) -> PathBuf {
    app.repo.state_dir().join("init-done.json")
}
fn inv_path(app: &App) -> PathBuf {
    app.repo.worktree_state_dir().join("inventory.json")
}
fn hist_path(app: &App) -> PathBuf {
    app.repo.worktree_state_dir().join("history.json")
}

pub fn load_state(app: &App) -> InitState {
    let mut st: InitState = std::fs::read_to_string(state_path(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let shared: BTreeMap<String, String> =
        std::fs::read_to_string(done_path(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for (k, v) in shared {
        st.done.entry(k).or_insert(v);
    }
    st
}

pub fn save_state(app: &App, st: &InitState) -> Result<()> {
    let mut shared: BTreeMap<String, String> =
        std::fs::read_to_string(done_path(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for (k, v) in &st.done {
        shared.insert(k.clone(), v.clone());
    }
    util::write_atomic(&done_path(app), &serde_json::to_string_pretty(&shared)?)?;
    util::write_atomic(&state_path(app), &serde_json::to_string_pretty(st)?)
}

/// Run the deterministic phases (scan, history, render) without touching hooks — what an agent may
/// trigger on its own; the result is an uncommitted `.ai/` diff for review.
pub fn bootstrap(dir: &Path) -> Result<String> {
    let lines = std::sync::Mutex::new(Vec::new());
    let opts = InitOpts { hooks: false, connect: Vec::new(), deepen: false, llm: None, jobs: 1, max_tasks: 0 };
    run(dir, &opts, &|l| lines.lock().unwrap().push(l.to_string()))?;
    Ok(lines.into_inner().unwrap().join("\n"))
}

pub fn compute(app: &App) -> Result<(Inventory, History, u64, u64)> {
    let cfg = &app.cfg().init;
    let t0 = Instant::now();
    let mut inv = scan::scan(&app.repo, &cfg.exclude, cfg.min_module_files, cfg.task_max_files)?;
    let scan_ms = t0.elapsed().as_millis() as u64;
    let t1 = Instant::now();
    let hist = history::mine(&app.repo, &inv.modules, cfg.history_max_commits, &knowledge_dirs(app))?;
    history::apply_churn(&mut inv.modules, &hist);
    scan::rank_modules(&mut inv.modules);
    let hist_ms = t1.elapsed().as_millis() as u64;
    Ok((inv, hist, scan_ms, hist_ms))
}

fn knowledge_dirs(app: &App) -> Vec<String> {
    let mut v: Vec<String> = app.cfg().kind_names().iter().map(|k| app.cfg().kind_path(k)).collect();
    v.push(app.cfg().store.dir.clone());
    // an ADR directory detected before the shared config exists
    for d in ["docs/adr", "docs/adrs", "docs/decisions", "adr", "doc/adr"] {
        if app.repo.abs(d).is_dir() {
            v.push(d.to_string());
        }
    }
    v
}

fn save_cache(app: &App, inv: &Inventory, hist: &History) -> Result<()> {
    util::write_atomic(&inv_path(app), &serde_json::to_string(inv)?)?;
    util::write_atomic(&hist_path(app), &serde_json::to_string(hist)?)?;
    Ok(())
}

/// The last scan's inventory as cached, however old (cheap: for the brief, never recomputed).
pub fn cached_inventory(app: &App) -> Option<Inventory> {
    std::fs::read_to_string(inv_path(app)).ok().and_then(|t| serde_json::from_str::<Inventory>(&t).ok())
}

/// Inventory + history from the cache when it matches HEAD, otherwise recomputed (and cached).
pub fn load_cached(app: &App) -> Result<(Inventory, History)> {
    let st = load_state(app);
    let head = app.repo.head();
    let fresh = st.phases.get("history").is_some_and(|p| p.head == head);
    if fresh {
        let inv = std::fs::read_to_string(inv_path(app)).ok().and_then(|t| serde_json::from_str::<Inventory>(&t).ok());
        let hist = std::fs::read_to_string(hist_path(app)).ok().and_then(|t| serde_json::from_str::<History>(&t).ok());
        if let (Some(i), Some(h)) = (inv, hist) {
            return Ok((i, h));
        }
    }
    let (inv, hist, scan_ms, hist_ms) = compute(app)?;
    save_cache(app, &inv, &hist)?;
    let mut st = load_state(app);
    st.phases.insert("scan".into(), PhaseInfo { at: util::now_iso(), summary: scan_summary(&inv), ms: scan_ms, head: head.clone() });
    st.phases.insert("history".into(), PhaseInfo { at: util::now_iso(), summary: history_summary(&hist), ms: hist_ms, head });
    save_state(app, &st)?;
    Ok((inv, hist))
}

fn scan_summary(inv: &Inventory) -> String {
    let code_total: usize = inv.languages.iter().filter(|l| scan::is_code(&l.lang)).map(|l| l.lines).sum();
    let top = inv
        .languages
        .iter()
        .find(|l| scan::is_code(&l.lang))
        .map(|l| format!("{} {}%", l.lang, (l.lines * 100).checked_div(code_total).unwrap_or(0)))
        .unwrap_or_else(|| "no code".into());
    let ws = if inv.workspace.is_empty() { String::new() } else { format!(" · {}", inv.workspace.join("/")) };
    format!("{} files · {top}{ws} · {} modules · {} manifests", inv.files, inv.modules.len(), inv.manifests.len())
}

fn history_summary(h: &History) -> String {
    format!("{} commits · {} decision-shaped in {} areas", h.commits, h.candidates.len(), h.clusters.len())
}

#[derive(Debug, Clone, Default)]
pub struct InitOpts {
    pub hooks: bool,
    pub connect: Vec<String>,
    pub deepen: bool,
    pub llm: Option<String>,
    pub jobs: usize,
    pub max_tasks: usize,
}

pub fn run(dir: &Path, opts: &InitOpts, out: &dyn Fn(&str)) -> Result<()> {
    let app = App::open(dir)?;
    out(&format!("kontext init · {}", app.repo.id));
    app.repo.prune_worktree_state();
    let head = app.repo.head();

    let t = Instant::now();
    let cfg = &app.cfg().init;
    let mut inv = scan::scan(&app.repo, &cfg.exclude, cfg.min_module_files, cfg.task_max_files)?;
    out(&format!("[1/5] scan     ✓ {} ({:.1}s)", scan_summary(&inv), t.elapsed().as_secs_f32()));
    let scan_ms = t.elapsed().as_millis() as u64;

    let t = Instant::now();
    let hist = history::mine(&app.repo, &inv.modules, cfg.history_max_commits, &knowledge_dirs(&app))?;
    history::apply_churn(&mut inv.modules, &hist);
    scan::rank_modules(&mut inv.modules);
    out(&format!("[2/5] history  ✓ {} ({:.1}s)", history_summary(&hist), t.elapsed().as_secs_f32()));
    let hist_ms = t.elapsed().as_millis() as u64;
    save_cache(&app, &inv, &hist)?;

    let t = Instant::now();
    let store_dir = app.cfg().store.dir.trim_end_matches('/').to_string();
    let project = app.cfg().project.name.clone().or_else(|| inv.root_name.clone()).unwrap_or_else(|| app.repo.name.clone());
    let cfg_written = render::write_config_if_missing(&app.repo, &store_dir, &project, &inv)?;
    // reload so a freshly written config (e.g. an existing ADR directory) applies
    let app = App::open(dir)?;
    let rep = render::render_all(&app, &inv, &hist)?;
    let _ = &project;
    let adr = inv.adr_dirs.first().map(|a| format!(" · decisions in {} ({} existing)", a.path, a.count)).unwrap_or_default();
    out(&format!(
        "[3/5] render   ✓ {}/architecture: overview{} + {} module docs ({} new, {} updated){adr}{} ({:.1}s)",
        store_dir,
        if rep.overview_written { " (written)" } else { "" },
        render::documented_modules(&inv, app.cfg().init.max_module_docs).len(),
        rep.modules_new,
        rep.modules_updated,
        cfg_written.map(|p| format!(" · config {p}")).unwrap_or_default(),
        t.elapsed().as_secs_f32()
    ));

    let mut st = load_state(&app);
    st.version = 1;
    st.phases.insert("scan".into(), PhaseInfo { at: util::now_iso(), summary: scan_summary(&inv), ms: scan_ms, head: head.clone() });
    st.phases.insert("history".into(), PhaseInfo { at: util::now_iso(), summary: history_summary(&hist), ms: hist_ms, head: head.clone() });
    st.phases.insert(
        "render".into(),
        PhaseInfo {
            at: util::now_iso(),
            summary: format!("{} module docs", rep.modules_new + rep.modules_updated),
            ms: 0,
            head: head.clone(),
        },
    );

    // wire
    let mut wired = Vec::new();
    if opts.hooks {
        match crate::hooks::install(&app, None) {
            Ok(r) => wired.push(r.summary()),
            Err(e) => wired.push(format!("hooks not installed: {e:#}")),
        }
    } else {
        wired.push("hooks skipped (--no-hooks)".into());
    }
    for target in &opts.connect {
        match crate::connect::connect(&app, target, true) {
            Ok(msg) => wired.push(msg.lines().next().unwrap_or("").to_string()),
            Err(e) => wired.push(format!("{target}: {e:#}")),
        }
    }
    out(&format!("[4/5] wire     ✓ {}", wired.join(" · ")));
    st.phases.insert("wire".into(), PhaseInfo { at: util::now_iso(), summary: wired.join("; "), ms: 0, head: head.clone() });
    save_state(&app, &st)?;

    // deepen
    let p = deepen::plan(&app, &inv, &hist, &st);
    let llm = opts.llm.clone().or_else(|| app.cfg().init.llm.clone());
    if opts.deepen {
        let Some(llm) = llm else {
            out("[5/5] deepen   ✗ no llm adapter: pass --llm <adapter> or set init.llm (presets: llm-claude, llm-codex, llm-ollama)");
            return Ok(());
        };
        out(&format!(
            "[5/5] deepen   … {}/{} done, running up to {} task(s) with '{llm}' ({} at a time)",
            p.done, p.total, opts.max_tasks, opts.jobs
        ));
        let (ok, failed) = deepen::autopilot(&app, &llm, opts.jobs, opts.max_tasks, &|line| out(line))?;
        let after = deepen::plan(&app, &inv, &hist, &load_state(&app));
        out(&format!(
            "      deepen   {} ✓ {ok} · ✗ {failed} · progress {}/{}",
            if after.pending.is_empty() { "complete" } else { "partial" },
            after.done,
            after.total
        ));
    } else {
        out(&format!(
            "[5/5] deepen   · {}/{} tasks done — let your agent continue with the `kontext-init` prompt (tools ctx_init / ctx_init_submit), or run `kontext init --deepen --llm <adapter>`",
            p.done, p.total
        ));
    }
    out(&format!(
        "\nReview with `git status -- {store_dir}` / `git diff -- {store_dir}`, then commit on a branch so the team reviews the bootstrap."
    ));
    Ok(())
}

pub fn status_text(app: &App) -> Result<String> {
    let st = load_state(app);
    let mut s = String::new();
    for ph in ["scan", "history", "render", "wire"] {
        match st.phases.get(ph) {
            Some(p) => s.push_str(&format!("  {ph:<8} ✓ {} ({})\n", p.summary, p.at.chars().take(16).collect::<String>())),
            None => s.push_str(&format!("  {ph:<8} · not run\n")),
        }
    }
    if st.phases.contains_key("render") || app.repo.abs(&format!("{}/architecture/overview.md", app.cfg().store.dir)).exists() {
        let (inv, hist) = load_cached(app)?;
        let p = deepen::plan(app, &inv, &hist, &load_state(app));
        s.push_str(&format!("  deepen   {}/{} tasks", p.done, p.total));
        if let Some(n) = p.pending.first() {
            s.push_str(&format!(" — next `{}`: {}", n.id, n.title));
        }
        s.push('\n');
    }
    Ok(s)
}
