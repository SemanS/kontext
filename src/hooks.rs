//! Git hook installation and handlers.
//!
//! Installation never replaces an existing hook: a marked block is inserted after the shebang
//! (or the original is chained when it is not a shell script). Every block is guarded by a
//! `kontext` lookup, so teammates without kontext are unaffected when hooks are shared.
use crate::app::App;
use crate::events::{self, Outbox};
use crate::inbox::Inbox;
use crate::ops;
use crate::store::Entry;
use crate::util;
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const HOOKS: &[&str] = &["pre-commit", "prepare-commit-msg", "post-commit", "post-merge", "post-rewrite"];
const START: &str = "# >>> kontext >>> (managed by `kontext hooks install`; remove with `kontext hooks uninstall`)";
const START_PREFIX: &str = "# >>> kontext >>>";
const END: &str = "# <<< kontext <<<";

fn block(hook: &str) -> String {
    let find = r#"k=$(command -v kontext 2>/dev/null || true); [ -z "$k" ] && [ -x "$HOME/.local/bin/kontext" ] && k="$HOME/.local/bin/kontext"; [ -z "$k" ] && [ -x "$HOME/.cargo/bin/kontext" ] && k="$HOME/.cargo/bin/kontext""#;
    let call = match hook {
        "pre-commit" => r#"if [ -n "$k" ]; then "$k" hook pre-commit || exit $?; fi"#.to_string(),
        "prepare-commit-msg" => r#"if [ -n "$k" ]; then "$k" hook prepare-commit-msg "$@" || true; fi"#.to_string(),
        other => format!(r#"if [ -n "$k" ]; then "$k" hook {other} "$@" >/dev/null 2>&1 || true; fi"#),
    };
    format!("{START}\n{find}\n{call}\n{END}\n")
}

#[derive(Debug, Default)]
pub struct InstallReport {
    pub dir: PathBuf,
    pub tracked: bool,
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub chained: Vec<String>,
    pub notes: Vec<String>,
}

impl InstallReport {
    pub fn summary(&self) -> String {
        let mut names: Vec<&str> =
            self.installed.iter().chain(self.updated.iter()).chain(self.chained.iter()).map(String::as_str).collect();
        names.sort();
        let mut s =
            format!("git hooks in {}: {}", self.dir.display(), if names.is_empty() { "up to date".into() } else { names.join(", ") });
        if self.tracked {
            s.push_str(" (tracked dir — commit it to share with the team)");
        }
        for n in &self.notes {
            s.push_str(&format!("\n  note: {n}"));
        }
        s
    }
}

/// Where git will look for hooks, and whether that directory is part of the repository.
pub fn hooks_dir(app: &App) -> Result<(PathBuf, bool)> {
    if let Some(hp) = app.repo.git_opt(&["config", "--get", "core.hooksPath"]).filter(|s| !s.is_empty()) {
        let hp = if let Some(rest) = hp.strip_prefix("~/") { util::home_dir().join(rest) } else { PathBuf::from(&hp) };
        let abs = if hp.is_absolute() { hp } else { app.repo.root.join(hp) };
        // husky v9: git runs `.husky/_/<hook>`, which sources the user script `.husky/<hook>`
        let abs = if abs.ends_with(".husky/_") { abs.parent().unwrap().to_path_buf() } else { abs };
        let tracked = app
            .repo
            .rel(&abs)
            .is_some_and(|rel| !rel.is_empty() && !app.repo.git_opt(&["ls-files", "--", &rel]).unwrap_or_default().is_empty());
        return Ok((abs, tracked));
    }
    let p = app.repo.git(&["rev-parse", "--path-format=absolute", "--git-path", "hooks"])?;
    Ok((PathBuf::from(p.trim()), false))
}

fn is_shell(first_line: &str) -> bool {
    let l = first_line.trim();
    !l.starts_with("#!") || ["/sh", "/bash", "/zsh", "/dash", "env sh", "env bash", "env zsh"].iter().any(|s| l.contains(s))
}

fn remove_block(text: &str) -> Option<String> {
    let s = text.find(START_PREFIX)?;
    let e = text[s..].find(END)? + s + END.len();
    let mut out = String::from(&text[..s]);
    out.push_str(text[e..].trim_start_matches('\n'));
    Some(out)
}

fn make_executable(p: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(p)?.permissions();
        perm.set_mode(perm.mode() | 0o755);
        std::fs::set_permissions(p, perm)?;
    }
    Ok(())
}

