//! Knowledge freshness, the one place for both signals that an entry may need a look:
//! * churn — active decisions, conventions and learnings whose governed `paths` saw many commits
//!   since someone last looked at them. A commit counts when it is not an ancestor of a look (a
//!   commit that changed the entry's own file, or the commits its newest ledger look named and
//!   landed in) and is not older than the entry's day (its `date`, or a `checked` date the ledger
//!   does not record). One `git log` of HEAD covers every entry and is shared with the verification
//!   view; the counts are cached per HEAD: a brief pays one `rev-parse` while nothing is committed;
//! * review date — active entries whose `review_by` date has passed. A review date is not an
//!   expiry: the entry stays where it is, marked, until someone confirms, supersedes or retires it.
use crate::app::App;
use crate::glob::path_matches;
use crate::store::Entry;
use crate::util;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex};

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

/// An entry under watch: commits from `since` (UTC seconds) on, on these path patterns, that no
/// look reaches — a commit that changed `file`, or one of `anchors`.
struct Watched<'a> {
    id: &'a str,
    file: &'a str,
    since: i64,
    paths: Vec<&'a str>,
    /// Commits a ledger look named or landed in.
    anchors: Vec<String>,
    /// The ledger line of that look, to find where it landed: (id, at).
    key: Option<(String, String)>,
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

/// The second a `checked` value names (a day from its start).
fn checked_seconds(checked: &str) -> Option<i64> {
    match crate::store::parse_checked(checked)? {
        crate::store::Checked::At(at) => Some(at.timestamp()),
        crate::store::Checked::Day(d) => d.and_hms_opt(0, 0, 0).map(|t| t.and_utc().timestamp()),
    }
}

/// A ledger look at an entry: its time, the commit it named, and its line's key.
pub(crate) struct LedgerLook {
    pub at: i64,
    pub commit: Option<String>,
    pub key: (String, String),
}

/// An entry needs a day to count from: its date or its confirmation. A confirmation the ledger
/// recorded counts from the commits its line named and landed in, not from its time, so a branch
/// merged after it still counts.
fn watched<'a>(entries: &'a [Entry], look: &dyn Fn(&Entry) -> Option<LedgerLook>) -> Vec<Watched<'a>> {
    entries
        .iter()
        .filter(|e| WATCHED_KINDS.contains(&e.kind.as_str()) && e.is_active())
        .filter_map(|e| {
            let paths: Vec<&str> = e.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
            if paths.is_empty() {
                return None;
            }
            let dated = e.date.as_deref().and_then(cutoff);
            let checked = e.checked.as_deref().and_then(confirmed_since);
            dated.or(checked)?;
            let ledger = look(e);
            let recorded = |c: &str| ledger.as_ref().is_some_and(|l| checked_seconds(c).is_some_and(|s| l.at >= s));
            let checked = checked.filter(|_| !e.checked.as_deref().is_some_and(recorded));
            let since = dated.into_iter().chain(checked).max().unwrap_or(i64::MIN);
            let (anchors, key) = match ledger {
                Some(l) => (l.commit.into_iter().collect(), Some(l.key)),
                None => (Vec::new(), None),
            };
            Some(Watched { id: &e.id, file: &e.rel_path, since, paths, anchors, key })
        })
        .collect()
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

// ------------------------------------------------------------------------------------ history

/// The format every history read shares: sha, commit time, parents and subject.
pub(crate) const FORMAT: &str = "--format=%x00%H%x1f%ct%x1f%P%x1f%s";

/// One commit of a history read.
#[derive(Debug, Clone, Default)]
pub(crate) struct HCommit {
    pub sha: String,
    pub time: i64,
    pub subject: String,
    /// Parents within the read (an older one cut off by the bound or a shallow graft is left out).
    pub parents: Vec<usize>,
    /// A merge: kept in the graph, never counted as a change.
    pub merge: bool,
    pub paths: Vec<String>,
}

/// HEAD's history as a graph, newest first as `git log` lists it, merges included. Counts go by
/// ancestry: a commit is "since a look" when the look's commit does not reach it.
#[derive(Debug, Default)]
pub(crate) struct History {
    pub commits: Vec<HCommit>,
    index: HashMap<String, usize>,
    /// Non-merge commits by the files they changed.
    by_path: HashMap<String, Vec<usize>>,
    /// The read stopped at `index.max_commits`: older commits are unknown.
    pub truncated: bool,
    /// A shallow clone: history stops at the graft, so counts are lower bounds.
    pub shallow: bool,
}

