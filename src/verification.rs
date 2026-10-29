//! What a reader needs to trust an entry: how it was last looked at (the newest of its ledger lines
//! and the `checked:` cache in its file) and last verified (a person's read or a passed command),
//! what changed on its governed paths since (counted by ancestry from the commits the look named and
//! landed in, not from a date or a position in the log), the premises it links to that no longer
//! hold, and running its `verify` command.
//!
//! One `git log` of HEAD (shared with freshness) and one `git log -p` of the ledger cover every
//! entry, cached per HEAD with a fingerprint of the looks, so listing entries costs a `rev-parse`
//! while nothing is committed.
use crate::app::App;
use crate::freshness::{History, Reach};
use crate::glob::path_matches;
use crate::ledger::{self, Line, Looks};
use crate::store::{Checked, Entry, Store, parse_checked};
use crate::util;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

/// Statuses that make a linked entry a changed premise.
const GONE: &[&str] = &["superseded", "deprecated", "retired"];
/// At most this many changed governed files per entry.
const MAX_PATHS: usize = 20;
/// At most this many commits named per entry.
const MAX_RECENT: usize = 5;

/// Capability strings for what this module adds to API v1 (`status.capabilities`): the
/// `last_verified`, `since_verified`, `failing` and `amended_since` fields, and drift that needs a
/// look at or after the last governed change and says where its base came from (`base_source`).
#[allow(dead_code)] // wired by the integrator
pub const CAPABILITIES: &[&str] = &["verification.last_verified", "drift.ordered"];

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
    /// `read`, `agent`, `bulk` or `verify` from the ledger; `cache` for a `checked:` date with no
    /// ledger line behind it.
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
    /// are stamps, and a `checked:` date nobody recorded is a cache.
    pub fn verified(&self) -> bool {
        self.kind == "read" || (self.kind == "verify" && self.passed == Some(true))
    }

    pub fn to_json(&self) -> Value {
        json!({"at":self.at,"kind":self.kind,"commit":self.commit,"by":self.by,"because":self.because,
            "passed":self.passed,"source":self.source,"verified":self.verified(),"landed":self.landed})
    }
}

/// The newer of the entry's newest ledger line and its frontmatter `checked`; the ledger line wins
/// a tie, since it says more. A `checked` date alone is a `cache`: it is kept and shown, never
/// counted as verified.
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
        kind: "cache".into(),
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

/// An entry's newest verifying look (a person's read or a passed command): `last_verified`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastVerified {
    pub kind: String,
    pub at: String,
    pub commit: Option<String>,
    /// The commit in HEAD's history that first added its line; None while only staged.
    pub landed: Option<String>,
}

impl LastVerified {
    pub fn to_json(&self) -> Value {
        json!({"kind":self.kind,"at":self.at,"commit":self.commit,"landed":self.landed})
    }
}

fn is_false(b: &bool) -> bool {
    !*b
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
    /// The look is older than the history read (`index.max_commits`): every commit read counts.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    /// A shallow clone: history stops at the graft, so the count is a lower bound.
    #[serde(default, skip_serializing_if = "is_false")]
    pub lower_bound: bool,
}

impl Since {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"commits":self.commits,"head":self.head,"paths":self.paths,
            "recent":self.recent.iter().map(|(sha,subject)| json!({"sha":sha,"subject":subject})).collect::<Vec<_>>()});
        if self.truncated {
            v["truncated"] = json!(true);
        }
        if self.lower_bound {
            v["lower_bound"] = json!(true);
        }
        v
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
    /// A ledger line: after the commits it named and landed in, by ancestry; by time when neither
    /// is in HEAD's history.
    Ledger { at: i64, anchors: Vec<String>, key: (String, String) },
    /// A `checked:` date only: after its second (a day: from the next one), and after every commit
    /// that changed the entry's own file, by ancestry.
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
            Start::Ledger { at, anchors, .. } => format!("l{at}{}", anchors.join(",")),
            Start::Checked { since } => format!("c{since}"),
        };
        format!("{}|{}|{}|{from};", self.id, self.file, self.paths.join(","))
    }
}