pub fn install(app: &App, dir_override: Option<PathBuf>) -> Result<InstallReport> {
    let (dir, tracked) = match dir_override {
        Some(d) => {
            let abs = if d.is_absolute() { d } else { app.repo.root.join(d) };
            let tracked = app.repo.rel(&abs).is_some_and(|rel| !app.repo.git_opt(&["ls-files", "--", &rel]).unwrap_or_default().is_empty());
            (abs, tracked)
        }
        None => hooks_dir(app)?,
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let mut rep = InstallReport { dir: dir.clone(), tracked, ..Default::default() };
    for hook in HOOKS {
        let path = dir.join(hook);
        let blk = block(hook);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if text.contains(START_PREFIX) {
                    let without = remove_block(&text).unwrap_or(text.clone());
                    let next = insert_block(&without, &blk);
                    if next != text {
                        std::fs::write(&path, next)?;
                        rep.updated.push(hook.to_string());
                    }
                } else if is_shell(text.lines().next().unwrap_or("")) {
                    std::fs::write(&path, insert_block(&text, &blk))?;
                    rep.updated.push(hook.to_string());
                } else {
                    let chained = dir.join(format!("{hook}.kontext-chained"));
                    std::fs::rename(&path, &chained)?;
                    std::fs::write(&path, format!("#!/bin/sh\n{blk}exec \"$(dirname \"$0\")/{hook}.kontext-chained\" \"$@\"\n"))?;
                    rep.chained.push(hook.to_string());
                }
            }
            Err(_) => {
                std::fs::write(&path, format!("#!/bin/sh\n{blk}"))?;
                rep.installed.push(hook.to_string());
            }
        }
        make_executable(&path)?;
    }
    // hook managers that regenerate hook files would drop the block
    for (marker, name) in [("lefthook.yml", "lefthook"), (".pre-commit-config.yaml", "pre-commit")] {
        if app.repo.root.join(marker).exists() {
            rep.notes.push(format!("{name} manages hooks here; if it rewrites them, add `kontext hook <name>` to its config instead"));
        }
    }
    if !tracked {
        for cand in [".githooks", ".husky", "githooks", ".hooks"] {
            let d = app.repo.root.join(cand);
            if d.is_dir() && d != dir {
                rep.notes.push(format!("{cand}/ exists and may be activated later (e.g. `core.hooksPath` set by npm install); run `kontext hooks install --dir {cand}` to cover it too"));
            }
        }
    }
    Ok(rep)
}

fn insert_block(text: &str, blk: &str) -> String {
    match text.split_once('\n') {
        Some((first, rest)) if first.starts_with("#!") => format!("{first}\n{blk}{rest}"),
        _ if text.starts_with("#!") => format!("{text}\n{blk}"),
        _ => format!("{blk}{text}"),
    }
}

