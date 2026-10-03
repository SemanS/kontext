//! Which repository a path belongs to. An agent's session starts in one directory but its work
//! reaches further: a submodule with its own team knowledge, another worktree of the same clone,
//! a sibling project. kontext answers from the repository that owns the paths in question.
use crate::app::{self, App};
use anyhow::{Result, bail};
use std::path::Path;
use std::sync::Arc;

/// A repository nested in this worktree: a submodule, or a clone inside it.
pub struct Nested {
    /// Its root relative to the outer repository's root, e.g. `extractor`.
    pub rel: String,
    pub app: Arc<App>,
}

/// Whether `path` lies inside `root` once symlinks are resolved: a committed symlink or an
/// absolute `.gitmodules` path must not pull an unrelated repository into this one's answers.
fn inside(root: &Path, path: &Path) -> bool {
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(r), Ok(p)) => p != r && p.starts_with(&r),
        _ => false,
    }
}

/// The repository rooted at `rel` in `app`'s worktree (really inside it, and not `app` itself),
/// and whether it is a separate one: a worktree of the same clone kept inside this one (the
/// `.claude/worktrees/<name>` layout) is another worktree, not a submodule with knowledge of its own.
fn inner_repo(app: &App, rel: &str) -> Option<(Arc<App>, bool)> {
    let dir = app.repo.root.join(rel);
    if !dir.join(".git").exists() || !inside(&app.repo.root, &dir) {
        return None;
    }
    let a = app::open_shared(&dir).ok()?;
    let separate = a.repo.common_dir != app.repo.common_dir;
    (a.repo.root != app.repo.root).then_some((a, separate))
}

/// A separate repository rooted at `rel` in `app`'s worktree (a submodule, a nested clone).
fn nested_app(app: &App, rel: &str) -> Option<Arc<App>> {
    inner_repo(app, rel).filter(|(_, separate)| *separate).map(|(a, _)| a)
}

/// Initialized submodules, from `.gitmodules` (no git process).
fn submodule_paths(app: &App) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(app.repo.root.join(".gitmodules")) else { return Vec::new() };
    let mut out: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "path").then(|| v.trim().trim_matches('"').trim_end_matches('/').to_string())
        })
        .filter(|p| !p.is_empty() && !p.starts_with('/') && !p.split('/').any(|s| s == ".."))
        .filter(|p| app.repo.root.join(p).join(".git").exists())
        .collect();
    out.dedup();
    out
}

/// Nested repositories (initialized submodules), opened.
pub fn nested_repos(app: &App) -> Vec<Nested> {
    submodule_paths(app).into_iter().filter_map(|rel| nested_app(app, &rel).map(|a| Nested { rel, app: a })).collect()
}

/// Nested repositories that keep their own team knowledge.
pub fn nested_stores(app: &App) -> Vec<Nested> {
    submodule_paths(app)
        .into_iter()
        // a store dir or a shared config, before paying for opening the repository
        .filter(|rel| app.repo.root.join(rel).join(".ai").is_dir() || app.repo.root.join(rel).join(".kontext.toml").is_file())
        .filter_map(|rel| nested_app(app, &rel).filter(|a| a.is_set_up()).map(|a| Nested { rel, app: a }))
        .collect()
}

/// The innermost repository inside `app`'s worktree that contains `rel` (which need not exist
/// yet): its root relative to `app`'s, the path inside it, the repository, and whether it is a
/// separate repository rather than another worktree of the same clone.
fn inner_of(app: &App, rel: &str) -> Option<(String, String, Arc<App>, bool)> {
    let rel = rel.trim_start_matches("./").trim_end_matches('/');
    if rel.is_empty() || rel == "." || rel.split('/').any(|s| s == "..") {
        return None;
    }
    let mut cur = Path::new(rel);
    while !cur.as_os_str().is_empty() {
        let root = cur.to_string_lossy().replace('\\', "/");
        if app.repo.root.join(cur).join(".git").exists()
            && let Some((a, separate)) = inner_repo(app, &root)
        {
            let inner = rel.get(root.len()..).unwrap_or("").trim_start_matches('/').to_string();
            return Some((root, inner, a, separate));
        }
        cur = cur.parent()?;
    }
    None
}

/// The innermost separate repository nested in `app`'s worktree that contains `rel`: its root
/// relative to `app`'s, the path inside it, and the repository.
pub fn nested_of(app: &App, rel: &str) -> Option<(String, String, Arc<App>)> {
    inner_of(app, rel).filter(|x| x.3).map(|(root, inner, a, _)| (root, inner, a))
}

/// Where a path named by an agent lives.
pub enum Place {
    /// in this repository (relative to its root)
    Here(String),
    /// in another repository: one nested in this worktree (`nested` = its root, relative), or elsewhere
    Other { app: Arc<App>, rel: String, nested: Option<String> },
    /// nowhere on disk
    Missing,
}

/// `path` relative to `root` when it lies inside it.
pub fn rel_to(root: &Path, path: &Path) -> Option<String> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    canon.strip_prefix(&root).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
}

