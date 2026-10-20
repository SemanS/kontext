//! Knowledge freshness, the one place for both signals that an entry may need a look:
//! * churn — active decisions, conventions and learnings whose governed `paths` saw many commits
//!   since someone last looked at them: the entry's `date`, its `checked` confirmation or the last
//!   commit that changed its own file, whichever is latest. One `git log` covers every entry, and the
//!   counts are cached per HEAD: a brief pays one `rev-parse` while nothing is committed;
//! * review date — active entries whose `review_by` date has passed. A review date is not an
//!   expiry: the entry stays where it is, marked, until someone confirms, supersedes or retires it.
use crate::app::App;
use crate::glob::path_matches;
use crate::store::Entry;
use crate::util;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The kinds churn watches. Incidents record what happened and stay true; module maps are
/// refreshed by init.
const WATCHED_KINDS: &[&str] = &["decision", "convention", "learning"];

/// An entry whose paths changed in at least `freshness.threshold_commits` commits since it was last
/// looked at.
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

/// An entry under watch: from the first second (UTC) after its day (or its confirmation's), these
/// path patterns — and only commits newer than the last one that changed `file`.
struct Watched<'a> {
    id: &'a str,
    file: &'a str,
    since: i64,
    paths: Vec<&'a str>,
}

/// The start of the UTC day after an entry's date (or confirmation). The day itself is left out: that
/// is usually the change it was recorded with, or the one that prompted the look. `2025-01-01` and `2025-01-01T12:00:00Z` name the same day.
fn cutoff(date: &str) -> Option<i64> {
    let day = date.get(..10)?;
    if !matches!(date.as_bytes().get(10), None | Some(b'T' | b' ')) {
        return None;
    }
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?.succ_opt()?.and_hms_opt(0, 0, 0).map(|d| d.and_utc().timestamp())
}

/// Where a confirmation starts the count: a time after its second (the commits of that very second
/// were what the confirmation looked at), a day (like a date) from the start of the next UTC day.
fn confirmed_since(checked: &str) -> Option<i64> {
    match crate::store::parse_checked(checked)? {
        crate::store::Checked::At(at) => Some(at.timestamp() + 1),
        crate::store::Checked::Day(_) => cutoff(checked),
    }
}

/// An entry needs a day to count from: its date or its confirmation. An edit of its file only
/// moves that day later, so an undated entry is never flagged for the age of its file.
fn watched(entries: &[Entry]) -> Vec<Watched<'_>> {
    entries
        .iter()
        .filter(|e| WATCHED_KINDS.contains(&e.kind.as_str()) && e.is_active())
        .filter_map(|e| {
            let paths: Vec<&str> = e.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
            if paths.is_empty() {
                return None;
            }
            let confirmed = e.checked.as_deref().and_then(confirmed_since);
            let since = e.date.as_deref().and_then(cutoff).into_iter().chain(confirmed).max()?;
            Some(Watched { id: &e.id, file: &e.rel_path, since, paths })
        })
        .collect()
}

struct Commit<'a> {
    time: i64,
    paths: Vec<&'a str>,
}

/// `git log -z --name-only --format=%x00<header>`: a NUL-prefixed header starts a commit, and its
/// paths follow NUL-terminated (tabs and newlines included), the first one after a newline git inserts.
pub(crate) fn parse_log(history: &str) -> Vec<(&str, Vec<&str>)> {
    let mut commits: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut header = true;
    let mut first_path = false;
    for field in history.split('\0') {
        if field.is_empty() {
            header = true;
        } else if header {
            commits.push((field, Vec::new()));
            first_path = true;
            header = false;
        } else if let Some((_, paths)) = commits.last_mut() {
            let path = if first_path { field.strip_prefix('\n').unwrap_or(field) } else { field };
            first_path = false;
            if !path.is_empty() {
                paths.push(path);
            }
        }
    }
    commits
}