impl History {
    /// A history from `(sha, time, subject, parent shas, paths)`, newest first.
    pub(crate) fn build(raw: Vec<(String, i64, String, Vec<String>, Vec<String>)>) -> History {
        let index: HashMap<String, usize> = raw.iter().enumerate().map(|(i, c)| (c.0.clone(), i)).collect();
        let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
        let commits = raw
            .into_iter()
            .enumerate()
            .map(|(i, (sha, time, subject, parents, paths))| {
                let merge = parents.len() > 1;
                if !merge {
                    for p in &paths {
                        by_path.entry(p.clone()).or_default().push(i);
                    }
                }
                let parents = parents.iter().filter_map(|p| index.get(p).copied()).collect();
                HCommit { sha, time, subject, parents, merge, paths }
            })
            .collect();
        History { commits, index, by_path, truncated: false, shallow: false }
    }

    /// `git log -z --name-only` output in [`FORMAT`].
    pub(crate) fn parse(text: &str) -> History {
        let raw = parse_log(text)
            .into_iter()
            .filter_map(|(header, paths)| {
                let mut p = header.splitn(4, '\x1f');
                let sha = p.next()?.trim().to_string();
                let time = p.next()?.trim().parse().ok()?;
                let parents = p.next().unwrap_or("").split_whitespace().map(String::from).collect();
                let subject = p.next().unwrap_or("").to_string();
                Some((sha, time, subject, parents, paths.into_iter().map(String::from).collect()))
            })
            .collect();
        History::build(raw)
    }

    pub(crate) fn position(&self, sha: &str) -> Option<usize> {
        self.index.get(sha).copied()
    }

    /// Non-merge commits that changed `file`.
    pub(crate) fn touching(&self, file: &str) -> &[usize] {
        self.by_path.get(file).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether each commit is reachable from any of `sources` (themselves included).
    fn reach(&self, sources: &[usize]) -> Vec<bool> {
        let mut seen = vec![false; self.commits.len()];
        let mut stack: Vec<usize> = sources.to_vec();
        while let Some(i) = stack.pop() {
            if i >= seen.len() || seen[i] {
                continue;
            }
            seen[i] = true;
            stack.extend(self.commits[i].parents.iter().copied());
        }
        seen
    }

    /// Non-merge commits no look reaches (`looked`, none: every one), from `after` (UTC seconds)
    /// on, that changed a file `governed` selects: how many, which files, and the newest `recent`.
    pub(crate) fn count(&self, looked: Option<&[bool]>, after: i64, governed: &dyn Fn(&str) -> bool, recent: usize) -> Counted {
        let mut out = Counted::default();
        for (i, c) in self.commits.iter().enumerate() {
            if c.merge || looked.is_some_and(|l| l.get(i).copied().unwrap_or(false)) || c.time < after {
                continue;
            }
            let mut hit = false;
            for f in c.paths.iter().filter(|f| governed(f)) {
                hit = true;
                out.paths.insert(f.clone());
            }
            if hit {
                out.commits += 1;
                if out.recent.len() < recent {
                    out.recent.push((c.sha.clone(), c.subject.clone()));
                }
            }
        }
        out
    }
}

/// What [`History::count`] found.
#[derive(Debug, Default)]
pub(crate) struct Counted {
    pub commits: usize,
    pub paths: BTreeSet<String>,
    pub recent: Vec<(String, String)>,
}

/// Ancestor sets of a history, each distinct set of looks computed once.
pub(crate) struct Reach<'h> {
    history: &'h History,
    memo: HashMap<Vec<usize>, Rc<Vec<bool>>>,
}

impl<'h> Reach<'h> {
    pub(crate) fn new(history: &'h History) -> Reach<'h> {
        Reach { history, memo: HashMap::new() }
    }

    /// What `sources` reach, or None without any.
    pub(crate) fn of(&mut self, mut sources: Vec<usize>) -> Option<Rc<Vec<bool>>> {
        sources.sort_unstable();
        sources.dedup();
        if sources.is_empty() {
            return None;
        }
        if let Some(r) = self.memo.get(&sources) {
            return Some(r.clone());
        }
        let r = Rc::new(self.history.reach(&sources));
        self.memo.insert(sources, r.clone());
        Some(r)
    }
}

static HISTORIES: LazyLock<Mutex<HashMap<String, Arc<History>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether this clone is shallow (its history stops at a graft).
pub(crate) fn is_shallow(app: &App) -> bool {
    app.repo.common_dir.join("shallow").is_file()
}

/// HEAD's history, read once per HEAD and process: up to `index.max_commits` commits, merges
/// included, a rename as both of its ends (moving code out of governed paths changes them too), and
/// no signature output, which would land among the paths.
pub(crate) fn history(app: &App, head: &str) -> Option<Arc<History>> {
    let max = app.cfg().index.max_commits.max(1);
    let key = format!("{}\0{head}\0{max}", app.repo.root.display());
    if let Some(h) = HISTORIES.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return Some(h);
    }
    let text = app
        .repo
        .git(&[
            "log",
            &format!("--max-count={max}"),
            "--no-show-signature",
            "--root",
            "--no-renames",
            "--name-only",
            "-z",
            FORMAT,
            head,
            "--",
        ])
        .ok()?;
    let mut h = History::parse(&text);
    h.truncated = h.commits.len() >= max;
    h.shallow = is_shallow(app);
    let h = Arc::new(h);
    if let Ok(mut m) = HISTORIES.lock() {
        if m.len() >= 8 {
            m.clear();
        }
        m.insert(key, h.clone());
    }
    Some(h)
}