pub fn locate(app: &App, raw: &str) -> Place {
    let raw = raw.trim();
    let p = Path::new(raw);
    if p.is_absolute() {
        if let Some(rel) = rel_to(&app.repo.root, p) {
            return locate(app, &rel);
        }
        if !p.exists() {
            return Place::Missing;
        }
        return match app::open_shared(p) {
            Ok(other) if other.repo.root != app.repo.root => {
                let rel = rel_to(&other.repo.root, p).unwrap_or_default();
                Place::Other { app: other, rel, nested: None }
            }
            _ => Place::Missing,
        };
    }
    let rel = raw.trim_start_matches("./").trim_end_matches('/');
    if rel.is_empty() || rel == "." {
        return Place::Here(String::new());
    }
    if rel.split('/').any(|s| s == "..") {
        return Place::Missing;
    }
    // under a submodule's root, existing or not (a file about to be written belongs there too);
    // a worktree of this clone kept inside this one is another worktree, as an absolute path would be
    if let Some((root, inner, n, separate)) = inner_of(app, rel) {
        return Place::Other { app: n, rel: inner, nested: separate.then_some(root) };
    }
    if app.repo.abs(rel).exists() {
        return Place::Here(rel.to_string());
    }
    // a path relative to a submodule (what its own AGENTS.md teaches): `apps/runner` for
    // `extractor/apps/runner` — when exactly one submodule has it
    let mut found = submodule_paths(app).into_iter().filter(|sub| app.repo.root.join(sub).join(rel).exists());
    match (found.next(), found.next()) {
        (Some(sub), None) => match nested_app(app, &sub) {
            Some(n) => Place::Other { app: n, rel: rel.to_string(), nested: Some(sub) },
            None => Place::Missing,
        },
        _ => Place::Missing,
    }
}

/// The repository a tool's `dir` names: a path inside it, absolute or relative to `app`'s root.
pub fn open_dir(app: &App, dir: &str) -> Result<Arc<App>> {
    let p = Path::new(dir.trim());
    let abs = if p.is_absolute() { p.to_path_buf() } else { app.repo.root.join(p) };
    if !abs.exists() {
        bail!("dir '{dir}' does not exist — give a path inside the repository, worktree or submodule to use");
    }
    app::open_shared(&abs)
}

/// How another repository is reached from `from`: `extractor/` for a nested one, else its path.
pub fn label(from: &App, to: &App) -> String {
    match rel_to(&from.repo.root, &to.repo.root) {
        Some(rel) if !rel.is_empty() => format!("{rel}/"),
        _ => to.repo.root.display().to_string(),
    }
}

/// The `dir` value that names `to` from `from`.
pub fn dir_arg(from: &App, to: &App) -> String {
    label(from, to).trim_end_matches('/').to_string()
}

/// One line saying which repository answered, when it is not the one asked.
pub fn banner(from: &App, to: &App) -> String {
    let branch = to.repo.branch.as_deref().map(|b| format!(", branch {b}")).unwrap_or_default();
    format!(
        "(answered from {} at {}{branch} — the other kontext tools take `dir: \"{}\"` for it)",
        to.project_name(),
        label(from, to),
        dir_arg(from, to)
    )
}

/// The part of a path or glob before its first wildcard, cut at a whole segment.
fn literal_prefix(p: &str) -> &str {
    match p.find(['*', '?', '[', '{']) {
        Some(i) => p[..i].rfind('/').map(|j| &p[..j]).unwrap_or(""),
        None => p.trim_end_matches('/'),
    }
}

/// Where a capture goes, and its paths as that repository sees them.
pub struct Routing {
    /// another repository that owns every path (and keeps team knowledge), when not this one
    pub owner: Option<Arc<App>>,
    pub paths: Vec<String>,
    pub notes: Vec<String>,
}