/// A look at an entry, from what [`of`] says of it.
fn look_from<'a>(e: &'a Entry, v: &Verification) -> Option<Look<'a>> {
    let paths: Vec<&str> = e.paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if paths.is_empty() {
        return None;
    }
    let from = match v.source {
        "ledger" => Start::Ledger { at: seconds(&v.at)?, anchors: v.commit.iter().cloned().collect(), key: (e.id.clone(), v.at.clone()) },
        _ => Start::Checked {
            since: match parse_checked(&v.at)? {
                Checked::At(t) => t.timestamp() + 1,
                Checked::Day(d) => d.succ_opt()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp(),
            },
        },
    };
    Some(Look { id: &e.id, file: &e.rel_path, paths, from })
}

/// `git log -p --format=%x00%H -- <ledger>`: for each line, the commit that first added it.
pub(crate) fn parse_landings(log: &str) -> HashMap<(String, String), String> {
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

/// The count for one look over HEAD's history.
fn count_one(look: &Look, history: &History, reach: &mut Reach, head: &str, is_store: &dyn Fn(&str) -> bool) -> Since {
    let (sources, after, missing) = match &look.from {
        Start::Ledger { at, anchors, .. } => {
            let sources: Vec<usize> = anchors.iter().filter_map(|a| history.position(a)).collect();
            match sources.is_empty() {
                // older than the read: every commit read counts; elsewhere (another branch, rebased
                // away): by time
                true if history.truncated => (sources, i64::MIN, true),
                true => (sources, *at + 1, false),
                false => (sources, i64::MIN, false),
            }
        }
        Start::Checked { since } => (history.touching(look.file).to_vec(), *since, false),
    };
    let looked = reach.of(sources);
    let governed = |f: &str| !is_store(f) && look.paths.iter().any(|p| path_matches(p, f));
    let c = history.count(looked.as_deref().map(Vec::as_slice), after, &governed, MAX_RECENT);
    Since {
        commits: c.commits,
        head: head.to_string(),
        paths: c.paths.into_iter().take(MAX_PATHS).collect(),
        recent: c.recent,
        truncated: missing,
        lower_bound: history.shallow,
    }
}

/// The count for every look; ledger looks also count from the commit that landed their line.
fn count(
    looks: &mut [Look],
    history: &History,
    landings: &HashMap<(String, String), String>,
    head: &str,
    is_store: &dyn Fn(&str) -> bool,
) -> BTreeMap<String, Since> {
    let mut reach = Reach::new(history);
    looks
        .iter_mut()
        .map(|look| {
            if let Start::Ledger { anchors, key, .. } = &mut look.from
                && let Some(sha) = landings.get(key)
            {
                anchors.push(sha.clone());
            }
            (look.id.to_string(), count_one(look, history, &mut reach, head, is_store))
        })
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    key: String,
    since: BTreeMap<String, Since>,
    #[serde(default)]
    since_verified: BTreeMap<String, Since>,
}

/// The ledger and what changed since each entry's look and verification, read once for a set of
/// entries.
pub struct Context {
    lines: Vec<Line>,
    since: BTreeMap<String, Since>,
    since_verified: BTreeMap<String, Since>,
    /// By entry file: the commit that landed its verification (only with [`Context::load_landed`]).
    landed: BTreeMap<String, String>,
    /// By entry file: the commit that landed its newest verifying line.
    landed_verified: BTreeMap<String, String>,
    /// Entry files whose body changed after the commit that landed their last verification.
    amended: BTreeSet<String>,
}

impl Context {
    /// Verifications of `entries` and, for the active ones with paths, the commits since (counted
    /// for all of them at once, so every caller shares one cache; nothing is read when `want`
    /// selects none).
    pub fn load(app: &App, entries: &[Entry], want: &dyn Fn(&Entry) -> bool) -> Context {
        let lines = ledger::read(app);
        let head = app.repo.head();
        let (since, since_verified) = match &head {
            Some(head) => since_all(app, entries, &lines, head, want),
            None => (BTreeMap::new(), BTreeMap::new()),
        };
        Context { lines, since, since_verified, landed: BTreeMap::new(), landed_verified: BTreeMap::new(), amended: BTreeSet::new() }
    }

    /// [`Context::load`], and where each selected entry's verification landed and whether its body
    /// changed since (what the machine API reports; the brief does not need it).
    pub fn load_landed(app: &App, entries: &[Entry], want: &dyn Fn(&Entry) -> bool) -> Context {
        let mut ctx = Context::load(app, entries, want);
        if let Some(head) = app.repo.head() {
            let l = landed_all(app, entries, &ctx.lines, &head, want);
            ctx.landed = l.landed;
            ctx.landed_verified = l.landed_verified;
            ctx.amended = l.amended;
        }
        ctx
    }

    pub fn newest(&self) -> HashMap<&str, &Line> {
        ledger::latest(&self.lines, false)
    }

    /// Per entry id: newest line, newest look, newest verification, failing.
    pub fn looks(&self) -> HashMap<&str, Looks<'_>> {
        ledger::looks(&self.lines)
    }

    pub fn verification(&self, entry: &Entry) -> Option<Verification> {
        self.with_landed(entry, of(entry, self.newest().get(entry.id.as_str()).copied()))
    }

    fn with_landed(&self, entry: &Entry, verification: Option<Verification>) -> Option<Verification> {
        verification.map(|v| Verification { landed: self.landed.get(&entry.rel_path).cloned(), ..v })
    }

    /// Commits on the entry's paths since its newest look (a failed verify is not one).
    pub fn since(&self, entry: &Entry) -> Option<&Since> {
        self.since.get(&entry.id)
    }

    /// Commits on the entry's paths since its last verification (a read or a passed verify).
    pub fn since_verified(&self, entry: &Entry) -> Option<&Since> {
        self.since_verified.get(&entry.id)
    }

    /// The entry's newest read or passed verify.
    pub fn last_verified(&self, entry: &Entry) -> Option<LastVerified> {
        let looks = self.looks();
        let l = looks.get(entry.id.as_str())?.verified?;
        Some(LastVerified {
            kind: l.kind.clone(),
            at: l.at.clone(),
            commit: l.commit.clone(),
            landed: self.landed_verified.get(&entry.rel_path).cloned(),
        })
    }

    /// The entry's newest `verify` failed, and none passed since.
    pub fn failing(&self, entry: &Entry) -> bool {
        self.looks().get(entry.id.as_str()).is_some_and(|l| l.failing)
    }

    /// The entry's body changed after its last verification landed (only with [`Context::load_landed`]).
    pub fn amended_since(&self, entry: &Entry) -> bool {
        self.amended.contains(&entry.rel_path)
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
        value["last_verified"] = self.last_verified(entry).as_ref().map(LastVerified::to_json).unwrap_or(Value::Null);
        value["since_verified"] = self.since_verified(entry).map(Since::to_json).unwrap_or(Value::Null);
        value["failing"] = json!(self.failing(entry));
        value["amended_since"] = json!(self.amended_since(entry));
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

/// Commits since the newest look and since the last verification of every active entry with
/// paths, by id.
fn since_all(
    app: &App,
    entries: &[Entry],
    lines: &[Line],
    head: &str,
    want: &dyn Fn(&Entry) -> bool,
) -> (BTreeMap<String, Since>, BTreeMap<String, Since>) {
    let looks = ledger::looks(lines);
    let get = |e: &Entry| looks.get(e.id.as_str()).copied().unwrap_or_default();
    let active: Vec<&Entry> = entries.iter().filter(|e| e.is_active()).collect();
    let mut seen: Vec<Look> = active.iter().filter_map(|e| look_from(e, &of(e, get(e).look)?)).collect();
    let mut verified: Vec<Look> = active
        .iter()
        .filter_map(|e| {
            let line = get(e).verified?;
            look_from(e, &of(&Entry::default(), Some(line))?)
        })
        .collect();
    let wanted = |l: &Look| entries.iter().any(|e| e.id == l.id && want(e));
    if !seen.iter().chain(verified.iter()).any(wanted) {
        return (BTreeMap::new(), BTreeMap::new());
    }
    let fingerprint: String =
        seen.iter().map(Look::fingerprint).chain(["|v|".to_string()]).chain(verified.iter().map(Look::fingerprint)).collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("verification.json");
    if let Some(c) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()).filter(|c| c.key == key) {
        return (c.since, c.since_verified);
    }
    let Some(history) = crate::freshness::history(app, head) else { return (BTreeMap::new(), BTreeMap::new()) };
    let ledger_looks = seen.iter().chain(verified.iter()).any(|l| matches!(l.from, Start::Ledger { .. }));
    let landings = if ledger_looks { landings(app, head) } else { Arc::new(HashMap::new()) };
    let store = app.store();
    let is_store = |f: &str| store.is_store_path(f);
    let since = count(&mut seen, &history, &landings, head, &is_store);
    let since_verified = count(&mut verified, &history, &landings, head, &is_store);
    if let Ok(text) = serde_json::to_string(&Cache { key, since: since.clone(), since_verified: since_verified.clone() }) {
        let _ = std::fs::create_dir_all(app.repo.worktree_state_dir());
        let _ = util::write_atomic(&path, &text);
    }
    (since, since_verified)
}

static LANDINGS: LazyLock<Mutex<HashMap<String, Arc<HashMap<(String, String), String>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// For every ledger line in HEAD's history (bounded by `index.max_commits` commits of the ledger),
/// the commit that first added it: one `git log -p` per HEAD and process, shared by every reader.
pub(crate) fn landings(app: &App, head: &str) -> Arc<HashMap<(String, String), String>> {
    let rel = ledger::rel_path(app);
    let max = app.cfg().index.max_commits.max(1);
    let key = format!("{}\0{head}\0{rel}\0{max}", app.repo.root.display());
    if let Some(l) = LANDINGS.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return l;
    }
    let max = format!("--max-count={max}");
    let map = app
        .repo
        .git_opt(&["log", "-p", "--unified=0", "--no-color", "--no-ext-diff", "--no-textconv", &max, "--format=%x00%H", head, "--", &rel])
        .map(|log| parse_landings(&log))
        .unwrap_or_default();
    let map = Arc::new(map);
    if let Ok(mut m) = LANDINGS.lock() {
        if m.len() >= 8 {
            m.clear();
        }
        m.insert(key, map.clone());
    }
    map
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LandedCache {
    key: String,
    landed: BTreeMap<String, String>,
    #[serde(default)]
    landed_verified: BTreeMap<String, String>,
    #[serde(default)]
    amended: BTreeSet<String>,
}

struct Landed {
    landed: BTreeMap<String, String>,
    landed_verified: BTreeMap<String, String>,
    amended: BTreeSet<String>,
}

/// The body of an entry's Markdown, for telling an amendment from a metadata edit.
fn body_of(path: &str, text: &str, kind: &str) -> String {
    Entry::parse(path, text, kind).body.trim().to_string()
}

/// By entry file, the commit in HEAD's history that landed each entry's verification: the commit
/// that first added its newest ledger line, or for a `checked:` date alone the last commit that
/// changed its file — unless the file has uncommitted changes, since the date may be one of them.
/// Also where its last verifying line landed, and whether the body changed in a commit that line
/// does not reach. Computed for every entry once per HEAD and verifications (cached); `want` only
/// decides whether anything is read, and the working-tree check is not cached.
fn landed_all(app: &App, entries: &[Entry], lines: &[Line], head: &str, want: &dyn Fn(&Entry) -> bool) -> Landed {
    let empty = || Landed { landed: BTreeMap::new(), landed_verified: BTreeMap::new(), amended: BTreeSet::new() };
    let looks = ledger::looks(lines);
    let targets: Vec<(&Entry, Verification, Option<&Line>)> = entries
        .iter()
        .filter_map(|e| {
            let l = looks.get(e.id.as_str()).copied().unwrap_or_default();
            Some((e, of(e, l.newest)?, l.verified))
        })
        .collect();
    if !targets.iter().any(|(e, _, _)| want(e)) {
        return empty();
    }
    let files: Vec<&str> = targets.iter().filter(|(_, v, _)| v.source == "frontmatter").map(|(e, _, _)| e.rel_path.as_str()).collect();
    let fingerprint: String = targets
        .iter()
        .map(|(e, v, l)| format!("{}|{}|{}|{}|{};", e.id, e.rel_path, v.source, v.at, l.map(|l| l.at.as_str()).unwrap_or("-")))
        .collect();
    let key = format!("{head}:{}:{}", app.cfg().index.max_commits, util::short_hash(&fingerprint));
    let path = app.repo.worktree_state_dir().join("verification-landed.json");
    let cached = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<LandedCache>(&t).ok()).filter(|c| c.key == key);
    let mut out = match cached {
        Some(c) => Landed { landed: c.landed, landed_verified: c.landed_verified, amended: c.amended },
        None => {
            let mut out = empty();
            let landings = if targets.iter().any(|(_, v, l)| v.source == "ledger" || l.is_some()) {
                landings(app, head)
            } else {
                Arc::new(HashMap::new())
            };
            for (e, v, l) in &targets {
                if v.source == "ledger"
                    && let Some(sha) = landings.get(&(e.id.clone(), v.at.clone()))
                {
                    out.landed.insert(e.rel_path.clone(), sha.clone());
                }
                if let Some(sha) = l.and_then(|l| landings.get(&(l.id.clone(), l.at.clone()))) {
                    out.landed_verified.insert(e.rel_path.clone(), sha.clone());
                }
            }
            let history = crate::freshness::history(app, head);
            if let Some(history) = &history {
                // newest first: the first commit that names a file is the last that changed it
                for f in &files {
                    if let Some(i) = history.touching(f).first() {
                        out.landed.insert(f.to_string(), history.commits[*i].sha.clone());
                    }
                }
                let mut reach = Reach::new(history);
                for (e, _, _) in &targets {
                    let Some(anchor) = out.landed_verified.get(&e.rel_path).and_then(|s| history.position(s)) else { continue };
                    let Some(seen) = reach.of(vec![anchor]) else { continue };
                    if history.touching(&e.rel_path).iter().all(|i| seen[*i]) {
                        continue;
                    }
                    let sha = &history.commits[anchor].sha;
                    let Some(then) = app.repo.git_opt(&["show", &format!("{sha}:{}", e.rel_path)]) else { continue };
                    let Some(now) = app.repo.git_opt(&["show", &format!("{head}:{}", e.rel_path)]) else { continue };
                    if body_of(&e.rel_path, &then, &e.kind) != body_of(&e.rel_path, &now, &e.kind) {
                        out.amended.insert(e.rel_path.clone());
                    }
                }
            }
            if let Ok(text) = serde_json::to_string(&LandedCache {
                key,
                landed: out.landed.clone(),
                landed_verified: out.landed_verified.clone(),
                amended: out.amended.clone(),
            }) {
                let _ = std::fs::create_dir_all(app.repo.worktree_state_dir());
                let _ = util::write_atomic(&path, &text);
            }
            out
        }
    };
    if !files.is_empty() {
        let mut args = vec!["status", "--porcelain=v1", "-z", "--no-renames", "--untracked-files=all", "--"];
        args.extend(files.iter().copied());
        if let Ok(status) = app.repo.git(&args) {
            for dirty in status.split('\0').filter_map(|l| l.get(3..)) {
                out.landed.remove(dirty);
            }
        }
    }
    out
}

