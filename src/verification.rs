//! What a reader needs to trust an entry: how it was last verified (the newest of its ledger lines
//! and the `checked:` cache in its file), what changed on its governed paths since that look
//! (counted from the commit the look named, not from a date), the premises it links to that no
//! longer hold, and running its `verify` command.
//!
//! One `git log` (and one `git log -p` of the ledger) covers every entry, cached per HEAD with a
//! fingerprint of the looks, so listing entries costs a `rev-parse` while nothing is committed.
use crate::app::App;
use crate::glob::path_matches;
use crate::ledger::{self, Line};
use crate::store::{Checked, Entry, Store, parse_checked};
use crate::util;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::LazyLock;

/// Statuses that make a linked entry a changed premise.
const GONE: &[&str] = &["superseded", "deprecated", "retired"];
/// At most this many changed governed files per entry.
const MAX_PATHS: usize = 20;
/// At most this many commits named per entry.
const MAX_RECENT: usize = 5;

/// The second a ledger time or a `checked` value names: a day counts from its start, so a ledger
/// line of the same day is the newer one.
fn seconds(at: &str) -> Option<i64> {
    match parse_checked(at)? {
        Checked::At(t) => Some(t.timestamp()),
        Checked::Day(d) => d.and_hms_opt(0, 0, 0).map(|t| t.and_utc().timestamp()),
    }
}

/// An entry's last verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub at: String,
    pub kind: String,
    pub commit: Option<String>,
    pub by: Option<String>,
    pub because: Option<String>,
    pub passed: Option<bool>,
    /// `ledger` or `frontmatter` (a `checked:` date with no ledger line behind it).
    pub source: &'static str,
    /// The commit in HEAD's history that landed this verification: for a ledger line, the commit
    /// that first added it; for a `checked:` date, the last commit that changed the entry's file.
    /// None while it is only staged or in the working tree (and when nobody asked: [`of`] leaves it
    /// to [`Context::load_landed`]).
    pub landed: Option<String>,
}

impl Verification {
    /// Only a person's read or a passed command counts as verified; agent and bulk confirmations
    /// are stamps.
    pub fn verified(&self) -> bool {
        self.kind == "read" || (self.kind == "verify" && self.passed == Some(true))
    }

    pub fn to_json(&self) -> Value {
        json!({"at":self.at,"kind":self.kind,"commit":self.commit,"by":self.by,"because":self.because,
            "passed":self.passed,"source":self.source,"verified":self.verified(),"landed":self.landed})
    }
}

/// The newer of the entry's newest ledger line and its frontmatter `checked`; the ledger line wins
/// a tie, since it says more.
pub fn of(entry: &Entry, newest: Option<&Line>) -> Option<Verification> {
    let fm = entry.checked.as_deref().filter(|c| seconds(c).is_some());
    let from_line = |l: &Line| Verification {
        at: l.at.clone(),
        kind: l.kind.clone(),
        commit: l.commit.clone(),
        by: l.by.clone(),
        because: Some(l.because.clone()).filter(|b| !b.is_empty()),
        passed: l.passed,
        source: "ledger",
        landed: None,
    };
    let from_fm = |c: &str| Verification {
        at: c.to_string(),
        kind: "read".into(),
        commit: None,
        by: None,
        because: None,
        passed: None,
        source: "frontmatter",
        landed: None,
    };
    match (newest, fm) {
        (Some(l), Some(c)) if seconds(c) > seconds(&l.at) => Some(from_fm(c)),
        (Some(l), _) => Some(from_line(l)),
        (None, Some(c)) => Some(from_fm(c)),
        (None, None) => None,
    }
}

/// The newest confirmation line when the `checked:` cache in the entry's file is behind it (a merge
/// took the older side, or the line came from another branch): what `confirm --repair` writes.
pub fn behind<'l>(entry: &Entry, newest_confirmation: Option<&'l Line>) -> Option<&'l Line> {
    let line = newest_confirmation?;
    let behind = match entry.checked.as_deref().and_then(parse_checked) {
        None => true,
        Some(Checked::At(t)) => t.timestamp() < seconds(&line.at)?,
        // a day the line falls on is not behind it
        Some(Checked::Day(d)) => match parse_checked(&line.at)? {
            Checked::At(t) => t.date_naive() > d,
            Checked::Day(day) => day > d,
        },
    };
    behind.then_some(line)
}

/// `checked` as the API reports it: the newer of the cache and the newest confirmation.
pub fn checked(entry: &Entry, newest_confirmation: Option<&Line>) -> Option<String> {
    match (entry.checked.as_deref(), newest_confirmation) {
        (Some(c), Some(l)) if seconds(c) > seconds(&l.at) => Some(c.to_string()),
        (_, Some(l)) => Some(l.at.clone()),
        (c, None) => c.map(str::to_string),
    }
}

/// Commits on an entry's governed paths after its verification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Since {
    pub commits: usize,
    pub head: String,
    /// The changed governed files, sorted, at most [`MAX_PATHS`].
    pub paths: Vec<String>,
    /// The newest commits among them: (full sha, subject), at most [`MAX_RECENT`].
    pub recent: Vec<(String, String)>,
}

impl Since {
    pub fn to_json(&self) -> Value {
        json!({"commits":self.commits,"head":self.head,"paths":self.paths,
            "recent":self.recent.iter().map(|(sha,subject)| json!({"sha":sha,"subject":subject})).collect::<Vec<_>>()})
    }
}

/// An active entry links to one that no longer holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Premise {
    pub id: String,
    pub status: String,
    pub superseded_by: Vec<String>,
}

impl Premise {
    pub fn to_json(&self) -> Value {
        json!({"id":self.id,"status":self.status,"superseded_by":self.superseded_by})
    }
}

static WIKI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\[\]|#\n]+)(?:[|#][^\[\]\n]*)?\]\]").expect("wiki link"));

fn names(id: &str, list: &[String]) -> bool {
    list.iter().any(|s| id == s || id.starts_with(&format!("{s}-")))
}