/// Matching commits per entry; a commit counts once, however many of its files match, and only
/// when no look at the entry reaches it: a commit that changed its file (an edit, a confirmation),
/// or the commits its newest ledger look named and landed in.
fn count(watched: &[Watched], history: &History) -> BTreeMap<String, usize> {
    let mut reach = Reach::new(history);
    watched
        .iter()
        .map(|w| {
            let mut sources: Vec<usize> = history.touching(w.file).to_vec();
            sources.extend(w.anchors.iter().filter_map(|s| history.position(s)));
            let looked = reach.of(sources);
            let governed = |f: &str| w.paths.iter().any(|p| path_matches(p, f));
            let n = history.count(looked.as_deref().map(Vec::as_slice), w.since, &governed, 0).commits;
            (w.id.to_string(), n)
        })
        .collect()
}

/// Active entries that may need a refresh, the most changed first. Empty when the threshold is 0,
/// before the first commit, or when no watched entry has both a day to count from and paths.
pub fn stale_entries(app: &App, entries: &[Entry]) -> Vec<Stale> {
    let threshold = app.cfg().freshness.threshold_commits;
    if threshold == 0 {
        return Vec::new();
    }
    let lines = crate::ledger::read(app);
    let looks = crate::ledger::looks(&lines);
    let look = |e: &Entry| {
        let l = looks.get(e.id.as_str())?.look?;
        let at = checked_seconds(&l.at)?;
        Some(LedgerLook { at, commit: l.commit.clone(), key: (l.id.clone(), l.at.clone()) })
    };
    let mut watched = watched(entries, &look);
    if watched.is_empty() {
        return Vec::new();
    }
    let Some(head) = app.repo.head() else { return Vec::new() };
    // the counts change with HEAD, the history read and the entries' days, files, paths and looks
    let fingerprint: String = watched
        .iter()
        .map(|w| {
            let key = w.key.as_ref().map(|k| k.1.as_str()).unwrap_or("-");
            format!("{}|{}|{}|{}|{}|{key};", w.id, w.file, w.since, w.paths.join(","), w.anchors.join(","))
        })
        .collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("freshness.json");
    let cached: Option<Cache> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|c: &Cache| c.key == key);
    let counts = match cached {
        Some(c) => c.counts,
        None => {
            let Some(history) = history(app, &head) else { return Vec::new() };
            if watched.iter().any(|w| w.key.is_some()) {
                let landings = crate::verification::landings(app, &head);
                for w in watched.iter_mut() {
                    if let Some(sha) = w.key.as_ref().and_then(|k| landings.get(k)) {
                        w.anchors.push(sha.clone());
                    }
                }
            }
            let counts = count(&watched, &history);
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

    fn no_ledger(_: &Entry) -> Option<LedgerLook> {
        None
    }

    /// A linear history, newest first: `c0` is HEAD, each commit's parent the next.
    fn linear(commits: &[(i64, &[&str])]) -> History {
        let n = commits.len();
        History::build(
            commits
                .iter()
                .enumerate()
                .map(|(i, (time, paths))| {
                    let parents = if i + 1 < n { vec![format!("c{}", i + 1)] } else { Vec::new() };
                    (format!("c{i}"), *time, format!("s{i}"), parents, paths.iter().map(|p| p.to_string()).collect())
                })
                .collect(),
        )
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
        let ids: Vec<&str> = watched(&entries, &no_ledger).iter().map(|w| w.id).collect();
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
        let history = linear(&[
            (day("2026-03-01", 9), &["src/a.rs"]),
            (day("2026-02-20", 9), &["src/b.rs"]),
            (day("2026-02-10", 9), &["src/c.rs"]),
            (day("2026-02-10", 9), &[".ai/decisions/edited.md", "src/d.rs"]),
            (day("2026-01-20", 9), &["src/e.rs"]),
        ]);
        let counts = count(&watched(&entries, &no_ledger), &history);
        assert_eq!(counts["plain"], 5);
        assert_eq!(counts["confirmed"], 4, "counted from the day after the confirmation");
        assert_eq!(counts["stale-check"], 5, "a confirmation older than the date does not move it back");
        assert_eq!(counts["confirmed-day"], 2, "a day counts from the start of the next one");
        assert_eq!(counts["confirmed-at"], 4, "a time counts after its second, the rest of its day included");
        assert_eq!(counts["edited"], 3, "the commit that edited the entry and its ancestors do not count");
    }

    #[test]
    fn a_branch_merged_after_an_edit_or_a_recorded_confirmation_still_counts() {
        // main: seed ← edit (E, touches the entry) ← merge M; feature: seed ← F (older than E)
        let day = |d: &str| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap().and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp();
        let raw = vec![
            ("m".to_string(), day("2026-10-07"), "merge".to_string(), vec!["e".to_string(), "f".to_string()], Vec::new()),
            ("e".to_string(), day("2026-10-06"), "edit".to_string(), vec!["s".to_string()], vec![".ai/decisions/rule.md".to_string()]),
            ("f".to_string(), day("2026-10-05"), "feature".to_string(), vec!["s".to_string()], vec!["src/a.rs".to_string()]),
            (
                "s".to_string(),
                day("2026-10-01"),
                "seed".to_string(),
                Vec::new(),
                vec![".ai/decisions/rule.md".to_string(), "src/a.rs".to_string()],
            ),
        ];
        let history = History::build(raw);
        assert!(history.commits[0].merge && history.touching(".ai/decisions/rule.md") == [1, 3]);
        let entry = Entry { rel_path: ".ai/decisions/rule.md".into(), ..decision("rule", Some("2026-10-01"), &["src/**"], None) };
        let counts = count(&watched(std::slice::from_ref(&entry), &no_ledger), &history);
        assert_eq!(counts["rule"], 1, "F was not seen by the edit, the merge itself never counts");

        // a confirmation recorded in the ledger at E counts from its commits, not from its time
        let confirmed = Entry { checked: Some("2026-10-06T12:00:00Z".into()), ..entry.clone() };
        let look = |_: &Entry| Some(LedgerLook { at: day("2026-10-06"), commit: Some("e".into()), key: ("rule".into(), "t".into()) });
        let w = watched(std::slice::from_ref(&confirmed), &look);
        assert_eq!(w[0].anchors, vec!["e".to_string()]);
        assert_eq!(count(&w, &history)["rule"], 1);
        // a `checked` with no ledger line is a date: the older branch commit falls before it
        assert_eq!(count(&watched(std::slice::from_ref(&confirmed), &no_ledger), &history)["rule"], 0);
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
    fn history_preserves_unusual_paths_merges_and_empty_commits() {
        let h = History::parse(concat!(
            "\0",
            "aaa\x1f1735819200\x1fbbb\x1fsubject a\0\n123\0src/tab\tname.rs\0src/line\nname.rs\0",
            "\0",
            "bbb\x1f1735905600\x1fccc ddd\x1fmerge b\0",
            "\0",
            "ccc\x1f1735992000\x1f\x1froot c\0\n\nleading.rs\0",
        ));
        assert_eq!(h.commits.len(), 3);
        assert_eq!((h.commits[0].sha.as_str(), h.commits[0].time, h.commits[0].subject.as_str()), ("aaa", 1735819200, "subject a"));
        assert_eq!(h.commits[0].paths, vec!["123", "src/tab\tname.rs", "src/line\nname.rs"]);
        assert_eq!(h.commits[0].parents, vec![1]);
        assert!(h.commits[1].merge && h.commits[1].paths.is_empty());
        assert_eq!(h.commits[1].parents, vec![2], "a parent outside the read is left out");
        assert_eq!(h.commits[2].paths, vec!["\nleading.rs"]);
        assert_eq!(h.touching("123"), &[0]);
    }

    #[test]
    fn counts_later_commits_on_governed_paths_once_each() {
        let entries = vec![
            decision("billing", Some("2026-01-05"), &["src/billing/**"], None),
            decision("api", Some("2026-02-01"), &["src/api", "docs/api.md"], None),
        ];
        let day = |d: &str, h: u32| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap().and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp();
        let history = linear(&[
            (day("2026-03-01", 9), &["src/billing/invoice.rs", "src/billing/tax.rs", "src/api/routes.rs"]),
            (day("2026-02-01", 18), &["src/api/routes.rs"]), // the api decision's own day
            (day("2026-01-20", 9), &["src/billing/tax.rs"]), // before the api decision
            (day("2026-01-10", 9), &["README.md"]),
            (day("2026-01-05", 23), &["src/billing/tax.rs"]), // the billing decision's own day
        ]);
        let counts = count(&watched(&entries, &no_ledger), &history);
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