/// `kontext confirm --migrate-legacy`: one `bulk` ledger line for every active entry whose only
/// verification is a `checked:` date (a stamp from before the ledger, or a hand edit), dated as
/// that date and naming the last commit that changed the entry's file, appended and staged. The
/// dates stay; they now read as what they were: stamps. Returns the lines written.
pub fn migrate_legacy(app: &App) -> anyhow::Result<Vec<Line>> {
    let _local = app.write_lock.lock().unwrap_or_else(|e| e.into_inner());
    let (entries, _) = app.entries();
    let lines = ledger::read(app);
    let newest = ledger::latest(&lines, false);
    let by = app.repo.user_name();
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| e.is_active()) {
        let Some(v) = of(e, newest.get(e.id.as_str()).copied()) else { continue };
        if v.source != "frontmatter" {
            continue;
        }
        let at = match parse_checked(&v.at) {
            Some(Checked::At(t)) => t.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            Some(Checked::Day(d)) => format!("{}T00:00:00Z", d.format("%Y-%m-%d")),
            None => continue,
        };
        let commit =
            app.repo.git_opt(&["log", "-1", "--format=%H", "--", &e.rel_path]).filter(|s| !s.is_empty()).or_else(|| app.repo.head());
        out.push(Line {
            at,
            id: e.id.clone(),
            kind: "bulk".into(),
            commit,
            by: by.clone(),
            passed: None,
            because: "migrated: a checked date with no ledger line".into(),
        });
    }
    ledger::append(app, &out)?;
    Ok(out)
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