/// The entries `entry` links to — `[[id]]` in its body, `supersedes`, an `extends` field — that are
/// superseded, retired or deprecated. The one it replaced itself is not a changed premise: that is
/// the point of it. A flag, never a change of either entry.
pub fn premises(entry: &Entry, all: &[Entry]) -> Vec<Premise> {
    if !entry.is_active() {
        return Vec::new();
    }
    let extends = entry.extra.iter().filter(|(k, _)| k == "extends").flat_map(|(_, v)| v.as_list());
    let links: Vec<String> = WIKI
        .captures_iter(&entry.body)
        .map(|c| c[1].trim().trim_start_matches("kx:").to_string())
        .chain(entry.supersedes.iter().cloned())
        .chain(extends)
        .collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for link in links {
        let Some(target) = Store::find(all, &link) else { continue };
        if target.id == entry.id || target.rel_path == entry.rel_path || !seen.insert(target.id.clone()) {
            continue;
        }
        let status = target.status.clone().unwrap_or_default();
        if !GONE.contains(&status.as_str()) {
            continue;
        }
        let replaced_by_this = status == "superseded"
            && names(&target.id, &entry.supersedes)
            && (target.superseded_by.is_empty() || names(&entry.id, &target.superseded_by));
        if replaced_by_this {
            continue;
        }
        out.push(Premise { id: target.id.clone(), status, superseded_by: target.superseded_by.clone() });
    }
    out
}

/// Where the count of an entry starts.
#[derive(Debug, Clone)]
enum Start {
    /// A ledger line: after the commit it named, or after the commit that landed the line,
    /// whichever is newer in HEAD's history; by time when neither is in it.
    Ledger { at: i64, commit: Option<String>, key: (String, String) },
    /// A `checked:` date only: as freshness counts — after its second (a day: from the next one),
    /// and after the last commit that changed the entry's own file.
    Checked { since: i64 },
}

struct Look<'a> {
    id: &'a str,
    file: &'a str,
    paths: Vec<&'a str>,
    from: Start,
}

impl Look<'_> {
    fn fingerprint(&self) -> String {
        let from = match &self.from {
            Start::Ledger { at, commit, .. } => format!("l{at}{}", commit.as_deref().unwrap_or("-")),
            Start::Checked { since } => format!("c{since}"),
        };
        format!("{}|{}|{}|{from};", self.id, self.file, self.paths.join(","))
    }
    fn time(&self) -> i64 {
        match &self.from {
            Start::Ledger { at, .. } => *at,
            Start::Checked { since } => *since,
        }
    }
}

#[derive(Debug, Clone)]
struct Commit {
    sha: String,
    time: i64,
    subject: String,
    paths: Vec<String>,
}

/// `git log --format=%x00%H %ct %s` output.
fn parse_commits(history: &str) -> Vec<Commit> {
    crate::freshness::parse_log(history)
        .into_iter()
        .filter_map(|(header, paths)| {
            let mut parts = header.splitn(3, ' ');
            let sha = parts.next()?.to_string();
            let time = parts.next()?.parse().ok()?;
            let subject = parts.next().unwrap_or("").to_string();
            Some(Commit { sha, time, subject, paths: paths.into_iter().map(String::from).collect() })
        })
        .collect()
}

/// `git log -p --format=%x00%H -- <ledger>`: for each line, the commit that first added it.
fn parse_landings(log: &str) -> HashMap<(String, String), String> {
    let mut out = HashMap::new();
    // newest first: an older commit that added the same line (a cherry-pick) overwrites
    for chunk in log.split('\0').filter(|c| !c.trim().is_empty()) {
        let mut lines = chunk.lines();
        let Some(sha) = lines.next().map(str::trim).filter(|s| !s.is_empty()) else { continue };
        for l in lines {
            if l.starts_with("+++") {
                continue;
            }
            if let Some(line) = l.strip_prefix('+').and_then(Line::parse) {
                out.insert((line.id, line.at), sha.to_string());
            }
        }
    }
    out
}

/// The count for every look over `commits` (newest first, as git lists them).
fn count(
    looks: &[Look],
    commits: &[Commit],
    landings: &HashMap<(String, String), String>,
    head: &str,
    is_store: &dyn Fn(&str) -> bool,
) -> BTreeMap<String, Since> {
    let position = |sha: &str| commits.iter().position(|c| c.sha == sha);
    looks
        .iter()
        .map(|look| {
            let (end, after) = match &look.from {
                Start::Ledger { at, commit, key } => {
                    let named = commit.as_deref().and_then(position);
                    let landed = landings.get(key).and_then(|s| position(s));
                    match named.into_iter().chain(landed).min() {
                        Some(end) => (end, i64::MIN),
                        None => (commits.len(), *at + 1),
                    }
                }
                Start::Checked { since } => {
                    (commits.iter().position(|c| c.paths.iter().any(|p| p == look.file)).unwrap_or(commits.len()), *since)
                }
            };
            let governed = |f: &str| !is_store(f) && look.paths.iter().any(|p| path_matches(p, f));
            let mut since = Since { head: head.to_string(), ..Default::default() };
            let mut changed = BTreeSet::new();
            for c in commits[..end].iter().filter(|c| c.time >= after) {
                let mut hit = false;
                for f in c.paths.iter().filter(|f| governed(f)) {
                    hit = true;
                    changed.insert(f.clone());
                }
                if hit {
                    since.commits += 1;
                    if since.recent.len() < MAX_RECENT {
                        since.recent.push((c.sha.clone(), c.subject.clone()));
                    }
                }
            }
            since.paths = changed.into_iter().take(MAX_PATHS).collect();
            (look.id.to_string(), since)
        })
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    key: String,
    since: BTreeMap<String, Since>,
}

/// The ledger and what changed since each entry's verification, read once for a set of entries.
pub struct Context {
    lines: Vec<Line>,
    since: BTreeMap<String, Since>,
    /// By entry file: the commit that landed its verification (only with [`Context::load_landed`]).
    landed: BTreeMap<String, String>,
}

impl Context {
    /// Verifications of `entries` and, for the active ones with paths that `want` selects, the
    /// commits since.
    pub fn load(app: &App, entries: &[Entry], want: &dyn Fn(&Entry) -> bool) -> Context {
        let lines = ledger::read(app);
        let head = app.repo.head();
        let since = match &head {
            Some(head) => since_all(app, entries, &lines, head, want),
            None => BTreeMap::new(),
        };
        Context { lines, since, landed: BTreeMap::new() }
    }

    /// [`Context::load`], and where each selected entry's verification landed (what the machine
    /// API reports; the brief does not need it).
    pub fn load_landed(app: &App, entries: &[Entry], want: &dyn Fn(&Entry) -> bool) -> Context {
        let mut ctx = Context::load(app, entries, want);
        if let Some(head) = app.repo.head() {
            ctx.landed = landed_all(app, entries, &ctx.lines, &head, want);
        }
        ctx
    }

