//! Knowledge drift: recorded entries whose governed code changed in a range of commits (and, without
//! a head, in the working tree) while nobody looked at the entry — its own file was not edited,
//! retired or confirmed in the same range. Deterministic and model-free. It reads the working tree
//! the way `git status` does, so a jj working copy (uncommitted in git's terms) counts as well.
use crate::app::App;
use crate::glob::path_matches;
use crate::store::Entry;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fmt::Write;

/// Kinds checked unless asked otherwise. Module maps are refreshed by init, not confirmed.
pub const DEFAULT_KINDS: &[&str] = &["decision", "convention", "learning", "incident"];

/// An entry the range left behind.
pub struct Drifted {
    pub entry: Entry,
    /// Changed files that match the entry's `paths`, sorted.
    pub changed: Vec<String>,
    /// Commits of the range that touched them, newest first: (full sha, subject).
    pub commits: Vec<(String, String)>,
}

pub struct Report {
    pub base: String,
    /// The head commit, or None for the working tree.
    pub head: Option<String>,
    pub entries: Vec<Drifted>,
}

impl Report {
    /// `base..head` (or `base..working tree`) with short commit ids.
    pub fn range(&self) -> String {
        let short = |s: &str| s.chars().take(8).collect::<String>();
        format!("{}..{}", short(&self.base), self.head.as_deref().map(short).unwrap_or_else(|| "working tree".into()))
    }

    pub fn to_json(&self) -> Value {
        let entries: Vec<Value> = self
            .entries
            .iter()
            .map(|d| {
                let mut v = json!({"id":d.entry.id,"kind":d.entry.kind,"title":d.entry.title,"path":d.entry.rel_path,"changed":d.changed,
                    "commits":d.commits.iter().map(|(sha, subject)| json!({"sha":sha,"subject":subject})).collect::<Vec<_>>()});
                if let Some(date) = &d.entry.date {
                    v["date"] = json!(date);
                }
                if let Some(checked) = &d.entry.checked {
                    v["checked"] = json!(checked);
                }
                v
            })
            .collect();
        json!({"base":self.base,"head":self.head.as_deref().unwrap_or("working"),"entries":entries})
    }
}

fn invalid(message: String) -> anyhow::Error {
    crate::api::error("invalid_input", message)
}