/// Run `command` with `sh -c` in `root`, stderr folded into stdout, stdin closed, killed (with
/// everything it started) after `limit`.
pub fn run(root: &Path, command: &str, limit: std::time::Duration) -> anyhow::Result<Outcome> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("sh");
    // `exec 2>&1` merges the streams inside the shell, so their order is kept without a second pipe
    cmd.arg("-c").arg(format!("exec 2>&1\n{command}")).current_dir(root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // its own process group: a timeout kills the test runner the shell started, not only the shell
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("could not start sh: {e}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut tail: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&buf[..n]);
            if tail.len() > 4 * TAIL_BYTES {
                tail.drain(..tail.len() - TAIL_BYTES);
            }
        }
        tail
    });
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= limit {
            timed_out = true;
            #[cfg(unix)]
            {
                // `--` before the negative id: procps's kill on Linux reads `-123` as an option, BSD's does not mind
                let _ = Command::new("kill").args(["-KILL", "--", &format!("-{}", child.id())]).stderr(Stdio::null()).status();
            }
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let mut tail = reader.join().unwrap_or_default();
    if tail.len() > TAIL_BYTES {
        tail.drain(..tail.len() - TAIL_BYTES);
    }
    let mut output_tail = String::from_utf8_lossy(&tail).into_owned();
    if timed_out {
        output_tail.push_str(&format!("\n[kontext: stopped after {} s]", limit.as_secs()));
    }
    let exit_code = if timed_out { None } else { status.code() };
    Ok(Outcome { passed: !timed_out && status.success(), exit_code, timed_out, output_tail })
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
        assert_eq!((fm.kind.as_str(), fm.source, fm.commit.as_deref()), ("cache", "frontmatter", None));
        assert!(!fm.verified(), "a checked date nobody recorded is a cache, not a verification");
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
        // newest first, linear: e ← d ← c ← b ← a
        let shas = ["e", "d", "c", "b", "a"];
        let paths: [&[&str]; 5] = [
            &["src/e.rs", ".ai/confirmations.log"],
            &["docs/x.md"],
            &["src/c.rs", "src/c2.rs"],
            &["src/b.rs", ".ai/confirmations.log"],
            &["src/a.rs"],
        ];
        let raw = (0..5)
            .map(|i| {
                let parents = if i < 4 { vec![shas[i + 1].to_string()] } else { Vec::new() };
                (
                    shas[i].to_string(),
                    500 - 100 * i as i64,
                    format!("s {}", shas[i]),
                    parents,
                    paths[i].iter().map(|p| p.to_string()).collect(),
                )
            })
            .collect();
        let history = History::build(raw);
        let look = |id: &'static str, from: Start| Look { id, file: ".ai/decisions/x.md", paths: vec!["src/**", ".ai/**"], from };
        let mut landings = HashMap::new();
        landings.insert(("landed".to_string(), "t".to_string()), "b".to_string());
        let ledger =
            |id: &str, at: i64, commit: &str| Start::Ledger { at, anchors: vec![commit.to_string()], key: (id.into(), "t".into()) };
        let mut looks = vec![
            look("named", ledger("named", 150, "a")),
            look("landed", ledger("landed", 150, "a")),
            look("elsewhere", ledger("elsewhere", 350, "zzz")),
            look("cached", Start::Checked { since: 250 }),
        ];
        let counts = count(&mut looks, &history, &landings, "e", &|f: &str| f.starts_with(".ai/"));
        assert_eq!(counts["named"].commits, 3, "b, c and e after a");
        assert_eq!(counts["named"].paths, vec!["src/b.rs", "src/c.rs", "src/c2.rs", "src/e.rs"], "the ledger itself is not code");
        assert_eq!(counts["named"].recent[0], ("e".to_string(), "s e".to_string()));
        assert_eq!(counts["landed"].commits, 2, "the commit that landed the line was reviewed with it");
        assert_eq!(counts["elsewhere"].commits, 1, "a commit outside HEAD's history: by time");
        assert!(!counts["elsewhere"].truncated);
        assert_eq!(counts["cached"].commits, 2);
        assert_eq!(counts["cached"].head, "e");
    }

    #[test]
    fn a_branch_merged_after_the_look_is_counted_and_a_merge_is_not() {
        // main: s ← l (the look landed) ← m (merge of f); feature: s ← f (older than l)
        let c = |sha: &str, time: i64, parents: &[&str], paths: &[&str]| {
            (
                sha.to_string(),
                time,
                format!("s {sha}"),
                parents.iter().map(|p| p.to_string()).collect(),
                paths.iter().map(|p| p.to_string()).collect(),
            )
        };
        let mut history = History::build(vec![
            c("m", 400, &["l", "f"], &[]),
            c("l", 300, &["s"], &[".ai/confirmations.log"]),
            c("f", 200, &["s"], &["src/f.rs"]),
            c("s", 100, &[], &["src/s.rs"]),
        ]);
        let mut looks = vec![Look {
            id: "x",
            file: ".ai/decisions/x.md",
            paths: vec!["src/**"],
            from: Start::Ledger { at: 300, anchors: vec!["s".into()], key: ("x".into(), "t".into()) },
        }];
        let mut landings = HashMap::new();
        landings.insert(("x".to_string(), "t".to_string()), "l".to_string());
        let counts = count(&mut looks, &history, &landings, "m", &|f: &str| f.starts_with(".ai/"));
        assert_eq!(counts["x"].commits, 1, "f was merged after the look");
        assert_eq!(counts["x"].recent, vec![("f".to_string(), "s f".to_string())]);
        // a look older than the history read: every commit read counts, and it says so
        history.truncated = true;
        history.shallow = true;
        let mut old = vec![Look {
            id: "y",
            file: ".ai/decisions/y.md",
            paths: vec!["src/**"],
            from: Start::Ledger { at: 50, anchors: vec!["gone".into()], key: ("y".into(), "t".into()) },
        }];
        let counts = count(&mut old, &history, &HashMap::new(), "m", &|f: &str| f.starts_with(".ai/"));
        assert_eq!((counts["y"].commits, counts["y"].truncated, counts["y"].lower_bound), (2, true, true));
        assert_eq!(counts["y"].to_json()["truncated"], true);
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
        let ok = run(&dir, "echo out; echo err >&2", std::time::Duration::from_secs(10)).unwrap();
        assert!(ok.passed && ok.exit_code == Some(0) && !ok.timed_out);
        assert_eq!(ok.output_tail, "out\nerr\n");
        let bad = run(&dir, "printf '%05000d' 0; exit 3", std::time::Duration::from_secs(10)).unwrap();
        assert!(!bad.passed && bad.exit_code == Some(3));
        assert_eq!(bad.output_tail.len(), TAIL_BYTES, "the tail only");
        let started = std::time::Instant::now();
        let slow = run(&dir, "sleep 30", std::time::Duration::from_millis(300)).unwrap();
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