    pub fn newest(&self) -> HashMap<&str, &Line> {
        ledger::latest(&self.lines, false)
    }

    pub fn verification(&self, entry: &Entry) -> Option<Verification> {
        self.with_landed(entry, of(entry, self.newest().get(entry.id.as_str()).copied()))
    }

    fn with_landed(&self, entry: &Entry, verification: Option<Verification>) -> Option<Verification> {
        verification.map(|v| Verification { landed: self.landed.get(&entry.rel_path).cloned(), ..v })
    }

    pub fn since(&self, entry: &Entry) -> Option<&Since> {
        self.since.get(&entry.id)
    }

    /// The additive fields of the machine API for one entry's JSON.
    pub fn annotate(
        &self,
        entry: &Entry,
        all: &[Entry],
        newest: &HashMap<&str, &Line>,
        confirmations: &HashMap<&str, &Line>,
        value: &mut Value,
    ) {
        let verification = self.with_landed(entry, of(entry, newest.get(entry.id.as_str()).copied()));
        value["verification"] = verification.as_ref().map(Verification::to_json).unwrap_or(Value::Null);
        value["since"] = self.since(entry).map(Since::to_json).unwrap_or(Value::Null);
        value["verify"] = json!(entry.verify);
        value["premises"] = json!(premises(entry, all).iter().map(Premise::to_json).collect::<Vec<_>>());
        if let Some(c) = checked(entry, confirmations.get(entry.id.as_str()).copied()) {
            value["checked"] = json!(c);
        }
    }
}

/// A fingerprint of the ledger, for snapshots that must change when a verification is added.
pub fn ledger_stamp(app: &App) -> String {
    std::fs::read(app.repo.abs(&ledger::rel_path(app))).map(|b| format!("{:016x}", util::fnv1a64(&b))).unwrap_or_default()
}

fn since_all(app: &App, entries: &[Entry], lines: &[Line], head: &str, want: &dyn Fn(&Entry) -> bool) -> BTreeMap<String, Since> {
    let newest = ledger::latest(lines, false);
    let looks: Vec<Look> = entries
        .iter()
        .filter(|e| e.is_active() && want(e))
        .filter_map(|e| {
            let paths: Vec<&str> = e.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
            if paths.is_empty() {
                return None;
            }
            let v = of(e, newest.get(e.id.as_str()).copied())?;
            let from = match v.source {
                "ledger" => Start::Ledger { at: seconds(&v.at)?, commit: v.commit.clone(), key: (e.id.clone(), v.at.clone()) },
                _ => Start::Checked {
                    since: match parse_checked(&v.at)? {
                        Checked::At(t) => t.timestamp() + 1,
                        Checked::Day(d) => d.succ_opt()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp(),
                    },
                },
            };
            Some(Look { id: &e.id, file: &e.rel_path, paths, from })
        })
        .collect();
    if looks.is_empty() {
        return BTreeMap::new();
    }
    let fingerprint: String = looks.iter().map(Look::fingerprint).collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("verification.json");
    if let Some(c) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()).filter(|c| c.key == key) {
        return c.since;
    }
    let oldest = looks.iter().map(Look::time).min().unwrap_or_default();
    let Some(history) = read_history(app, head, oldest) else { return BTreeMap::new() };
    let commits = parse_commits(&history);
    let landings = if looks.iter().any(|l| matches!(l.from, Start::Ledger { .. })) { read_landings(app, head) } else { HashMap::new() };
    let store = app.store();
    let since = count(&looks, &commits, &landings, head, &|f| store.is_store_path(f));
    if let Ok(text) = serde_json::to_string(&Cache { key, since: since.clone() }) {
        let _ = std::fs::create_dir_all(app.repo.worktree_state_dir());
        let _ = util::write_atomic(&path, &text);
    }
    since
}

/// For every ledger line in HEAD's history, the commit that first added it.
fn read_landings(app: &App, head: &str) -> HashMap<(String, String), String> {
    let rel = ledger::rel_path(app);
    app.repo
        .git_opt(&["log", "-p", "--unified=0", "--no-color", "--no-ext-diff", "--no-textconv", "--format=%x00%H", head, "--", &rel])
        .map(|log| parse_landings(&log))
        .unwrap_or_default()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LandedCache {
    key: String,
    landed: BTreeMap<String, String>,
}

/// By entry file, the commit in HEAD's history that landed each selected entry's verification: the
/// commit that first added its newest ledger line, or for a `checked:` date alone the last commit
/// that changed its file — unless the file has uncommitted changes, since the date may be one of them.
/// Cached per HEAD and verifications; the working-tree check is not.
fn landed_all(app: &App, entries: &[Entry], lines: &[Line], head: &str, want: &dyn Fn(&Entry) -> bool) -> BTreeMap<String, String> {
    let newest = ledger::latest(lines, false);
    let targets: Vec<(&Entry, Verification)> =
        entries.iter().filter(|e| want(e)).filter_map(|e| Some((e, of(e, newest.get(e.id.as_str()).copied())?))).collect();
    if targets.is_empty() {
        return BTreeMap::new();
    }
    let files: Vec<&str> = targets.iter().filter(|(_, v)| v.source == "frontmatter").map(|(e, _)| e.rel_path.as_str()).collect();
    let fingerprint: String = targets.iter().map(|(e, v)| format!("{}|{}|{}|{};", e.id, e.rel_path, v.source, v.at)).collect();
    let key = format!("{head}:{}", util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("verification-landed.json");
    let cached = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<LandedCache>(&t).ok()).filter(|c| c.key == key);
    let mut landed = match cached {
        Some(c) => c.landed,
        None => {
            let mut landed = BTreeMap::new();
            if targets.iter().any(|(_, v)| v.source == "ledger") {
                let landings = read_landings(app, head);
                for (e, v) in targets.iter().filter(|(_, v)| v.source == "ledger") {
                    if let Some(sha) = landings.get(&(e.id.clone(), v.at.clone())) {
                        landed.insert(e.rel_path.clone(), sha.clone());
                    }
                }
            }
            if !files.is_empty() {
                let mut args = vec!["log", "-z", "--no-renames", "--name-only", "--format=%x00%H", head, "--"];
                args.extend(files.iter().copied());
                if let Ok(log) = app.repo.git(&args) {
                    // newest first: the first commit that names a file is the last that changed it
                    for (sha, paths) in crate::freshness::parse_log(&log) {
                        for p in paths.into_iter().filter(|p| files.contains(p)) {
                            landed.entry(p.to_string()).or_insert_with(|| sha.trim().to_string());
                        }
                    }
                }
            }
            if let Ok(text) = serde_json::to_string(&LandedCache { key, landed: landed.clone() }) {
                let _ = std::fs::create_dir_all(app.repo.worktree_state_dir());
                let _ = util::write_atomic(&path, &text);
            }
            landed
        }
    };
    if !files.is_empty() {
        let mut args = vec!["status", "--porcelain=v1", "-z", "--no-renames", "--untracked-files=all", "--"];
        args.extend(files.iter().copied());
        if let Ok(status) = app.repo.git(&args) {
            for dirty in status.split('\0').filter_map(|l| l.get(3..)) {
                landed.remove(dirty);
            }
        }
    }
    landed
}

/// Non-merge commits of HEAD from two days before the oldest look (the count then compares exact
/// positions or times), each with its sha, time, subject and files.
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
            "--format=%x00%H %ct %s",
            head,
            "--",
        ])
        .ok()
}

