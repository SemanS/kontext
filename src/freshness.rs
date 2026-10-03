//! Knowledge freshness: active decisions whose governed `paths` saw many commits since the
//! decision was made, a hint that the decision may need a review. One `git log` covers every
//! decision, and the counts are cached per HEAD: a brief pays one `rev-parse` while nothing is
//! committed.
use crate::app::App;
use crate::glob::path_matches;
use crate::store::Entry;
use crate::util;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A decision whose paths changed in at least `freshness.threshold_commits` commits since its date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    pub id: String,
    pub title: String,
    pub commits: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    key: String,
    counts: BTreeMap<String, usize>,
}

/// A decision under watch: from the first second (UTC) after its day, these path patterns.
struct Watched<'a> {
    id: &'a str,
    since: i64,
    paths: Vec<&'a str>,
}

/// The start of the UTC day after a decision's date. The decision's own day is left out: that is
/// usually the change it was recorded with. `2025-01-01` and `2025-01-01T12:00:00Z` name the same day.
fn cutoff(date: &str) -> Option<i64> {
    let day = date.get(..10)?;
    if !matches!(date.as_bytes().get(10), None | Some(b'T' | b' ')) {
        return None;
    }
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?.succ_opt()?.and_hms_opt(0, 0, 0).map(|d| d.and_utc().timestamp())
}

fn watched(entries: &[Entry]) -> Vec<Watched<'_>> {
    entries
        .iter()
        .filter(|e| e.kind == "decision" && e.is_active())
        .filter_map(|e| {
            let paths: Vec<&str> = e.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
            if paths.is_empty() {
                return None;
            }
            Some(Watched { id: &e.id, since: cutoff(e.date.as_deref()?)?, paths })
        })
        .collect()
}

struct Commit<'a> {
    time: i64,
    paths: Vec<&'a str>,
}

/// `git log -z --name-only --format=%x00%ct`: a NUL-prefixed timestamp starts a commit, and its paths
/// follow NUL-terminated (tabs and newlines included), the first one after a newline git inserts.
fn parse_history(history: &str) -> Vec<Commit<'_>> {
    let mut commits: Vec<Commit<'_>> = Vec::new();
    let mut header = true;
    let mut first_path = false;
    for field in history.split('\0') {
        if field.is_empty() {
            header = true;
        } else if header {
            if let Ok(time) = field.parse() {
                commits.push(Commit { time, paths: Vec::new() });
                first_path = true;
            }
            header = false;
        } else if let Some(commit) = commits.last_mut() {
            let path = if first_path { field.strip_prefix('\n').unwrap_or(field) } else { field };
            first_path = false;
            if !path.is_empty() {
                commit.paths.push(path);
            }
        }
    }
    commits
}

/// Matching commits per decision; a commit counts once, however many of its files match.
fn count(decisions: &[Watched], commits: &[Commit]) -> BTreeMap<String, usize> {
    decisions
        .iter()
        .map(|d| {
            let n =
                commits.iter().filter(|c| c.time >= d.since && c.paths.iter().any(|f| d.paths.iter().any(|p| path_matches(p, f)))).count();
            (d.id.to_string(), n)
        })
        .collect()
}

/// One history read for every decision: non-merge commits of HEAD from a day before the oldest
/// cutoff (the count then compares exact times), a rename as both of its ends (moving code out of
/// governed paths changes them too), and no signature output, which would land among the paths.
fn read_history(app: &App, head: &str, oldest: i64) -> Option<String> {
    let max = format!("--max-count={}", app.cfg().index.max_commits.max(1));
    let from = chrono::DateTime::from_timestamp(oldest - 2 * 86_400, 0)?.format("--since=%Y-%m-%dT00:00:00").to_string();
    app.repo
        .git(&[
            "log",
            &max,
            &from,
            "--no-merges",
            "--no-show-signature",
            "--root",
            "--no-renames",
            "--name-only",
            "-z",
            "--format=%x00%ct",
            head,
            "--",
        ])
        .ok()
}

/// Active decisions that may need a refresh, the most changed first. Empty when the threshold is 0,
/// before the first commit, or when no decision has both a date and paths.
pub fn stale_decisions(app: &App, entries: &[Entry]) -> Vec<Stale> {
    let threshold = app.cfg().freshness.threshold_commits;
    if threshold == 0 {
        return Vec::new();
    }
    let decisions = watched(entries);
    if decisions.is_empty() {
        return Vec::new();
    }
    let Some(head) = app.repo.head() else { return Vec::new() };
    // the counts change with HEAD, the history read and the decisions' dates and paths
    let fingerprint: String = decisions.iter().map(|d| format!("{}|{}|{};", d.id, d.since, d.paths.join(","))).collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("freshness.json");
    let cached: Option<Cache> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|c: &Cache| c.key == key);
    let counts = match cached {
        Some(c) => c.counts,
        None => {
            let oldest = decisions.iter().map(|d| d.since).min().unwrap_or_default();
            let Some(history) = read_history(app, &head, oldest) else { return Vec::new() };
            let counts = count(&decisions, &parse_history(&history));
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
            (n >= threshold).then(|| Stale { id: e.id.clone(), title: e.title.clone(), commits: n })
        })
        .collect();
    stale.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.id.cmp(&b.id)));
    stale
}