/// Where a capture with these paths belongs. It moves to another repository only when every path
/// is that repository's and it has a knowledge store to promote into; otherwise it stays here,
/// with its paths relative to this root and a note about what did not fit.
pub fn owner_of(app: &App, paths: &[String]) -> Routing {
    let mut other: Option<Arc<App>> = None;
    let (mut here_paths, mut there_paths) = (Vec::new(), Vec::new());
    let (mut here, mut split) = (false, false);
    let mut missing = Vec::new();
    for p in paths {
        let lit = literal_prefix(p);
        let tail = p.get(lit.len()..).unwrap_or("").trim_start_matches('/');
        match locate(app, if lit.is_empty() { "." } else { lit }) {
            Place::Here(rel) => {
                here = true;
                // an absolute path into this worktree is kept relative, like every other path
                let r = if Path::new(p).is_absolute() { join(&rel, tail) } else { p.clone() };
                here_paths.push(r.clone());
                there_paths.push(r);
            }
            Place::Missing => {
                missing.push(p.clone());
                here_paths.push(p.clone());
                there_paths.push(p.clone());
            }
            Place::Other { app: o, rel, nested } => {
                match &other {
                    Some(t) if t.repo.root != o.repo.root => split = true,
                    Some(_) => {}
                    None => other = Some(o.clone()),
                }
                // seen from here: under the submodule's directory, or as given (absolute)
                here_paths.push(match &nested {
                    Some(root) => join(root, &join(&rel, tail)),
                    None => p.clone(),
                });
                there_paths.push(join(&rel, tail));
            }
        }
    }
    let mut notes = Vec::new();
    // a path that exists nowhere yet may be this repository's next file: it keeps the capture here
    let owner = other.clone().filter(|t| !here && !split && missing.is_empty() && t.is_set_up());
    match (&owner, &other) {
        (Some(_), _) | (None, None) => {}
        (None, Some(t)) => {
            let note = if !t.is_set_up() {
                // a team capture there would be refused: only private notes can go
                format!(
                    "kept in {}: {} keeps no team knowledge (a private note can go there with `dir: \"{}\"`)",
                    app.project_name(),
                    label(app, t),
                    dir_arg(app, t)
                )
            } else if here || split || !missing.is_empty() {
                format!(
                    "kept in {}: these paths span more than one repository — capture what concerns {} there with `dir: \"{}\"`",
                    app.project_name(),
                    label(app, t),
                    dir_arg(app, t)
                )
            } else {
                String::new()
            };
            if !note.is_empty() {
                notes.push(note);
            }
        }
    }
    if owner.is_none() && !missing.is_empty() {
        notes.push(format!(
            "not found in {}: {} — if they belong to another repository, worktree or submodule, capture it there with `dir`",
            app.project_name(),
            missing.join(", ")
        ));
    }
    let paths = if owner.is_some() { there_paths } else { here_paths };
    Routing { owner, paths, notes }
}

/// `rel/tail` without stray slashes; the whole repository when both are empty.
fn join(rel: &str, tail: &str) -> String {
    match (rel.is_empty(), tail.is_empty()) {
        (true, true) => "**".to_string(),
        (true, false) => tail.to_string(),
        (false, true) => rel.to_string(),
        (false, false) => format!("{rel}/{tail}"),
    }
}

/// A nested repository's URI as seen from the outer one: `kx:<rel>/<id>`, `git:<rel>/<sha>`, `file:<rel>/<path>`.
pub fn prefix_uri(rel: &str, uri: &str) -> String {
    for scheme in ["kx:", "git:", "file:", "inbox:"] {
        if let Some(rest) = uri.strip_prefix(scheme) {
            return format!("{scheme}{rel}/{rest}");
        }
    }
    uri.to_string()
}

/// Focus items for a brief, split by the repository they belong to.
pub struct FocusSplit {
    /// paths of this repository and topic words
    pub here: Vec<String>,
    pub here_paths: usize,
    /// paths of other repositories: (repository, nested root when nested here, paths relative to it)
    pub elsewhere: Vec<(Arc<App>, Option<String>, Vec<String>)>,
    pub words: Vec<String>,
}

pub fn split_focus(app: &App, focus: &[String]) -> FocusSplit {
    let mut s = FocusSplit { here: Vec::new(), here_paths: 0, elsewhere: Vec::new(), words: Vec::new() };
    for item in focus {
        let t = item.trim().trim_start_matches("./");
        if t.is_empty() {
            continue;
        }
        let path = t.split(':').next().unwrap_or(t);
        match locate(app, path) {
            Place::Here(rel) => {
                s.here.push(if rel.is_empty() { ".".into() } else { rel });
                s.here_paths += 1;
            }
            // a submodule without team knowledge has nothing of its own to add: its paths stay
            // this repository's focus (its AGENTS.md files, its modules), as before
            Place::Other { app: o, rel, nested: Some(root) } if !o.is_set_up() => {
                s.here.push(join(&root, &rel));
                s.here_paths += 1;
            }
            Place::Other { app: o, rel, nested } => {
                let rel = if rel.is_empty() { ".".to_string() } else { rel };
                match s.elsewhere.iter_mut().find(|(a, _, _)| a.repo.root == o.repo.root) {
                    Some((_, _, list)) => list.push(rel),
                    None => s.elsewhere.push((o, nested, vec![rel])),
                }
            }
            Place::Missing => {
                s.here.push(t.to_string());
                s.words.push(t.to_string());
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_prefixes() {
        assert_eq!(literal_prefix("extractor/apps/**"), "extractor/apps");
        assert_eq!(literal_prefix("src/*.rs"), "src");
        assert_eq!(literal_prefix("*.md"), "");
        assert_eq!(literal_prefix("plugins/youtube/core/src/lib.rs"), "plugins/youtube/core/src/lib.rs");
        assert_eq!(literal_prefix("docs/"), "docs");
    }

    #[test]
    fn uris_get_the_nested_path() {
        assert_eq!(prefix_uri("extractor", "kx:2026-09-30-keep"), "kx:extractor/2026-09-30-keep");
        assert_eq!(prefix_uri("extractor", "git:4238a17"), "git:extractor/4238a17");
        assert_eq!(prefix_uri("extractor", "file:README.md#run"), "file:extractor/README.md#run");
        assert_eq!(prefix_uri("extractor", "viking://x"), "viking://x");
    }
}