/// The JSON fields for every entry value in a machine API result (`entry`, `entries`,
/// `matches[].entry`), found by file and id among `all`.
pub fn annotate_result(app: &App, all: &[Entry], result: &mut Value) {
    let mut targets: Vec<&mut Value> = Vec::new();
    let Some(obj) = result.as_object_mut() else { return };
    for (key, value) in obj.iter_mut() {
        match key.as_str() {
            "entry" => targets.push(value),
            "entries" | "matches" => {
                if let Some(items) = value.as_array_mut() {
                    for item in items {
                        if item.get("entry").is_some_and(Value::is_object) {
                            targets.push(item.get_mut("entry").expect("entry"));
                        } else {
                            targets.push(item);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if targets.is_empty() {
        return;
    }
    let find = |v: &Value| -> Option<&Entry> {
        let file = v.get("file").or_else(|| v.get("path")).and_then(Value::as_str);
        let id = v.get("id").and_then(Value::as_str)?;
        all.iter().find(|e| Some(e.rel_path.as_str()) == file && e.id == id).or_else(|| all.iter().find(|e| e.id == id))
    };
    let wanted: BTreeSet<String> = targets.iter().filter_map(|v| find(v)).map(|e| e.rel_path.clone()).collect();
    let ctx = Context::load_landed(app, all, &|e| wanted.contains(&e.rel_path));
    let newest = ctx.newest();
    let confirmations = ledger::latest(&ctx.lines, true);
    for v in targets {
        if let Some(entry) = find(v) {
            ctx.annotate(entry, all, &newest, &confirmations, v);
        }
    }
}

/// What a `verify` command did.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// The last [`TAIL_BYTES`] of stdout and stderr together.
    pub output_tail: String,
}

pub const TAIL_BYTES: usize = 2048;
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// `KONTEXT_VERIFY_TIMEOUT_SECS`, else two minutes.
pub fn timeout() -> std::time::Duration {
    let secs = std::env::var("KONTEXT_VERIFY_TIMEOUT_SECS").ok().and_then(|s| s.trim().parse::<u64>().ok()).filter(|n| *n > 0);
    std::time::Duration::from_secs(secs.unwrap_or(DEFAULT_TIMEOUT_SECS))
}

/// Variables a `verify` command always sees (plus every `LC_*`); `[verify] pass_env` in the user
/// config adds more. Everything else, tokens and cloud keys included, is withheld.
pub const ENV_ALLOW: &[&str] = &["PATH", "HOME", "USER", "LANG", "TMPDIR", "TERM"];

/// How long stdout is still read after the shell exited (or was killed), before the process group
/// is killed: a background child that keeps the pipe open cannot hold the caller.
const DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Process groups of the verify commands running in this process.
static RUNNING: LazyLock<std::sync::Mutex<BTreeSet<u32>>> = LazyLock::new(Default::default);

fn kill_group(pgid: u32) {
    #[cfg(unix)]
    {
        // `--` before the negative id: procps's kill on Linux reads `-123` as an option, BSD's does not mind
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{pgid}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    let _ = pgid;
}

/// Kill every verify command this process started and is still waiting for, with everything it
/// started: the MCP server calls this when its client goes away.
#[allow(dead_code)] // the MCP server calls it when its stdin closes
pub fn kill_running_verifies() {
    let groups: Vec<u32> = RUNNING.lock().map(|g| g.iter().copied().collect()).unwrap_or_default();
    for pgid in groups {
        kill_group(pgid);
    }
}

fn keep_env(name: &str, pass_env: &[String]) -> bool {
    ENV_ALLOW.contains(&name) || name.starts_with("LC_") || pass_env.iter().any(|p| p == name)
}

/// Run `command` with `sh -c` in `root`, stderr folded into stdout, stdin closed, killed (with
/// everything it started) after `limit`. It sees only [`ENV_ALLOW`], `LC_*` and the variables named
/// in `pass_env`; the output tail is redacted.
pub fn run(root: &Path, command: &str, limit: std::time::Duration, pass_env: &[String]) -> anyhow::Result<Outcome> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    let mut cmd = Command::new("sh");
    // `exec 2>&1` merges the streams inside the shell, so their order is kept without a second pipe
    cmd.arg("-c").arg(format!("exec 2>&1\n{command}")).current_dir(root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    cmd.env_clear();
    for (k, v) in std::env::vars_os() {
        if k.to_str().is_some_and(|k| keep_env(k, pass_env)) {
            cmd.env(k, v);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // its own process group: a timeout kills the test runner the shell started, not only the shell
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("could not start sh: {e}"))?;
    let pgid = child.id();
    if let Ok(mut g) = RUNNING.lock() {
        g.insert(pgid);
    }
    let mut stdout = child.stdout.take().expect("piped stdout");
    let tail: Arc<Mutex<Vec<u8>>> = Arc::default();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    {
        let tail = Arc::clone(&tail);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                t.extend_from_slice(&buf[..n]);
                if t.len() > 4 * TAIL_BYTES {
                    let cut = t.len() - TAIL_BYTES;
                    t.drain(..cut);
                }
            }
            let _ = done_tx.send(());
        });
    }
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let waited = (|| -> anyhow::Result<std::process::ExitStatus> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() >= limit {
                timed_out = true;
                kill_group(pgid);
                let _ = child.kill();
                return Ok(child.wait()?);
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    })();
    // the shell is gone: read what is left for a bounded time, then kill whatever it left behind
    let drained = done_rx.recv_timeout(DRAIN_GRACE).is_ok();
    kill_group(pgid);
    if !drained {
        // a descendant that left the group can still hold the pipe: never wait for it
        let _ = done_rx.recv_timeout(std::time::Duration::from_millis(100));
    }
    if let Ok(mut g) = RUNNING.lock() {
        g.remove(&pgid);
    }
    let status = waited?;
    let mut tail = std::mem::take(&mut *tail.lock().unwrap_or_else(|e| e.into_inner()));
    if tail.len() > TAIL_BYTES {
        tail.drain(..tail.len() - TAIL_BYTES);
    }
    // output can carry tokens and connection strings; it goes to agents and into Peer's Hub
    let mut output_tail = crate::secrets::redact(&String::from_utf8_lossy(&tail)).0;
    if timed_out {
        output_tail.push_str(&format!("\n[kontext: stopped after {} s]", limit.as_secs()));
    } else if !drained {
        output_tail.push_str("\n[kontext: a background process kept the output open; it was stopped]");
    }
    let exit_code = if timed_out { None } else { status.code() };
    Ok(Outcome { passed: !timed_out && status.success(), exit_code, timed_out, output_tail })
}

/// Whether a person pinned this entry's `verify` command on this machine (`kontext trust
/// --verify`): the command runs only then. The pin covers the entry's path and the command's text.
pub fn is_pinned(app: &App, entry: &Entry, command: &str) -> bool {
    crate::config::is_verify_pinned(&app.repo, &entry.rel_path, command)
}

/// Active entries whose `verify` command is not pinned on this machine (`only`: these ids).
pub fn unpinned(app: &App, only: Option<&str>) -> Vec<crate::config::VerifyPin> {
    let (entries, _) = app.entries();
    let wanted: Option<String> = only.map(|id| Store::find(&entries, id).map(|e| e.id.clone()).unwrap_or_else(|| id.to_string()));
    entries
        .iter()
        .filter(|e| e.is_active() && wanted.as_deref().is_none_or(|w| w == e.id))
        .filter_map(|e| {
            let command = e.verify.clone()?;
            crate::store::validate_verify(&command).ok()?;
            (!is_pinned(app, e, &command)).then(|| crate::config::VerifyPin { id: e.id.clone(), rel_path: e.rel_path.clone(), command })
        })
        .collect()
}

/// Uncommitted changes (staged, unstaged or untracked) to an entry's file or to the paths it
/// governs: a verify run there checks the working tree, not the commit HEAD names.
pub fn dirty_paths(app: &App, entry: &Entry) -> Vec<String> {
    let ledger = ledger::rel_path(app);
    let mut args: Vec<String> =
        ["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--"].map(String::from).to_vec();
    args.push(format!(":(literal){}", entry.rel_path));
    for p in &entry.paths {
        let p = p.trim_start_matches('/');
        if !p.is_empty() {
            args.push(format!(":(glob){p}"));
        }
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let Ok(out) = app.repo.git_bytes(&args) else { return Vec::new() };
    out.split(|b| *b == 0)
        .filter(|rec| rec.len() > 3)
        .map(|rec| String::from_utf8_lossy(&rec[3..]).into_owned())
        .filter(|p| *p != ledger)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, status: &str, body: &str) -> Entry {
        Entry {
            id: id.into(),
            kind: "decision".into(),
            title: id.into(),
            status: Some(status.into()),
            body: body.into(),
            rel_path: format!(".ai/decisions/{id}.md"),
            paths: vec!["src/**".into()],
            ..Default::default()
        }
    }

    fn line(at: &str, id: &str, kind: &str, commit: Option<&str>) -> Line {
        Line {
            at: at.into(),
            id: id.into(),
            kind: kind.into(),
            commit: commit.map(String::from),
            by: None,
            passed: None,
            because: String::new(),
        }
    }

    #[test]
    fn the_newer_of_the_ledger_and_the_cache_is_the_verification() {
        let mut e = entry("a", "accepted", "");
        assert!(of(&e, None).is_none());
        e.checked = Some("2026-10-20".into());
        let fm = of(&e, None).unwrap();
        assert_eq!((fm.kind.as_str(), fm.source, fm.commit.as_deref()), ("read", "frontmatter", None));
        let l = line("2026-10-20T09:00:00Z", "a", "agent", Some("abc"));
        assert_eq!(of(&e, Some(&l)).unwrap().source, "ledger", "the same day: the line says more");
        assert!(!of(&e, Some(&l)).unwrap().verified(), "an agent's confirmation is a stamp");
        e.checked = Some("2026-10-21T00:00:00Z".into());
        assert_eq!(of(&e, Some(&l)).unwrap().source, "frontmatter");
        assert_eq!(checked(&e, Some(&l)).as_deref(), Some("2026-10-21T00:00:00Z"));
        // a merge took the older side: the cache is behind its ledger
        e.checked = Some("2026-10-01T00:00:00Z".into());
        assert_eq!(behind(&e, Some(&l)).map(|l| l.at.as_str()), Some("2026-10-20T09:00:00Z"));
        assert_eq!(checked(&e, Some(&l)).as_deref(), Some("2026-10-20T09:00:00Z"));
        e.checked = Some("2026-10-20".into());
        assert!(behind(&e, Some(&l)).is_none(), "a day the line falls on");
        e.checked = Some("2026-10-19".into());
        assert!(behind(&e, Some(&l)).is_some());
        e.checked = None;
        assert!(behind(&e, Some(&l)).is_some());
        let mut v = line("2026-10-22T00:00:00Z", "a", "verify", None);
        v.passed = Some(true);
        assert!(of(&e, Some(&v)).unwrap().verified());
    }

    #[test]
    fn premises_are_linked_entries_that_no_longer_hold() {
        let all = vec![
            entry("old-cache", "superseded", ""),
            Entry { superseded_by: vec!["new-cache".into()], ..entry("replaced", "superseded", "") },
            entry("gone", "deprecated", ""),
            entry("fine", "accepted", ""),
            Entry { extra: vec![("extends".into(), crate::store::FmValue::List(vec!["fine".into()]))], ..entry("x", "accepted", "") },
        ];
        let mut e =
            entry("new-cache", "accepted", "Builds on [[old-cache]], [[gone|the retired one]], [[fine]] and [[missing]]. Again [[gone]].");
        e.supersedes = vec!["replaced".into()];
        let p = premises(&e, &all);
        assert_eq!(
            p.iter().map(|p| (p.id.as_str(), p.status.as_str())).collect::<Vec<_>>(),
            vec![("old-cache", "superseded"), ("gone", "deprecated")]
        );
        // the replaced entry is a premise when something else replaced it
        e.supersedes = vec!["replaced".into()];
        let other = Entry { id: "other".into(), rel_path: ".ai/decisions/other.md".into(), ..e.clone() };
        assert!(premises(&other, &all).iter().any(|p| p.id == "replaced" && p.superseded_by == vec!["new-cache".to_string()]));
        // `extends` counts too, and an inactive entry has none
        let ext =
            Entry { extra: vec![("extends".into(), crate::store::FmValue::List(vec!["gone".into()]))], ..entry("ext", "accepted", "") };
        assert_eq!(premises(&ext, &all).len(), 1);
        assert!(premises(&entry("y", "superseded", "[[gone]]"), &all).is_empty());
    }

    #[test]
    fn counts_from_the_commit_a_look_named_or_landed_in() {
        let c = |sha: &str, time: i64, paths: &[&str]| Commit {
            sha: sha.into(),
            time,
            subject: format!("s {sha}"),
            paths: paths.iter().map(|p| p.to_string()).collect(),
        };
        // newest first
        let commits = vec![
            c("e", 500, &["src/e.rs", ".ai/confirmations.log"]),
            c("d", 400, &["docs/x.md"]),
            c("c", 300, &["src/c.rs", "src/c2.rs"]),
            c("b", 200, &["src/b.rs", ".ai/confirmations.log"]),
            c("a", 100, &["src/a.rs"]),
        ];
        let look = |id: &'static str, from: Start| Look { id, file: ".ai/decisions/x.md", paths: vec!["src/**", ".ai/**"], from };
        let mut landings = HashMap::new();
        landings.insert(("landed".to_string(), "t".to_string()), "b".to_string());
        let looks = vec![
            look("named", Start::Ledger { at: 150, commit: Some("a".into()), key: ("named".into(), "t".into()) }),
            look("landed", Start::Ledger { at: 150, commit: Some("a".into()), key: ("landed".into(), "t".into()) }),
            look("elsewhere", Start::Ledger { at: 350, commit: Some("zzz".into()), key: ("elsewhere".into(), "t".into()) }),
            look("cached", Start::Checked { since: 250 }),
        ];
        let counts = count(&looks, &commits, &landings, "e", &|f: &str| f.starts_with(".ai/"));
        assert_eq!(counts["named"].commits, 3, "b, c and e after a");
        assert_eq!(counts["named"].paths, vec!["src/b.rs", "src/c.rs", "src/c2.rs", "src/e.rs"], "the ledger itself is not code");
        assert_eq!(counts["named"].recent[0], ("e".to_string(), "s e".to_string()));
        assert_eq!(counts["landed"].commits, 2, "the commit that landed the line was reviewed with it");
        assert_eq!(counts["elsewhere"].commits, 1, "a commit outside HEAD's history: by time");
        assert_eq!(counts["cached"].commits, 2);
        assert_eq!(counts["cached"].head, "e");
    }

    #[test]
    fn landings_name_the_commit_that_first_added_a_line() {
        let log = "\0bbb\n\ndiff --git a/.ai/confirmations.log b/.ai/confirmations.log\n+++ b/.ai/confirmations.log\n@@ -1 +2 @@\n+2026-10-27T10:00:00Z\tx\tread\t-\t-\t-\t\n\0aaa\n\n+++ b/.ai/confirmations.log\n+2026-10-26T10:00:00Z\ty\tagent\t-\t-\t-\t\n";
        let l = parse_landings(log);
        assert_eq!(l[&("x".to_string(), "2026-10-27T10:00:00Z".to_string())], "bbb");
        assert_eq!(l[&("y".to_string(), "2026-10-26T10:00:00Z".to_string())], "aaa");
    }

    #[test]
    fn a_command_passes_fails_or_is_stopped() {
        let dir = std::env::temp_dir();
        let ok = run(&dir, "echo out; echo err >&2", std::time::Duration::from_secs(10), &[]).unwrap();
        assert!(ok.passed && ok.exit_code == Some(0) && !ok.timed_out);
        assert_eq!(ok.output_tail, "out\nerr\n");
        let bad = run(&dir, "printf '%05000d' 0; exit 3", std::time::Duration::from_secs(10), &[]).unwrap();
        assert!(!bad.passed && bad.exit_code == Some(3));
        assert_eq!(bad.output_tail.len(), TAIL_BYTES, "the tail only");
        let started = std::time::Instant::now();
        let slow = run(&dir, "sleep 30", std::time::Duration::from_millis(300), &[]).unwrap();
        assert!(slow.timed_out && !slow.passed && slow.exit_code.is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "the sleep was killed with its shell");
    }
}

/// The ledger, the read side and the brief over a real repository.
#[cfg(test)]
mod repo_tests {
    use crate::app::App;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "kontext-verification-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            let f = Self(path);
            f.git(&["init", "-q", "-b", "main"]);
            f.git(&["config", "user.email", "test@example.invalid"]);
            f.git(&["config", "user.name", "Ana Test"]);
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
        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.0.join(path)).unwrap()
        }
        fn entry(&self, id: &str, extra: &str, body: &str) {
            self.write(
                &format!(".ai/conventions/{id}.md"),
                &format!("---\nid: {id}\nkind: convention\ntitle: Rule {id}\nstatus: accepted\ndate: 2026-01-10\npaths: [src/**]\n{extra}---\n\n{body}\n"),
            );
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

    fn ok(app: &App, op: &str, input: Value) -> Value {
        let v = crate::api::execute(app, op, &input);
        assert_eq!(v["ok"], true, "{v}");
        v["result"].clone()
    }

    fn get(app: &App, id: &str) -> Value {
        ok(app, "entries.get", json!({"id": id}))["entry"].clone()
    }

    #[test]
    fn confirmations_land_in_a_union_merged_ledger_and_count_commits_since() {
        let f = Fixture::new();
        for id in ["alpha", "b1", "b2", "b3", "b4", "b5", "b6"] {
            f.entry(id, "", &format!("Rule {id}."));
        }
        f.write(".ai/.gitattributes", "*.md text\n");
        f.write("src/lib.rs", "pub fn a() {}\n");
        let seed = f.commit("seed");
        let app = f.app();

        let one = crate::api_write::confirm_ids(&app, &["alpha".into()], "read", Some("read it\tagainst src/lib.rs"));
        assert_eq!(one[0].as_ref().unwrap()["ledger"]["kind"], "read");
        let attributes = f.read(".ai/.gitattributes");
        assert!(attributes.starts_with("*.md text\n") && attributes.contains("\nconfirmations.log merge=union\n"), "{attributes}");
        let ids: Vec<String> = (1..=6).map(|n| format!("b{n}")).collect();
        let bulk = crate::api_write::confirm_ids(&app, &ids, "read", None);
        assert!(bulk.iter().all(|r| r.as_ref().unwrap()["ledger"]["kind"] == "bulk"), "more than five ids are a bulk stamp");
        let lines = crate::ledger::parse_all(&f.read(".ai/confirmations.log"));
        assert_eq!(lines.len(), 7);
        assert_eq!(
            (lines[0].kind.as_str(), lines[0].commit.as_deref(), lines[0].by.as_deref(), lines[0].because.as_str()),
            ("read", Some(seed.as_str()), Some("Ana Test"), "read it against src/lib.rs")
        );
        let staged = f.git(&["diff", "--cached", "--name-only"]);
        assert!(staged.contains(".ai/.gitattributes") && staged.contains(".ai/confirmations.log"), "{staged}");
        // a second write keeps the attributes as they are
        crate::ledger::ensure_attributes(&app.repo, ".ai").unwrap();
        assert_eq!(f.read(".ai/.gitattributes"), attributes);
        let landed = f.commit("docs: rules still hold");

        // nothing on the paths since the look
        let alpha = get(&app, "alpha");
        assert_eq!(alpha["verification"]["kind"], "read");
        assert_eq!(alpha["verification"]["commit"], seed.as_str());
        assert_eq!(alpha["verification"]["source"], "ledger");
        assert_eq!(alpha["verification"]["verified"], true);
        assert_eq!(alpha["since"]["commits"], 0, "the commit that landed the confirmation was reviewed with it");
        assert_eq!(alpha["checked"], alpha["verification"]["at"]);
        assert_eq!(get(&app, "b1")["verification"]["verified"], false, "a bulk stamp is not a verification");

        f.write("src/lib.rs", "pub fn a() -> u8 { 1 }\n");
        f.write("README.md", "unrelated\n");
        let change = f.commit("feat: a returns one");
        f.write("README.md", "unrelated again\n");
        f.commit("docs: readme");
        let alpha = get(&app, "alpha");
        assert_eq!(
            alpha["since"],
            json!({"commits":1,"head":f.git(&["rev-parse","HEAD"]),"paths":["src/lib.rs"],"recent":[{"sha":change,"subject":"feat: a returns one"}]})
        );
        let listed = ok(&app, "entries.list", json!({}));
        assert!(listed["entries"].as_array().unwrap().iter().all(|e| e["since"]["commits"] == 1 && e["premises"] == json!([])));
        assert_ne!(landed, change);

        // the brief says how each governing entry was verified and what changed since
        let brief = crate::ops::brief(&app, &["src/lib.rs".into()], 6000, false);
        assert!(brief.contains(&format!("  verified read at {} (", &seed[..7])), "{brief}");
        assert!(brief.contains("; 1 commit on its paths since: feat: a returns one"), "{brief}");
        assert!(brief.contains("verified bulk at"), "{brief}");
        assert_eq!(brief.matches(crate::ops::READ_BEFORE_CHANGING).count(), 1, "{brief}");
        let unfocused = crate::ops::brief(&app, &[], 6000, false);
        assert!(!unfocused.contains("verified ") && !unfocused.contains(crate::ops::READ_BEFORE_CHANGING));
    }

    #[test]
    fn two_branches_confirming_merge_without_a_conflict() {
        let f = Fixture::new();
        f.entry("a", "", "A.");
        f.entry("b", "", "B.");
        f.write("src/lib.rs", "\n");
        f.commit("seed");
        let app = f.app();
        // the first write brings the attribute; it reaches both branches before they part
        crate::api_write::confirm_ids(&app, &["a".into()], "read", None);
        f.commit("confirm a");
        f.git(&["checkout", "-q", "-b", "side"]);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        crate::api_write::confirm_ids(&app, &["b".into()], "read", None);
        f.commit("confirm b on side");
        f.git(&["checkout", "-q", "main"]);
        crate::api_write::confirm_ids(&app, &["a".into()], "agent", None);
        f.commit("confirm a again on main");
        f.git(&["merge", "-q", "--no-edit", "side"]);
        let lines = crate::ledger::parse_all(&f.read(".ai/confirmations.log"));
        assert_eq!(lines.len(), 3, "both sides' lines are kept");
        assert!(!f.read(".ai/confirmations.log").contains("<<<<<<<"));
    }

    #[test]
    fn verify_runs_the_entry_command_and_records_the_outcome() {
        let f = Fixture::new();
        f.entry("lib-exists", "verify: test -f src/lib.rs && echo found\n", "The library has a root.");
        f.entry("broken", "verify: \"echo nope >&2; exit 3\"\n", "Always fails.");
        f.entry("plain", "", "No command.");
        f.entry("gone", "verify: touch ran\nstatus: deprecated\n", "Retired.");
        f.write("src/lib.rs", "\n");
        let seed = f.commit("seed");
        let app = f.app();
        assert_eq!(get(&app, "lib-exists")["verify"], "test -f src/lib.rs && echo found");
        assert_eq!(get(&app, "lib-exists")["verification"], Value::Null);
        assert_eq!(get(&app, "lib-exists")["since"], Value::Null, "never verified: nothing to count from");
        // nothing runs before a person pins the command on this machine
        crate::config::set_test_trust_dir(&f.0.join(".git/kontext-trust"));
        let refused = crate::api::execute(&app, "drafts.verify", &json!({"id":"lib-exists"}));
        assert_eq!(refused["error"]["code"], "untrusted_verify", "{refused}");
        assert_eq!(refused["error"]["command"], "test -f src/lib.rs && echo found");
        assert_eq!(refused["error"]["hint"], "kontext trust --verify lib-exists");
        assert!(!f.0.join(".ai/confirmations.log").exists());
        let pins = crate::verification::unpinned(&app, None);
        let mut ids: Vec<&str> = pins.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["broken", "lib-exists"], "never an inactive entry's");
        crate::config::pin_verify(&app.repo, &pins).unwrap();
        assert!(crate::verification::unpinned(&app, None).is_empty());

        let passed = ok(&app, "drafts.verify", json!({"id":"lib-exists","because":"before the refactor"}));
        assert_eq!(passed["passed"], true);
        assert_eq!(passed["exit_code"], 0);
        assert_eq!(passed["output_tail"], "found\n");
        assert_eq!(passed["commit"], seed.as_str());
        let failed = ok(&app, "drafts.verify", json!({"id":"broken"}));
        assert_eq!(
            (failed["passed"].clone(), failed["exit_code"].clone(), failed["output_tail"].clone()),
            (json!(false), json!(3), json!("nope\n"))
        );
        let lines = crate::ledger::parse_all(&f.read(".ai/confirmations.log"));
        assert_eq!(
            lines.iter().map(|l| (l.id.as_str(), l.kind.as_str(), l.passed)).collect::<Vec<_>>(),
            vec![("lib-exists", "verify", Some(true)), ("broken", "verify", Some(false))]
        );
        assert_eq!(lines[0].because, "before the refactor");
        assert!(f.git(&["diff", "--cached", "--name-only"]).contains(".ai/confirmations.log"));
        let v = get(&app, "broken")["verification"].clone();
        assert_eq!((v["kind"].clone(), v["passed"].clone(), v["verified"].clone()), (json!("verify"), json!(false), json!(false)));
        assert_eq!(get(&app, "lib-exists")["verification"]["verified"], true);
        // a verify line is not a confirmation: the checked cache is not behind it
        assert!(!crate::ops::check_all(&app).issues.iter().any(|i| i.msg.contains("behind its ledger")));

        let err = |id: &str| crate::api::execute(&app, "drafts.verify", &json!({"id": id}))["error"]["code"].clone();
        assert_eq!(err("plain"), "invalid_input");
        assert_eq!(err("gone"), "operation_conflict");
        assert!(!f.0.join("ran").exists(), "an inactive entry's command never runs");
        assert_eq!(err("nope"), "not_found");
    }

    #[test]
    fn a_verification_says_which_commit_landed_it() {
        let f = Fixture::new();
        f.entry("cached", "checked: 2026-01-11\n", "Only a date in the file.");
        f.entry("looked", "", "Confirmed through the ledger.");
        f.write("src/lib.rs", "\n");
        let seed = f.commit("seed");
        f.write("src/lib.rs", "pub fn a() {}\n");
        f.commit("feat: a");
        let app = f.app();
        let cached = get(&app, "cached")["verification"].clone();
        assert_eq!(
            (cached["source"].clone(), cached["landed"].clone()),
            (json!("frontmatter"), json!(seed)),
            "the last commit of its file"
        );
        assert_eq!(get(&app, "looked")["verification"], Value::Null);

        crate::api_write::confirm_ids(&app, &["looked".into()], "read", None);
        let staged = get(&app, "looked")["verification"].clone();
        assert_eq!(staged["source"], "ledger");
        assert_eq!(staged["landed"], Value::Null, "only staged");
        let landed = f.commit("docs: looked still holds");
        assert_eq!(get(&app, "looked")["verification"]["landed"], landed.as_str());
        assert_eq!(get(&app, "looked")["since"]["commits"], 0);
        let listed = ok(&app, "entries.list", json!({}));
        let row = listed["entries"].as_array().unwrap().iter().find(|e| e["id"] == "looked").unwrap().clone();
        assert_eq!(row["verification"]["landed"], landed.as_str());

        // a later commit leaves it where it landed; a `checked:` date edited in the working tree has not landed
        f.write("src/lib.rs", "pub fn a() -> u8 { 1 }\n");
        f.commit("feat: a returns one");
        assert_eq!(get(&app, "looked")["verification"]["landed"], landed.as_str());
        assert_eq!(get(&app, "looked")["since"]["commits"], 1);
        f.write(".ai/conventions/cached.md", &f.read(".ai/conventions/cached.md").replace("2026-01-11", "2026-01-12"));
        assert_eq!(get(&app, "cached")["verification"]["landed"], Value::Null);
        // the brief has no use for it
        let brief = crate::ops::brief(&app, &["src/lib.rs".into()], 6000, false);
        assert!(brief.contains("verified read at"), "{brief}");
    }

    #[test]
    fn a_superseded_premise_is_flagged_never_rewritten() {
        let f = Fixture::new();
        f.entry("old-cache", "superseded_by: [new-cache]\n", "Cache everything.");
        f.write(".ai/conventions/old-cache.md", &f.read(".ai/conventions/old-cache.md").replace("status: accepted", "status: superseded"));
        f.entry("new-cache", "supersedes: [old-cache]\n", "Cache reads only.");
        f.entry("ttl", "", "TTLs follow [[old-cache]].");
        f.write("src/lib.rs", "\n");
        f.commit("seed");
        let before = f.read(".ai/conventions/ttl.md");
        let app = f.app();
        assert_eq!(get(&app, "ttl")["premises"], json!([{"id":"old-cache","status":"superseded","superseded_by":["new-cache"]}]));
        assert_eq!(get(&app, "new-cache")["premises"], json!([]), "the entry that replaced it");
        let rep = crate::ops::check_all(&app);
        let info: Vec<&str> = rep.issues.iter().filter(|i| i.level == crate::store::Level::Info).map(|i| i.msg.as_str()).collect();
        assert_eq!(info, vec!["ttl links to old-cache, which is superseded by new-cache: does it still hold?"]);
        assert_eq!(rep.errors() + rep.warnings(), 0);
        let brief = crate::ops::brief(&app, &["src/lib.rs".into()], 6000, false);
        assert!(brief.contains("never verified; premise changed: old-cache is superseded"), "{brief}");
        assert_eq!(f.read(".ai/conventions/ttl.md"), before);
    }
}