/// The full id of the commit `rev` names. Options and odd bytes never reach git as a revision.
pub fn resolve(app: &App, rev: &str) -> Result<String> {
    let rev = rev.trim();
    if rev.is_empty() || rev.starts_with('-') || rev.len() > 256 || rev.contains(['\0', '\n', '\r']) {
        return Err(invalid(format!("'{rev}' is not a git revision")));
    }
    app.repo
        .git_opt(&["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("unknown revision '{rev}'")))
}

/// Where this branch left the remote's default branch: the merge base of HEAD with origin/HEAD
/// (else origin/main, else origin/master), else HEAD itself.
pub fn default_base(app: &App) -> Result<String> {
    let head = app.repo.head().ok_or_else(|| crate::api::error("no_commits", "the repository has no commits yet"))?;
    let remote = app
        .repo
        .git_opt(&["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
        .filter(|r| !r.is_empty())
        .into_iter()
        .chain(["refs/remotes/origin/main".to_string(), "refs/remotes/origin/master".to_string()])
        .find(|r| app.repo.git_status_ok(&["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")]));
    Ok(remote.and_then(|r| app.repo.git_opt(&["merge-base", "HEAD", &r])).filter(|s| !s.is_empty()).unwrap_or(head))
}

/// Non-merge commits of `base..head`, newest first, with the files each changed (a rename as both
/// of its ends).
fn commits(app: &App, base: &str, head: &str) -> Result<Vec<(String, String, Vec<String>)>> {
    let range = format!("{base}..{head}");
    let log = app.repo.git(&[
        "log",
        "--no-merges",
        "--no-show-signature",
        "--no-renames",
        "--name-only",
        "-z",
        "--format=%x00%H %s",
        &range,
        "--",
    ])?;
    Ok(crate::freshness::parse_log(&log)
        .into_iter()
        .map(|(header, paths)| {
            let (sha, subject) = header.split_once(' ').unwrap_or((header, ""));
            (sha.to_string(), subject.to_string(), paths.into_iter().map(String::from).collect())
        })
        .collect())
}

/// The entries as committed at `rev`, under the current configuration's kind directories.
fn entries_at(app: &App, rev: &str) -> Result<Vec<Entry>> {
    let store = app.store();
    let mut dirs: Vec<String> = app.cfg().kind_names().iter().map(|k| app.cfg().kind_path(k)).collect();
    dirs.sort();
    dirs.dedup();
    Ok(app
        .repo
        .files_at(rev, &dirs)?
        .into_iter()
        .filter(|(path, _)| store.is_entry_path(path))
        .map(|(path, bytes)| {
            let kind = store.kind_for_path(&path).unwrap_or_default();
            Entry::parse(&path, &String::from_utf8_lossy(&bytes), &kind)
        })
        .collect())
}

/// The drift of `base..head` (default base: [`default_base`]; no head: the working tree on top of
/// `base..HEAD`), for entries of `kinds` (default [`DEFAULT_KINDS`]), ordered by id.
pub fn drift(app: &App, base: Option<&str>, head: Option<&str>, kinds: &[String]) -> Result<Report> {
    let base = match base {
        Some(b) => resolve(app, b)?,
        None => default_base(app)?,
    };
    let head = head.map(|h| resolve(app, h)).transpose()?;
    let tip = match &head {
        Some(h) => h.clone(),
        None => app.repo.head().ok_or_else(|| crate::api::error("no_commits", "the repository has no commits yet"))?,
    };
    let commits = commits(app, &base, &tip)?;
    let mut touched: BTreeSet<String> = commits.iter().flat_map(|(_, _, paths)| paths.iter().cloned()).collect();
    let entries = match &head {
        Some(h) => entries_at(app, h)?,
        None => {
            touched.extend(app.repo.changed_paths()?.into_iter().map(|(_, p)| p));
            app.entries().0
        }
    };
    let store = app.store();
    // knowledge files are what an entry is checked against, not the code it governs
    let code: Vec<&String> = touched.iter().filter(|p| !store.is_store_path(p)).collect();
    let kinds: Vec<&str> = if kinds.is_empty() { DEFAULT_KINDS.to_vec() } else { kinds.iter().map(String::as_str).collect() };
    let mut out: Vec<Drifted> = Vec::new();
    for entry in entries {
        let patterns: Vec<&str> = entry.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
        if !entry.is_active() || patterns.is_empty() || !kinds.contains(&entry.kind.as_str()) || touched.contains(&entry.rel_path) {
            continue;
        }
        let governed = |f: &str| patterns.iter().any(|p| path_matches(p, f));
        let changed: Vec<String> = code.iter().filter(|f| governed(f)).map(|f| f.to_string()).collect();
        if changed.is_empty() {
            continue;
        }
        let commits = commits
            .iter()
            .filter(|(_, _, paths)| paths.iter().any(|f| !store.is_store_path(f) && governed(f)))
            .map(|(sha, subject, _)| (sha.clone(), subject.clone()))
            .collect();
        out.push(Drifted { entry, changed, commits });
    }
    out.sort_by(|a, b| a.entry.id.cmp(&b.entry.id).then(a.entry.rel_path.cmp(&b.entry.rel_path)));
    Ok(Report { base, head, entries: out })
}

/// What to do about each listed entry, for people (`kontext …`) or agents (`ctx_…`).
fn advice(agent: bool) -> &'static str {
    if agent {
        "For each, read it against the change: if it still holds, ctx_confirm it; if it needs a correction, edit it; if it no longer holds, record a replacement (ctx_capture with `supersedes: [<id>]`) or ctx_challenge it and retire it (ctx_prepare_commit retire=[<id>]). Each of these stages the entry with the change."
    } else {
        "For each, read it against the change: if it still holds, confirm it (`kontext confirm <id>`); if it needs a correction, edit it; if it no longer holds, supersede it (`kontext capture --supersedes <id> …`) or retire it (`kontext challenge <id> \"why\"`, then `kontext prepare-commit --retire <id>`)."
    }
}

fn list(items: &[String], max: usize) -> String {
    let mut s = items.iter().take(max).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > max {
        let _ = write!(s, " (+{} more)", items.len() - max);
    }
    s
}

/// The report as text.
pub fn render_text(report: &Report, agent: bool) -> String {
    let mut out = String::new();
    let n = report.entries.len();
    if n == 0 {
        let _ = writeln!(
            out,
            "No knowledge drift in {}: every entry that governs a changed file was edited or confirmed with it.",
            report.range()
        );
        return out;
    }
    let _ = writeln!(
        out,
        "Knowledge drift in {}: {n} entr{} govern{} changed files but {} not edited or confirmed in the range.\n",
        report.range(),
        if n == 1 { "y" } else { "ies" },
        if n == 1 { "s" } else { "" },
        if n == 1 { "was" } else { "were" }
    );
    for d in &report.entries {
        let _ = writeln!(out, "- [{}] {} <kx:{}> — {}", d.entry.kind, d.entry.title, d.entry.id, d.entry.rel_path);
        let _ = writeln!(out, "  changed: {}", list(&d.changed, 8));
        if !d.commits.is_empty() {
            let commits: Vec<String> = d.commits.iter().map(|(sha, subject)| format!("{} {subject}", &sha[..sha.len().min(8)])).collect();
            let _ = writeln!(out, "  commits: {}", list(&commits, 5));
        }
    }
    let _ = writeln!(out, "\n{}", advice(agent));
    out
}

/// The report as Markdown, for a CI job summary or a pull request comment.
pub fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    let n = report.entries.len();
    let _ = writeln!(out, "### Knowledge drift (`{}`)\n", report.range());
    if n == 0 {
        let _ = writeln!(out, "No drift: every entry that governs a changed file was edited or confirmed in this range.");
        return out;
    }
    let _ = writeln!(
        out,
        "{n} entr{} govern{} files changed in this range but {} not edited or confirmed:\n",
        if n == 1 { "y" } else { "ies" },
        if n == 1 { "s" } else { "" },
        if n == 1 { "was" } else { "were" }
    );
    let _ = writeln!(out, "| Entry | Kind | Changed files | Commits |\n| --- | --- | --- | --- |");
    let cell = |s: &str| s.replace('|', "\\|").replace(['\r', '\n'], " ");
    for d in &report.entries {
        let changed: Vec<String> = d.changed.iter().map(|f| format!("`{}`", cell(f))).collect();
        let commits: Vec<String> = d.commits.iter().map(|(sha, _)| format!("`{}`", &sha[..sha.len().min(8)])).collect();
        let _ = writeln!(
            out,
            "| {} (`{}`, [file]({})) | {} | {} | {} |",
            cell(&d.entry.title),
            cell(&d.entry.id),
            cell(&d.entry.rel_path),
            d.entry.kind,
            list(&changed, 5),
            if commits.is_empty() { "—".to_string() } else { list(&commits, 5) }
        );
    }
    let _ = writeln!(out, "\n{}", advice(false));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path =
                std::env::temp_dir().join(format!("kontext-drift-unit-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            let f = Self(path);
            f.git(&["init", "-q", "-b", "main"]);
            f.git(&["config", "user.email", "test@example.invalid"]);
            f.git(&["config", "user.name", "Test"]);
            f
        }
        fn git(&self, args: &[&str]) -> String {
            let out = std::process::Command::new("git").arg("-C").arg(&self.0).args(args).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8(out.stdout).unwrap().trim().into()
        }
        fn write(&self, path: &str, text: &str) {
            let p = self.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-qm", message]);
            self.git(&["rev-parse", "HEAD"])
        }
        fn app(&self) -> App {
            let mut app = App::open(&self.0).unwrap();
            app.loaded.cfg.adapters.clear();
            app
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ids(r: &Report) -> Vec<&str> {
        r.entries.iter().map(|d| d.entry.id.as_str()).collect()
    }

    #[test]
    fn a_range_lists_governing_entries_its_commits_left_alone() {
        let f = Fixture::new();
        let entry = |id: &str, kind: &str, paths: &str, status: &str| {
            f.write(
                &format!(".ai/learnings/{id}.md"),
                &format!("---\nid: {id}\nkind: {kind}\ntitle: T {id}\nstatus: {status}\npaths: [{paths}]\n---\nBody.\n"),
            )
        };
        entry("billing", "learning", "src/billing/**", "accepted");
        entry("glob", "convention", "'**/*.sql'", "accepted");
        entry("edited", "decision", "src/billing/**", "accepted");
        entry("gone", "decision", "src/billing/**", "deprecated");
        entry("map", "architecture", "src/billing/**", "accepted");
        entry("elsewhere", "incident", "src/api/**", "accepted");
        entry("unscoped", "learning", "", "accepted");
        f.write("src/billing/a.rs", "a\n");
        f.write("db/schema.sql", "create table t;\n");
        let base = f.commit("seed");
        f.write("src/billing/a.rs", "b\n");
        f.write("src/billing/new.rs", "n\n");
        let first = f.commit("feat(billing): change a");
        f.write(
            ".ai/learnings/edited.md",
            "---\nid: edited\nkind: decision\ntitle: T edited\nstatus: accepted\npaths: [src/billing/**]\n---\nBody, edited.\n",
        );
        f.write("db/schema.sql", "create table u;\n");
        f.commit("feat(db): schema");
        let app = f.app();
        let r = drift(&app, Some(&base), Some("HEAD"), &[]).unwrap();
        assert_eq!(ids(&r), vec!["billing", "glob"]);
        assert_eq!(r.entries[0].changed, vec!["src/billing/a.rs", "src/billing/new.rs"]);
        assert_eq!(r.entries[0].commits, vec![(first.clone(), "feat(billing): change a".to_string())]);
        assert_eq!(r.entries[1].changed, vec!["db/schema.sql"]);
        assert_eq!(r.head.as_deref(), Some(f.git(&["rev-parse", "HEAD"]).as_str()));
        let json = r.to_json();
        assert_eq!(json["base"], base.as_str());
        assert_eq!(json["entries"][0]["path"], ".ai/learnings/billing.md");
        assert!(json["entries"][0].get("date").is_none() && json["entries"][0].get("checked").is_none());
        // architecture only when asked for; an explicit head reads the entries as committed there
        let maps = drift(&app, Some(&base), Some(&first), &["architecture".to_string()]).unwrap();
        assert_eq!(ids(&maps), vec!["map"]);
        let at_first = drift(&app, Some(&base), Some(&first), &[]).unwrap();
        assert_eq!(ids(&at_first), vec!["billing", "edited"], "edited only later, after the range");
        assert!(drift(&app, Some(&base), Some(&base), &[]).unwrap().entries.is_empty());
        for bad in ["--output=x", "no-such-rev", ""] {
            let err = drift(&app, Some(bad), None, &[]).err().unwrap();
            assert_eq!(crate::api::error_code(&err), Some("invalid_input"), "{bad}");
        }
    }

    #[test]
    fn the_working_tree_counts_without_a_head_and_the_base_defaults_to_the_fork_point() {
        let f = Fixture::new();
        f.write(".ai/conventions/billing.md", "---\nid: billing\nkind: convention\ntitle: Billing\npaths: [src/billing/**]\n---\nBody.\n");
        f.write("src/billing/a.rs", "a\n");
        let base = f.commit("seed");
        // origin/main is where this branch started
        f.git(&["update-ref", "refs/remotes/origin/main", &base]);
        f.write("README.md", "later\n");
        f.commit("docs: readme");
        let app = f.app();
        assert_eq!(default_base(&app).unwrap(), base);
        assert!(drift(&app, None, None, &[]).unwrap().entries.is_empty());
        f.write("src/billing/untracked.rs", "u\n");
        let r = drift(&app, None, None, &[]).unwrap();
        assert_eq!(ids(&r), vec!["billing"]);
        assert!(r.head.is_none() && r.entries[0].commits.is_empty());
        assert_eq!(r.to_json()["head"], "working");
        assert!(render_text(&r, false).contains("`kontext confirm <id>`"));
        assert!(
            render_markdown(&r)
                .contains("| Billing (`billing`, [file](.ai/conventions/billing.md)) | convention | `src/billing/untracked.rs` | — |")
        );
        // confirming (or any edit of the entry) in the working tree counts as a look
        f.write(
            ".ai/conventions/billing.md",
            "---\nid: billing\nkind: convention\ntitle: Billing\nchecked: 2026-10-20\npaths: [src/billing/**]\n---\nBody.\n",
        );
        assert!(drift(&app, None, None, &[]).unwrap().entries.is_empty());
        f.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
        assert_eq!(default_base(&app).unwrap(), f.git(&["rev-parse", "HEAD"]), "no remote: HEAD");
    }
}