/// The history read with `--format=%x00%ct`: commit times and paths.
fn parse_history(history: &str) -> Vec<Commit<'_>> {
    parse_log(history).into_iter().filter_map(|(header, paths)| Some(Commit { time: header.parse().ok()?, paths })).collect()
}

/// Matching commits per entry; a commit counts once, however many of its files match. `commits` are
/// newest first, as `git log` lists them: a commit that changed the entry's own file (an edit, a
/// confirmation) was a look at it, and only the commits listed before it count. Order rather than
/// time decides, since commits made within one second share a timestamp.
fn count(watched: &[Watched], commits: &[Commit]) -> BTreeMap<String, usize> {
    watched
        .iter()
        .map(|w| {
            let looked = commits.iter().position(|c| c.paths.contains(&w.file)).unwrap_or(commits.len());
            let n = commits[..looked]
                .iter()
                .filter(|c| c.time >= w.since && c.paths.iter().any(|f| w.paths.iter().any(|p| path_matches(p, f))))
                .count();
            (w.id.to_string(), n)
        })
        .collect()
}

/// One history read for every entry: non-merge commits of HEAD from a day before the oldest
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

/// Active entries that may need a refresh, the most changed first. Empty when the threshold is 0,
/// before the first commit, or when no watched entry has both a day to count from and paths.
pub fn stale_entries(app: &App, entries: &[Entry]) -> Vec<Stale> {
    let threshold = app.cfg().freshness.threshold_commits;
    if threshold == 0 {
        return Vec::new();
    }
    let watched = watched(entries);
    if watched.is_empty() {
        return Vec::new();
    }
    let Some(head) = app.repo.head() else { return Vec::new() };
    // the counts change with HEAD, the history read and the entries' days, files and paths
    let fingerprint: String = watched.iter().map(|w| format!("{}|{}|{}|{};", w.id, w.file, w.since, w.paths.join(","))).collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("freshness.json");
    let cached: Option<Cache> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|c: &Cache| c.key == key);
    let counts = match cached {
        Some(c) => c.counts,
        None => {
            let oldest = watched.iter().map(|w| w.since).min().unwrap_or_default();
            let Some(history) = read_history(app, &head, oldest) else { return Vec::new() };
            let counts = count(&watched, &parse_history(&history));
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

/// An active entry (any kind) whose `review_by` date has passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub id: String,
    pub title: String,
    pub review_by: String,
}

/// Active entries past their review date (`today` is `YYYY-MM-DD`), the longest overdue first.
pub fn review_due(entries: &[Entry], today: &str) -> Vec<Due> {
    let mut due: Vec<Due> = entries
        .iter()
        .filter(|e| e.review_due(today))
        .map(|e| Due { id: e.id.clone(), title: e.title.clone(), review_by: e.review_by.clone().unwrap_or_default() })
        .collect();
    due.sort_by(|a, b| a.review_by.cmp(&b.review_by).then(a.id.cmp(&b.id)));
    due
}

/// Both signals for one set of entries.
pub struct Freshness {
    pub stale: Vec<Stale>,
    pub due: Vec<Due>,
}

pub fn check(app: &App, entries: &[Entry]) -> Freshness {
    Freshness { stale: stale_entries(app, entries), due: review_due(entries, &util::today()) }
}

/// `[ref] Title (review by 2026-01-31), …` for the first `max`.
pub fn render_due(due: &[Due], max: usize, name: &dyn Fn(&Due) -> String) -> String {
    let mut s: Vec<String> = due.iter().take(max).map(|d| format!("[{}] {} (review by {})", name(d), d.title, d.review_by)).collect();
    if due.len() > max {
        s.push(format!("+{} more (`kontext status`)", due.len() - max));
    }
    s.join(", ")
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
    fn watches_active_decisions_conventions_and_learnings_with_a_day_and_paths() {
        let of = |kind: &str, id: &str| Entry { kind: kind.into(), ..decision(id, Some("2026-01-05"), &["src/**"], None) };
        let entries = vec![
            decision("a", Some("2026-01-05"), &["src/billing/**"], None),
            decision("no-paths", Some("2026-01-05"), &[], None),
            decision("blank-paths", Some("2026-01-05"), &[" "], None),
            decision("no-date", None, &["src/**"], None),
            decision("bad-date", Some("last week"), &["src/**"], None),
            decision("old", Some("2026-01-05"), &["src/**"], Some("superseded")),
            Entry { checked: Some("2026-02-01".into()), ..decision("confirmed-only", None, &["src/**"], None) },
            of("convention", "conv"),
            of("learning", "learn"),
            of("incident", "inc"),
            of("architecture", "map"),
        ];
        let ids: Vec<&str> = watched(&entries).iter().map(|w| w.id).collect();
        assert_eq!(ids, vec!["a", "confirmed-only", "conv", "learn"]);
    }

    #[test]
    fn a_confirmation_or_an_edit_of_the_entry_resets_the_count() {
        let day = |d: &str, h: u32| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap().and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp();
        let rule = |id: &str, checked: Option<&str>| Entry {
            checked: checked.map(str::to_string),
            rel_path: format!(".ai/decisions/{id}.md"),
            ..decision(id, Some("2026-01-05"), &["src/**"], None)
        };
        let entries = vec![
            rule("plain", None),
            rule("confirmed", Some("2026-02-01")),
            rule("stale-check", Some("2025-12-01")),
            rule("edited", None),
            rule("confirmed-day", Some("2026-02-10")),
            rule("confirmed-at", Some("2026-02-10T08:00:00Z")),
        ];
        let commits = vec![
            Commit { time: day("2026-03-01", 9), paths: vec!["src/a.rs"] },
            Commit { time: day("2026-02-20", 9), paths: vec!["src/b.rs"] },
            // the same second as the next one: the order in the log decides, not the time
            Commit { time: day("2026-02-10", 9), paths: vec!["src/c.rs"] },
            Commit { time: day("2026-02-10", 9), paths: vec![".ai/decisions/edited.md", "src/d.rs"] },
            Commit { time: day("2026-01-20", 9), paths: vec!["src/e.rs"] },
        ];
        let counts = count(&watched(&entries), &commits);
        assert_eq!(counts["plain"], 5);
        assert_eq!(counts["confirmed"], 4, "counted from the day after the confirmation");
        assert_eq!(counts["stale-check"], 5, "a confirmation older than the date does not move it back");
        assert_eq!(counts["confirmed-day"], 2, "a day counts from the start of the next one");
        assert_eq!(counts["confirmed-at"], 4, "a time counts after its second, the rest of its day included");
        assert_eq!(counts["edited"], 3, "the commit that edited the entry and the older ones do not count");
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
    fn review_dates_mark_active_entries_of_any_kind() {
        let mut e = decision("freeze", Some("2026-01-05"), &[], None);
        e.review_by = Some("2026-01-31".into());
        let mut conv = Entry { kind: "convention".into(), ..decision("conv", None, &[], None) };
        conv.review_by = Some("2026-03-01".into());
        let mut gone = decision("gone", None, &[], Some("deprecated"));
        gone.review_by = Some("2026-01-01".into());
        let later = Entry { review_by: Some("2027-01-01".into()), ..decision("later", None, &[], None) };
        let entries = vec![conv, e, gone, later, decision("none", None, &[], None)];
        let due = review_due(&entries, "2026-04-01");
        assert_eq!(due.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), vec!["freeze", "conv"]);
        assert!(review_due(&entries, "2026-01-31").is_empty(), "due only after the day itself");
        assert_eq!(render_due(&due, 1, &|d| d.id.clone()), "[freeze] freeze (review by 2026-01-31), +1 more (`kontext status`)");
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