/// `[ref] Title (34 commits), …` for the first `max`, each named as `name` refers to it.
pub fn render_list(stale: &[Stale], max: usize, name: &dyn Fn(&Stale) -> String) -> String {
    let mut s: Vec<String> = stale.iter().take(max).map(|d| format!("[{}] {} ({} commits)", name(d), d.title, d.commits)).collect();
    if stale.len() > max {
        s.push(format!("+{} more (`kontext status`)", stale.len() - max));
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

    #[test]
    fn watches_only_active_dated_decisions_with_paths() {
        let entries = vec![
            decision("a", Some("2026-01-05"), &["src/billing/**"], None),
            decision("no-paths", Some("2026-01-05"), &[], None),
            decision("blank-paths", Some("2026-01-05"), &[" "], None),
            decision("no-date", None, &["src/**"], None),
            decision("bad-date", Some("last week"), &["src/**"], None),
            decision("old", Some("2026-01-05"), &["src/**"], Some("superseded")),
            Entry { kind: "convention".into(), ..decision("conv", Some("2026-01-05"), &["src/**"], None) },
        ];
        let ids: Vec<&str> = watched(&entries).iter().map(|w| w.id).collect();
        assert_eq!(ids, vec!["a"]);
    }

    #[test]
    fn cutoff_starts_the_next_utc_day() {
        assert_eq!(cutoff("2025-01-01"), Some(1735776000));
        assert_eq!(cutoff("2025-01-01T12:00:00Z"), Some(1735776000), "a time names the same day");
        for date in ["2025-02-30", "2025-1-01", "2025-01-01x", ""] {
            assert_eq!(cutoff(date), None, "{date}");
        }
    }

    #[test]
    fn history_preserves_unusual_paths_and_empty_commits() {
        let commits = parse_history(concat!(
            "\0",
            "1735819200\0\n123\0src/tab\tname.rs\0src/line\nname.rs\0",
            "\0",
            "1735905600\0",
            "\0",
            "1735992000\0\n\nleading.rs\0",
        ));
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].time, 1735819200);
        assert_eq!(commits[0].paths, vec!["123", "src/tab\tname.rs", "src/line\nname.rs"]);
        assert!(commits[1].paths.is_empty());
        assert_eq!(commits[2].paths, vec!["\nleading.rs"]);
    }

    #[test]
    fn counts_later_commits_on_governed_paths_once_each() {
        let entries = vec![
            decision("billing", Some("2026-01-05"), &["src/billing/**"], None),
            decision("api", Some("2026-02-01"), &["src/api", "docs/api.md"], None),
        ];
        let day = |d: &str, h: u32| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap().and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp();
        let commits = vec![
            Commit { time: day("2026-03-01", 9), paths: vec!["src/billing/invoice.rs", "src/billing/tax.rs", "src/api/routes.rs"] },
            Commit { time: day("2026-02-01", 18), paths: vec!["src/api/routes.rs"] }, // the api decision's own day
            Commit { time: day("2026-01-20", 9), paths: vec!["src/billing/tax.rs"] }, // before the api decision
            Commit { time: day("2026-01-10", 9), paths: vec!["README.md"] },
            Commit { time: day("2026-01-05", 23), paths: vec!["src/billing/tax.rs"] }, // the billing decision's own day
        ];
        let counts = count(&watched(&entries), &commits);
        assert_eq!(counts.get("billing"), Some(&2), "two files of one commit count once");
        assert_eq!(counts.get("api"), Some(&1));
    }

    #[test]
    fn renders_a_bounded_list() {
        let s = |id: &str, n| Stale { id: id.into(), title: format!("T {id}"), commits: n };
        let list = vec![s("a", 30), s("b", 25), s("c", 21)];
        let name = |d: &Stale| d.id.clone();
        assert_eq!(render_list(&list, 2, &name), "[a] T a (30 commits), [b] T b (25 commits), +1 more (`kontext status`)");
        assert_eq!(render_list(&list[..1], 3, &name), "[a] T a (30 commits)");
    }
}