pub fn uninstall(app: &App, dir_override: Option<PathBuf>) -> Result<Vec<String>> {
    let dir = match dir_override {
        Some(d) if d.is_absolute() => d,
        Some(d) => app.repo.root.join(d),
        None => hooks_dir(app)?.0,
    };
    let mut removed = Vec::new();
    for hook in HOOKS {
        let path = dir.join(hook);
        let chained = dir.join(format!("{hook}.kontext-chained"));
        if chained.exists() {
            std::fs::rename(&chained, &path)?;
            removed.push(format!("{hook} (restored original)"));
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if let Some(rest) = remove_block(&text) {
            if rest.trim().is_empty() || rest.trim() == "#!/bin/sh" {
                std::fs::remove_file(&path)?;
            } else {
                std::fs::write(&path, rest)?;
            }
            removed.push(hook.to_string());
        }
    }
    Ok(removed)
}

pub fn status(app: &App) -> Result<String> {
    let (dir, tracked) = hooks_dir(app)?;
    let mut s = format!("hooks dir: {}{}\n", dir.display(), if tracked { " (tracked)" } else { "" });
    for hook in HOOKS {
        let state = match std::fs::read_to_string(dir.join(hook)) {
            Ok(t) if t.contains(START_PREFIX) => "kontext ✓",
            Ok(_) => "other hook (no kontext block)",
            Err(_) => "—",
        };
        s.push_str(&format!("  {hook:<20} {state}\n"));
    }
    Ok(s)
}

// ------------------------------------------------------------------------------------ handlers

pub fn run(app: &App, name: &str, args: &[String]) -> Result<i32> {
    if std::env::var("KONTEXT_SKIP").is_ok_and(|v| v == "1" || v == "true") {
        return Ok(0);
    }
    match name {
        "pre-commit" => pre_commit(app),
        "prepare-commit-msg" => {
            prepare_commit_msg(app, args)?;
            Ok(0)
        }
        "post-commit" | "post-merge" | "post-rewrite" => {
            let n = sync_events(app, false).unwrap_or(0);
            if name == "post-commit" {
                Inbox::open(&app.repo).cleanup_committed(&app.repo);
            }
            if (n > 0 || !Outbox::open(&app.repo).list().is_empty()) && app.cfg().adapters.values().any(|a| !a.on.is_empty()) {
                events::spawn_background_flush(&app.repo);
            }
            Ok(0)
        }
        other => bail!("unknown hook '{other}' (known: {})", HOOKS.join(", ")),
    }
}

fn pre_commit(app: &App) -> Result<i32> {
    let rep = ops::check_staged(app)?;
    let report = ops::render_check(&rep);
    if !report.is_empty() {
        eprint!("{}", report.lines().map(|l| format!("kontext: {l}\n")).collect::<String>());
    }
    if rep.errors() > 0 {
        eprintln!("kontext: commit blocked — fix the errors above (bypass once with KONTEXT_SKIP=1 or --no-verify).");
        return Ok(1);
    }
    let store = app.store();
    let staged = app.repo.staged_paths()?;
    let store_changed = staged.iter().any(|(_, p)| store.is_entry_path(p));
    if store_changed
        && app.cfg().store.index_file
        && let Err(e) = stage_index(app)
    {
        eprintln!("kontext: could not refresh the knowledge index: {e:#}");
    }
    if app.cfg().hooks.remind_inbox {
        let code: Vec<&String> = staged.iter().filter(|(_, p)| !store.is_store_path(p)).map(|(_, p)| p).collect();
        let related: Vec<String> = Inbox::open(&app.repo)
            .list()
            .into_iter()
            .filter(|e| e.visibility.as_deref() != Some("private"))
            .filter(|e| {
                code.iter().any(|p| e.paths.iter().any(|pat| crate::glob::path_matches(pat, p) || crate::glob::path_overlaps(pat, p)))
            })
            .map(|e| e.id)
            .collect();
        if !related.is_empty() {
            eprintln!(
                "kontext: {} inbox candidate(s) relate to this commit: {} — `kontext promote <id>` (then re-stage) to share them with it.",
                related.len(),
                related.join(", ")
            );
        }
    }
    Ok(0)
}

/// Entries exactly as staged, read in one `git cat-file --batch` call.
fn staged_entries(app: &App) -> Result<Vec<Entry>> {
    let store = app.store();
    let mut dirs: Vec<String> = app.cfg().kind_names().iter().map(|k| app.cfg().kind_path(k)).collect();
    dirs.sort();
    dirs.dedup();
    let mut args = vec!["ls-files", "-s", "-z", "--"];
    args.extend(dirs.iter().map(String::as_str));
    let listing = app.repo.git_bytes(&args)?;
    let mut blobs: Vec<(String, String)> = Vec::new();
    for rec in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let rec = String::from_utf8_lossy(rec);
        let Some((meta, path)) = rec.split_once('\t') else { continue };
        if !store.is_entry_path(path) {
            continue;
        }
        if let Some(sha) = meta.split_whitespace().nth(1) {
            blobs.push((path.to_string(), sha.to_string()));
        }
    }
    if blobs.is_empty() {
        return Ok(Vec::new());
    }
    let input: String = blobs.iter().map(|(_, s)| format!("{s}\n")).collect();
    let out = app.repo.git_with_stdin(&["cat-file", "--batch"], &input)?;
    let mut entries = Vec::new();
    let mut rest = out.as_str();
    for (path, _) in &blobs {
        let Some((header, after)) = rest.split_once('\n') else { break };
        let size: usize = header.split_whitespace().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let body = after.get(..size).unwrap_or(after);
        let kind = store.kind_for_path(path).unwrap_or_default();
        entries.push(Entry::parse(path, body, &kind));
        rest = after.get(size..).unwrap_or("").strip_prefix('\n').unwrap_or("");
    }
    Ok(entries)
}

