//! Knowledge freshness: active decisions whose governed `paths` have seen many commits since the
//! decision was made. One `git log` covers every decision, and its result is cached per HEAD, so
//! the brief pays one `rev-parse` when nothing was committed.
use crate::app::App;
use crate::glob::path_matches;
use crate::store::Entry;
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A decision whose paths changed in at least `brief.stale_after_commits` commits since its date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub commits: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    key: String,
    counts: BTreeMap<String, usize>,
}

/// What a decision is checked against: its day (`YYYY-MM-DD`) and its path patterns.
struct Watched<'a> {
    id: &'a str,
    date: &'a str,
    paths: &'a [String],
}

fn watched(entries: &[Entry]) -> Vec<Watched<'_>> {
    entries
        .iter()
        .filter(|e| e.kind == "decision" && e.is_active() && !e.paths.is_empty())
        .filter_map(|e| {
            let date = e.date.as_deref()?.get(..10)?;
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some(Watched { id: &e.id, date, paths: &e.paths })
        })
        .collect()
}

/// Commits per decision that touch its paths on a later day than the decision (the decision's own
/// day is left out: that is usually the change it was recorded with). `commits` are `(day, files)`.
fn count(decisions: &[Watched], commits: &[(String, Vec<String>)]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for d in decisions {
        let n = commits
            .iter()
            .filter(|(day, files)| day.as_str() > d.date && files.iter().any(|f| d.paths.iter().any(|p| path_matches(p, f))))
            .count();
        counts.insert(d.id.to_string(), n);
    }
    counts
}

/// `(committer day, files)` of non-merge commits on HEAD since `since`, newest first, in one git call.
fn read_log(app: &App, since: &str) -> Vec<(String, Vec<String>)> {
    let max = format!("--max-count={}", app.cfg().index.max_commits.max(1));
    let since = format!("--since={since}T00:00:00");
    let Ok(out) = app.repo.git(&["log", "--no-merges", &max, &since, "--format=%x1e%cs", "--name-only", "HEAD"]) else {
        return Vec::new();
    };
    out.split('\x1e')
        .filter_map(|rec| {
            let mut lines = rec.lines().map(str::trim).filter(|l| !l.is_empty());
            let day = lines.next()?.to_string();
            Some((day, lines.map(str::to_string).collect()))
        })
        .collect()
}

/// Active decisions that may need a refresh, most-changed first. Empty when the threshold is 0,
/// there is no commit yet, or no decision has both a date and paths.
pub fn stale_decisions(app: &App, entries: &[Entry]) -> Vec<Stale> {
    let threshold = app.cfg().brief.stale_after_commits;
    if threshold == 0 {
        return Vec::new();
    }
    let decisions = watched(entries);
    if decisions.is_empty() {
        return Vec::new();
    }
    let Some(head) = app.repo.head() else { return Vec::new() };
    let fingerprint: String = decisions.iter().map(|d| format!("{}|{}|{};", d.id, d.date, d.paths.join(","))).collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("freshness.json");
    let cached: Option<Cache> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|c: &Cache| c.key == key);
    let counts = match cached {
        Some(c) => c.counts,
        None => {
            let since = decisions.iter().map(|d| d.date).min().unwrap_or_default();
            let counts = count(&decisions, &read_log(app, since));
            if let Ok(text) = serde_json::to_string(&Cache { key, counts: counts.clone() }) {
                let _ = std::fs::create_dir_all(app.repo.worktree_state_dir());
                let _ = util::write_atomic(&path, &text);
            }
            counts
        }
    };
    let mut stale: Vec<Stale> = entries
        .iter()
        .filter_map(|e| {
            let n = *counts.get(&e.id)?;
            (n >= threshold).then(|| Stale { id: e.id.clone(), short_id: e.short_id(), title: e.title.clone(), commits: n })
        })
        .collect();
    stale.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.id.cmp(&b.id)));
    stale
}

/// `[0007] Use cents (34 commits), …` for the first `max` stale decisions.
pub fn render_list(stale: &[Stale], max: usize) -> String {
    let mut s: Vec<String> = stale.iter().take(max).map(|d| format!("[{}] {} ({} commits)", d.short_id, d.title, d.commits)).collect();
    if stale.len() > max {
        s.push(format!("+{} more", stale.len() - max));
    }
    s.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(id: &str, date: Option<&str>, paths: &[&str], status: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            kind: "decision".into(),
            title: id.into(),
            date: date.map(str::to_string),
            status: status.map(str::to_string),
            paths: paths.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        }
    }

    fn commit(day: &str, files: &[&str]) -> (String, Vec<String>) {
        (day.into(), files.iter().map(|f| f.to_string()).collect())
    }

    #[test]
    fn watches_only_active_dated_decisions_with_paths() {
        let entries = vec![
            decision("a", Some("2026-01-05"), &["src/billing/**"], None),
            decision("no-paths", Some("2026-01-05"), &[], None),
            decision("no-date", None, &["src/**"], None),
            decision("bad-date", Some("last week"), &["src/**"], None),
            decision("old", Some("2026-01-05"), &["src/**"], Some("superseded")),
            Entry { kind: "convention".into(), ..decision("conv", Some("2026-01-05"), &["src/**"], None) },
        ];
        let ids: Vec<&str> = watched(&entries).iter().map(|w| w.id).collect();
        assert_eq!(ids, vec!["a"]);
    }

    #[test]
    fn counts_later_commits_on_governed_paths() {
        let entries = vec![
            decision("billing", Some("2026-01-05"), &["src/billing/**"], None),
            decision("api", Some("2026-02-01T10:00:00Z"), &["src/api", "docs/api.md"], None),
        ];
        let commits = vec![
            commit("2026-03-01", &["src/billing/invoice.rs", "src/api/routes.rs"]),
            commit("2026-02-01", &["src/api/routes.rs"]),  // the api decision's own day
            commit("2026-01-20", &["src/billing/tax.rs"]), // before the api decision
            commit("2026-01-10", &["README.md"]),
            commit("2026-01-05", &["src/billing/tax.rs"]), // the billing decision's own day
        ];
        let counts = count(&watched(&entries), &commits);
        assert_eq!(counts.get("billing"), Some(&2));
        assert_eq!(counts.get("api"), Some(&1));
    }

    #[test]
    fn renders_a_bounded_list() {
        let s = |id: &str, n| Stale { id: id.into(), short_id: id.into(), title: format!("T {id}"), commits: n };
        let list = vec![s("a", 30), s("b", 25), s("c", 21)];
        assert_eq!(render_list(&list, 2), "[a] T a (30 commits), [b] T b (25 commits), +1 more");
        assert_eq!(render_list(&list[..1], 3), "[a] T a (30 commits)");
    }
}