fn stage_index(app: &App) -> Result<()> {
    let store = app.store();
    let entries = staged_entries(app)?;
    let rel = store.index_path();
    // render against the staged view, write it to the work tree and stage exactly that
    store.update_index_file(&entries)?;
    let content = std::fs::read_to_string(app.repo.abs(&rel))?;
    let blob = app.repo.git_with_stdin(&["hash-object", "-w", "--stdin"], &content)?;
    app.repo.git(&["update-index", "--add", "--cacheinfo", &format!("100644,{},{rel}", blob.trim())])?;
    Ok(())
}

fn prepare_commit_msg(app: &App, args: &[String]) -> Result<()> {
    if !app.cfg().hooks.trailers {
        return Ok(());
    }
    let Some(file) = args.first() else { return Ok(()) };
    let source = args.get(1).map(String::as_str).unwrap_or("");
    if matches!(source, "merge" | "squash") {
        return Ok(());
    }
    for t in ops::staged_trailers(app)? {
        app.repo.git(&["interpret-trailers", "--in-place", "--if-exists", "addIfDifferent", "--trailer", &t, file])?;
    }
    Ok(())
}

/// Compare knowledge at HEAD with the last synced snapshot and queue `sync` events for changes.
/// The first run only records the snapshot (use `backfill` to mirror everything once).
pub fn sync_events(app: &App, backfill: bool) -> Result<usize> {
    let snap_path = app.repo.worktree_state_dir().join("synced.json");
    let old: Option<BTreeMap<String, String>> = std::fs::read_to_string(&snap_path).ok().and_then(|t| serde_json::from_str(&t).ok());
    let store = app.store();
    let mut dirs: Vec<String> = app.cfg().kind_names().iter().map(|k| app.cfg().kind_path(k)).collect();
    dirs.sort();
    dirs.dedup();
    if app.repo.head().is_none() {
        return Ok(0);
    }
    let mut args = vec!["ls-tree", "-r", "-z", "HEAD", "--"];
    args.extend(dirs.iter().map(String::as_str));
    let listing = app.repo.git_bytes(&args)?;
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    for rec in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let rec = String::from_utf8_lossy(rec);
        let Some((meta, path)) = rec.split_once('\t') else { continue };
        if store.is_entry_path(path)
            && let Some(sha) = meta.split_whitespace().nth(2)
        {
            current.insert(path.to_string(), sha.to_string());
        }
    }
    let subscribed = app.cfg().adapters.values().any(|a| a.on.iter().any(|o| o.event == "sync"));
    let mut n = 0;
    if subscribed && (old.is_some() || backfill) {
        let outbox = Outbox::open(&app.repo);
        let empty = BTreeMap::new();
        let prev = if backfill { &empty } else { old.as_ref().unwrap() };
        for (path, sha) in &current {
            if prev.get(path) == Some(sha) {
                continue;
            }
            let Ok(text) = app.repo.git(&["cat-file", "blob", sha]) else { continue };
            let kind = store.kind_for_path(path).unwrap_or_default();
            let e = Entry::parse(path, &text, &kind);
            outbox.push("sync", events::entry_payload(&app.repo, &e, "team"))?;
            n += 1;
        }
    }
    util::write_atomic(&snap_path, &serde_json::to_string(&current)?)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_insert_and_remove() {
        let original = "#!/bin/sh\nset -eu\nnpm run vault:check\n";
        let with = insert_block(original, &block("pre-commit"));
        assert!(with.starts_with("#!/bin/sh\n# >>> kontext >>>"));
        assert!(with.ends_with("set -eu\nnpm run vault:check\n"));
        assert_eq!(remove_block(&with).unwrap(), original);
        let husky = "npx lint-staged\n";
        let with = insert_block(husky, &block("pre-commit"));
        assert!(with.starts_with("# >>> kontext >>>") && with.ends_with("npx lint-staged\n"));
        assert!(is_shell("#!/usr/bin/env bash") && !is_shell("#!/usr/bin/env node") && is_shell("npx lint-staged"));
    }
}
