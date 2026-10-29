//! Phase 2 — mine git history: decision-shaped commits, dependency swaps, churn, conventions.
use super::scan::Module;
use crate::repo::Repo;
use crate::signals::{DecisionWordsCfg, Words, decision_score_with, detect_language, is_bot_author};
use crate::util;
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub commits: usize,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub contributors: usize,
    pub conventional_ratio: f32,
    pub ticket_example: Option<String>,
    pub ticket_ratio: f32,
    pub candidates: Vec<Candidate>,
    pub clusters: Vec<Cluster>,
    pub file_churn: BTreeMap<String, usize>,
    pub file_last: BTreeMap<String, String>,
    /// The language detected in commit subjects when it is not English (`sk`, `cs`).
    #[serde(default)]
    pub language: Option<String>,
    /// Commits left out because a knowledge entry they name in a trailer already exists.
    #[serde(default)]
    pub cited: usize,
    /// Commits by bots, never mined for decisions.
    #[serde(default)]
    pub bot_commits: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Candidate {
    pub sha: String,
    pub short: String,
    pub date: String,
    pub author: String,
    pub subject: String,
    pub body: String,
    pub files: Vec<String>,
    pub score: f32,
    pub reasons: Vec<String>,
    pub module: Option<String>,
}

/// Fewer decision commits than this and a module's cluster joins its enclosing module's.
const MIN_CLUSTER: usize = 3;
/// Commits per decision task.
const CHUNK: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cluster {
    pub id: String,
    pub module: Option<String>,
    pub title: String,
    pub commits: Vec<String>,
    pub score: f32,
}

/// What `mine` needs besides the repository.
#[derive(Debug, Clone, Default)]
pub struct MineOpts {
    pub max_commits: usize,
    /// Commits that only touch these (ADRs, `.ai/`) are knowledge already, not candidates.
    pub knowledge_dirs: Vec<String>,
    /// Ids of the entries in the store: a commit whose `Decision:` (… `Incident:`) trailer names
    /// one of them is recorded knowledge already.
    pub known_ids: HashSet<String>,
    /// Trailer keys that link a commit to an entry (`Decision`, `Convention`, `Learning`, `Incident`, …).
    pub trailer_keys: Vec<String>,
    /// `[init] bot_authors`, on top of the built-in bots.
    pub bot_authors: Vec<String>,
    /// `[init] decision_words`, on top of English and the detected language.
    pub decision_words: DecisionWordsCfg,
}

const MANIFEST_KEYS_STOP: &[&str] = &[
    "name",
    "version",
    "description",
    "main",
    "module",
    "types",
    "typings",
    "private",
    "license",
    "license-file",
    "author",
    "type",
    "engines",
    "packageManager",
    "edition",
    "authors",
    "repository",
    "readme",
    "homepage",
    "documentation",
    "keywords",
    "categories",
    "publish",
    "build",
    "rust-version",
    "resolver",
    "members",
    "default-members",
    "exclude",
    "include",
    "path",
    "features",
    "optional",
    "default-features",
    "workspace",
    "requires-python",
    "python",
    "node",
    "npm",
    "bin",
    "files",
    "scripts",
    "sideEffects",
    "exports",
    "imports",
    "browserslist",
    "lto",
    "panic",
    "strip",
    "opt-level",
    "debug",
    "codegen-units",
];

static TICKET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z][A-Z0-9]{1,9}-\d{1,6}\b").unwrap());
static CONVENTIONAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z]+(\([^)]*\))?!?: ").unwrap());
static TRAILER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Za-z][\w-]*):\s*(\S+)\s*$").unwrap());

/// Entry ids a commit message names in knowledge trailers (`Decision: <id>` and the like).
pub fn trailer_ids(message: &str, keys: &[String]) -> Vec<String> {
    message
        .lines()
        .filter_map(|l| TRAILER.captures(l.trim()))
        .filter(|c| keys.iter().any(|k| k.eq_ignore_ascii_case(&c[1])))
        .map(|c| c[2].to_string())
        .collect()
}

/// The default trailer keys, plus the ones the config sets per kind.
pub fn default_trailer_keys() -> Vec<String> {
    ["Decision", "Convention", "Learning", "Incident"].iter().map(|s| s.to_string()).collect()
}

/// A path that says the commit changes structure, whatever language its message is in: a
/// migration or a dependency manifest.
fn structural_path(f: &str) -> bool {
    let name = f.rsplit('/').next().unwrap_or(f);
    f.contains("migrations/")
        || f.contains("migrate/")
        || matches!(name, "package.json" | "Cargo.toml" | "pyproject.toml" | "go.mod" | "pom.xml" | "build.gradle" | "Gemfile")
        || name.starts_with("build.gradle")
}

pub fn mine(repo: &Repo, modules: &[Module], opts: &MineOpts) -> Result<History> {
    let mut h = History::default();
    if repo.head().is_none() {
        return Ok(h);
    }
    let max_commits = opts.max_commits.max(1);
    let out = repo.git(&[
        "log",
        &format!("-n{max_commits}"),
        "--no-merges",
        "--date=short",
        "--name-only",
        "--format=%x1e%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1f%b%x1f",
    ])?;
    let mut authors = BTreeSet::new();
    let mut conventional = 0usize;
    let mut tickets = 0usize;
    struct Raw {
        sha: String,
        short: String,
        author: String,
        date: String,
        subject: String,
        body: String,
        files: Vec<String>,
        bot: bool,
        cited: bool,
    }
    let mut raws: Vec<Raw> = Vec::new();
    for rec in out.split('\x1e') {
        let p: Vec<&str> = rec.splitn(7, '\x1f').collect();
        if p.len() < 7 {
            continue;
        }
        let files: Vec<String> = p[6].lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
        let subject = p[4].to_string();
        // a trailer naming an entry the store has: its knowledge is recorded already
        let cited = trailer_ids(p[5], &opts.trailer_keys).iter().any(|id| opts.known_ids.contains(id));
        // attribution trailers say nothing about the change
        let body = p[5]
            .lines()
            .filter(|l| {
                let l = l.trim().to_lowercase();
                !(l.starts_with("co-authored-by:") || l.starts_with("signed-off-by:") || l.starts_with("🤖 generated with"))
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        h.commits += 1;
        authors.insert(p[2].to_string());
        if CONVENTIONAL.is_match(&subject) {
            conventional += 1;
        }
        if let Some(m) = TICKET.find(&subject) {
            tickets += 1;
            if h.ticket_example.is_none() {
                h.ticket_example = Some(m.as_str().to_string());
            }
        }
        if h.last_date.is_none() {
            h.last_date = Some(p[3].to_string());
        }
        h.first_date = Some(p[3].to_string());
        for f in &files {
            *h.file_churn.entry(f.clone()).or_default() += 1;
            h.file_last.entry(f.clone()).or_insert_with(|| p[3].to_string());
        }
        raws.push(Raw {
            sha: p[0].trim().to_string(),
            short: p[1].to_string(),
            bot: is_bot_author(p[2], &opts.bot_authors),
            author: p[2].to_string(),
            date: p[3].to_string(),
            subject,
            body,
            files,
            cited,
        });
    }
    // decision words follow the language people write their subjects in
    let lang = detect_language(raws.iter().filter(|r| !r.bot).map(|r| r.subject.as_str()));
    h.language = lang.map(str::to_string);
    let words = Words::new(lang, &opts.decision_words);
    let mut all: Vec<Candidate> = Vec::new();
    let mut excluded: HashSet<String> = HashSet::new();
    for r in raws {
        let mut sig = decision_score_with(&r.subject, &r.body, &words);
        if r.files.iter().any(|f| structural_path(f)) {
            sig.score += 0.5;
            sig.reasons.push("touches manifests or migrations".into());
        }
        if TICKET.is_match(&r.subject) && r.body.chars().count() > 300 {
            sig.score += 0.5;
            sig.reasons.push("ticket with a reasoned body".into());
        }
        if r.bot {
            h.bot_commits += 1;
            excluded.insert(r.short.clone());
        } else if r.cited {
            h.cited += 1;
            excluded.insert(r.short.clone());
        }
        all.push(Candidate {
            sha: r.sha,
            short: r.short,
            date: r.date,
            author: r.author,
            subject: r.subject,
            body: util::truncate_chars(&r.body, 1500),
            files: r.files.into_iter().take(40).collect(),
            score: sig.score,
            reasons: sig.reasons,
            module: None,
        });
    }
    // the first commit of a repository often states its architecture
    if h.commits < max_commits
        && let Some(root) = all.last_mut()
        && root.body.len() >= 200
    {
        root.score += 3.0;
        root.reasons.push("founding commit".into());
    }
    h.contributors = authors.len();
    if h.commits > 0 {
        h.conventional_ratio = conventional as f32 / h.commits as f32;
        h.ticket_ratio = tickets as f32 / h.commits as f32;
    }

    // dependency swaps from manifest diffs, over the same window as the scoring
    let swaps = dependency_changes(repo, max_commits);
    for c in all.iter_mut() {
        if let Some((added, removed)) = swaps.get(&c.short) {
            if !added.is_empty() && !removed.is_empty() {
                c.score += 4.0;
                c.reasons.push(format!("replaced {} with {}", removed.join(", "), added.join(", ")));
            } else if !added.is_empty() && added.len() <= 3 {
                c.score += 1.5;
                c.reasons.push(format!("adopted {}", added.join(", ")));
            } else if !removed.is_empty() && removed.len() <= 3 {
                c.score += 1.5;
                c.reasons.push(format!("dropped {}", removed.join(", ")));
            }
        }
    }

    // module attribution: the module most of the files belong to; a tie goes to the first path
    let mut mod_paths: Vec<&Module> = modules.iter().collect();
    mod_paths.sort_by(|a, b| b.path.len().cmp(&a.path.len()).then_with(|| a.path.cmp(&b.path)));
    let owner = |f: &str| mod_paths.iter().find(|m| f.starts_with(&format!("{}/", m.path))).map(|m| m.path.clone());
    for c in all.iter_mut() {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for f in &c.files {
            if let Some(o) = owner(f) {
                *counts.entry(o).or_default() += 1;
            }
        }
        c.module = pick_module(counts);
    }

    let order: Vec<String> = all.iter().map(|c| c.short.clone()).collect();
    let threshold = 3.5;
    let is_knowledge = |f: &String| opts.knowledge_dirs.iter().any(|d| f.starts_with(&format!("{}/", d.trim_end_matches('/'))));
    let mut pool: Vec<Candidate> = all
        .into_iter()
        .filter(|c| !excluded.contains(&c.short))
        .filter(|c| c.files.is_empty() || !c.files.iter().all(is_knowledge))
        .collect();
    // stable: equal scores keep history order (newest first)
    pool.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    // history written in rules rather than in "we decided" words scores lower across the board:
    // when few commits pass, the next best (still clearly decision-shaped) fill up to a quarter
    let strong = pool.iter().filter(|c| c.score >= threshold).count();
    let target = (h.commits / 4).clamp(12, 200);
    let keep =
        if strong >= target { strong } else { strong + pool[strong..].iter().take(target - strong).filter(|c| c.score >= 2.5).count() };
    pool.truncate(keep.min(400));
    let cands = pool;

    h.clusters = cluster(&cands, modules, &order);
    h.candidates = cands;
    Ok(h)
}

/// The module with the most files; a tie goes to the path that sorts first.
fn pick_module(counts: BTreeMap<String, usize>) -> Option<String> {
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(m, _)| m)
}

/// Group candidates by module, oldest first, in parts of up to CHUNK commits. A part is named by
/// the module and the oldest commit it starts from, so ids stay put while newer history arrives.
fn cluster(cands: &[Candidate], modules: &[Module], order: &[String]) -> Vec<Cluster> {
    let mut by_module: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
    for c in cands {
        by_module.entry(c.module.clone().unwrap_or_else(|| "(repository)".into())).or_default().push(c);
    }
    // a module with one or two decision commits joins the nearest enclosing module: the commits of
    // apps/api/billing read better next to the rest of apps/api than as a task of their own
    let module_paths: HashSet<&str> = modules.iter().map(|m| m.path.as_str()).collect();
    let mut keys: Vec<String> = by_module.keys().cloned().collect();
    keys.sort_by(|a, b| b.matches('/').count().cmp(&a.matches('/').count()).then_with(|| a.cmp(b)));
    for k in keys {
        if by_module.get(&k).is_none_or(|v| v.len() >= MIN_CLUSTER) || k == "(repository)" {
            continue;
        }
        let parent = k.match_indices('/').rev().map(|(i, _)| &k[..i]).find(|a| module_paths.contains(a)).map(str::to_string);
        if let Some(p) = parent {
            let moved = by_module.remove(&k).unwrap_or_default();
            by_module.entry(p).or_default().extend(moved);
        }
    }
    let position: HashMap<&str, usize> = order.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();
    let mut clusters: Vec<Cluster> = Vec::new();
    for (m, mut list) in by_module {
        // oldest first; `order` is newest first
        list.sort_by(|a, b| {
            let (pa, pb) = (position.get(a.short.as_str()).copied().unwrap_or(0), position.get(b.short.as_str()).copied().unwrap_or(0));
            pb.cmp(&pa).then_with(|| a.sha.cmp(&b.sha))
        });
        let module = if m == "(repository)" { None } else { Some(m.clone()) };
        let base = format!("dec:{}", if module.is_some() { util::slugify(&m, 60) } else { "repository".into() });
        let parts = list.len().div_ceil(CHUNK);
        for (i, part) in list.chunks(CHUNK).enumerate() {
            let first = part[0].sha.chars().take(7).collect::<String>();
            let title = if parts > 1 { format!("decisions in {m} (part {} of {parts})", i + 1) } else { format!("decisions in {m}") };
            clusters.push(Cluster {
                id: format!("{base}-{first}"),
                title,
                module: module.clone(),
                commits: part.iter().map(|c| c.short.clone()).collect(),
                score: part.iter().map(|c| c.score).sum(),
            });
        }
    }
    clusters.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.id.cmp(&b.id)));
    clusters
}

/// Per commit: dependencies added / removed in any manifest (package.json, Cargo.toml,
/// pyproject.toml, go.mod, pom.xml, Gemfile, requirements*.txt).
fn dependency_changes(repo: &Repo, max: usize) -> HashMap<String, (Vec<String>, Vec<String>)> {
    let mut map: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    let out = repo
        .git(&[
            "log",
            &format!("-n{max}"),
            "--no-merges",
            "-p",
            // the whole manifest as context: a dependency is known by the section it sits in
            "--unified=100000",
            "--no-ext-diff",
            "--format=%x1e%h",
            "--",
            "package.json",
            "*/package.json",
            "Cargo.toml",
            "*/Cargo.toml",
            "pyproject.toml",
            "*/pyproject.toml",
            "go.mod",
            "*/go.mod",
            "pom.xml",
            "*/pom.xml",
            "Gemfile",
            "*/Gemfile",
            "requirements*.txt",
            "*/requirements*.txt",
        ])
        .unwrap_or_default();
    for rec in out.split('\x1e') {
        let Some((sha, diff)) = rec.split_once('\n') else { continue };
        let sha = sha.trim();
        if sha.is_empty() {
            continue;
        }
        let (added, removed) = manifest_changes(diff);
        if !added.is_empty() || !removed.is_empty() {
            map.insert(sha.to_string(), (added, removed));
        }
    }
    map
}

#[derive(Clone, Copy, PartialEq)]
enum ManifestKind {
    Npm,
    Toml,
    GoMod,
    Pom,
    Gemfile,
    Requirements,
    Other,
}

fn manifest_kind(path: &str) -> ManifestKind {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "package.json" => ManifestKind::Npm,
        "Cargo.toml" | "pyproject.toml" => ManifestKind::Toml,
        "go.mod" => ManifestKind::GoMod,
        "pom.xml" => ManifestKind::Pom,
        "Gemfile" => ManifestKind::Gemfile,
        n if n.starts_with("requirements") && n.ends_with(".txt") => ManifestKind::Requirements,
        _ => ManifestKind::Other,
    }
}

/// Where a line sits in a manifest, tracked separately for the old and the new side of a diff.
#[derive(Default, Clone)]
struct ManifestState {
    section: String,
    in_array: bool,
    in_block: bool,
}

static NPM_DEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\s*"(@?[A-Za-z0-9][\w./-]*)"\s*:\s*""#).unwrap());
static NPM_SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*"(?:dependencies|devDependencies|peerDependencies|optionalDependencies)"\s*:\s*\{"#).unwrap());
static TOML_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\s*"?([A-Za-z0-9][\w.-]*)"?\s*=\s*(.*)$"#).unwrap());
static QUOTED_REQ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"["']\s*([A-Za-z0-9][A-Za-z0-9._-]*)"#).unwrap());
static GO_REQ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:require\s+)?([^\s()]+\.[^\s()]+)\s+v\d").unwrap());
static POM_ARTIFACT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<artifactId>\s*([^<\s]+)\s*</artifactId>").unwrap());
static GEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\s*gem\s+['"]([^'"]+)['"]"#).unwrap());
static REQ_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9][A-Za-z0-9._-]*)\s*(?:\[[^\]]*\])?\s*(?:[<>=!~;@]|$)").unwrap());

fn toml_dep_section(section: &str) -> bool {
    section == "dependencies"
        || section == "dev-dependencies"
        || section == "build-dependencies"
        || section.ends_with(".dependencies")
        || section.ends_with(".dev-dependencies")
        || section.ends_with(".build-dependencies")
}

/// TOML sections whose array values list PEP 508 requirements.
fn toml_req_array(section: &str, key: &str) -> bool {
    (section == "project" && key == "dependencies") || section == "project.optional-dependencies" || section == "dependency-groups"
}

/// Feed one manifest line (without its diff sign) to the state; returns the dependency it names, if any.
fn manifest_line(kind: ManifestKind, st: &mut ManifestState, line: &str) -> Vec<String> {
    let t = line.trim();
    match kind {
        ManifestKind::Npm => {
            if NPM_SECTION.is_match(line) {
                st.in_block = true;
                return Vec::new();
            }
            if st.in_block && t.starts_with('}') {
                st.in_block = false;
                return Vec::new();
            }
            if st.in_block
                && let Some(c) = NPM_DEP.captures(line)
            {
                return vec![c[1].to_string()];
            }
            Vec::new()
        }
        ManifestKind::Toml => {
            if st.in_array {
                if t.starts_with(']') || (t.contains(']') && !t.contains('[')) {
                    st.in_array = false;
                }
                return QUOTED_REQ.captures_iter(t).map(|c| c[1].to_string()).collect();
            }
            if t.starts_with('[') {
                let sec = t.trim_start_matches('[').split(']').next().unwrap_or("").trim().replace(['"', '\''], "");
                st.section = sec.clone();
                // `[dependencies.serde]` declares a dependency in its header
                if let Some(i) = sec.rfind("dependencies.") {
                    let name = &sec[i + "dependencies.".len()..];
                    if !name.is_empty() && !name.contains('.') {
                        return vec![name.to_string()];
                    }
                }
                return Vec::new();
            }
            let Some(c) = TOML_KEY.captures(t) else { return Vec::new() };
            let (key, value) = (c[1].to_string(), c[2].trim().to_string());
            if toml_req_array(&st.section, &key) && value.starts_with('[') {
                if !value.contains(']') {
                    st.in_array = true;
                }
                return QUOTED_REQ.captures_iter(&value).map(|c| c[1].to_string()).collect();
            }
            if toml_dep_section(&st.section) && (value.starts_with('{') || value.starts_with('"') || value.starts_with('\'')) {
                return vec![key];
            }
            Vec::new()
        }
        ManifestKind::GoMod => {
            if t.starts_with("require (") || t == "require(" {
                st.in_block = true;
                return Vec::new();
            }
            if st.in_block && t.starts_with(')') {
                st.in_block = false;
                return Vec::new();
            }
            if t.contains("// indirect") || !(st.in_block || t.starts_with("require ")) {
                return Vec::new();
            }
            GO_REQ.captures(t).map(|c| vec![c[1].to_string()]).unwrap_or_default()
        }
        ManifestKind::Pom => {
            let mut out = Vec::new();
            if t.contains("<dependency>") {
                st.in_block = true;
            }
            if st.in_block
                && let Some(c) = POM_ARTIFACT.captures(t)
            {
                out.push(c[1].to_string());
            }
            if t.contains("</dependency>") {
                st.in_block = false;
            }
            out
        }
        ManifestKind::Gemfile => GEM.captures(t).map(|c| vec![c[1].to_string()]).unwrap_or_default(),
        ManifestKind::Requirements => {
            if t.starts_with('#') || t.starts_with('-') {
                return Vec::new();
            }
            REQ_LINE.captures(t).map(|c| vec![c[1].to_string()]).unwrap_or_default()
        }
        ManifestKind::Other => Vec::new(),
    }
}

/// Dependencies added and removed in one commit's manifest diff (full-context `git log -p`).
fn manifest_changes(diff: &str) -> (Vec<String>, Vec<String>) {
    let mut added: BTreeSet<String> = BTreeSet::new();
    let mut removed: BTreeSet<String> = BTreeSet::new();
    let mut kind = ManifestKind::Other;
    let (mut old, mut new) = (ManifestState::default(), ManifestState::default());
    let mut in_hunk = false;
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("diff --git ") {
            kind = manifest_kind(rest.rsplit(' ').next().unwrap_or(rest));
            (old, new) = (ManifestState::default(), ManifestState::default());
            in_hunk = false;
            continue;
        }
        if l.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk || kind == ManifestKind::Other {
            continue;
        }
        let (sign, text) = match l.chars().next() {
            Some(c @ ('+' | '-' | ' ')) => (c, &l[1..]),
            _ => continue,
        };
        let record = |st: &mut ManifestState, set: &mut BTreeSet<String>| {
            for name in manifest_line(kind, st, text) {
                if !MANIFEST_KEYS_STOP.contains(&name.as_str()) && !name.starts_with("@types/") {
                    set.insert(name);
                }
            }
        };
        match sign {
            '+' => record(&mut new, &mut added),
            '-' => record(&mut old, &mut removed),
            _ => {
                let mut sink = BTreeSet::new();
                record(&mut old, &mut sink);
                record(&mut new, &mut sink);
            }
        }
    }
    // a version bump shows up as both - and +: not a change of dependency
    let both: Vec<String> = added.intersection(&removed).cloned().collect();
    for b in both {
        added.remove(&b);
        removed.remove(&b);
    }
    (added.into_iter().collect(), removed.into_iter().collect())
}

pub fn apply_churn(modules: &mut [Module], h: &History) {
    for m in modules.iter_mut() {
        let prefix = format!("{}/", m.path);
        let mut churn = 0;
        let mut last: Option<String> = None;
        for (f, n) in h.file_churn.range(prefix.clone()..) {
            if !f.starts_with(&prefix) {
                break;
            }
            churn += n;
            if let Some(d) = h.file_last.get(f)
                && last.as_ref().is_none_or(|l| d > l)
            {
                last = Some(d.clone());
            }
        }
        m.churn = churn;
        m.last_change = last;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(path: &str, body: &str) -> String {
        format!("diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,9 +1,9 @@\n{body}")
    }

    #[test]
    fn cargo_profile_keys_are_not_dependencies() {
        let d = diff(
            "Cargo.toml",
            " [package]\n name = \"x\"\n+license-file = \"LICENSE\"\n [dependencies]\n-moment = \"1\"\n+dayjs = { version = \"1\" }\n [profile.release]\n+lto = \"fat\"\n+panic = \"abort\"\n+strip = \"symbols\"\n",
        );
        let (a, r) = manifest_changes(&d);
        assert_eq!(a, vec!["dayjs"]);
        assert_eq!(r, vec!["moment"]);
        // a version bump is no change; a `[dependencies.x]` table is a dependency
        let d = diff("Cargo.toml", " [dependencies]\n-serde = \"1.0\"\n+serde = \"1.1\"\n+[dependencies.tokio]\n+version = \"1\"\n");
        assert_eq!(manifest_changes(&d), (vec!["tokio".to_string()], vec![]));
    }

    #[test]
    fn pep621_go_pom_gem_and_requirements() {
        let d = diff("pyproject.toml", " [project]\n name = \"x\"\n dependencies = [\n-  \"requests>=2\",\n+  \"httpx>=0.27\",\n ]\n");
        assert_eq!(manifest_changes(&d), (vec!["httpx".to_string()], vec!["requests".to_string()]));
        let d = diff(
            "go.mod",
            " module x\n require (\n-\tgithub.com/pkg/errors v0.9.1\n+\tgo.uber.org/zap v1.27.0\n+\tgolang.org/x/sys v0.1.0 // indirect\n )\n",
        );
        assert_eq!(manifest_changes(&d), (vec!["go.uber.org/zap".to_string()], vec!["github.com/pkg/errors".to_string()]));
        let d = diff(
            "pom.xml",
            " <artifactId>app</artifactId>\n <dependencies>\n <dependency>\n-<artifactId>log4j</artifactId>\n+<artifactId>logback-classic</artifactId>\n </dependency>\n",
        );
        assert_eq!(manifest_changes(&d), (vec!["logback-classic".to_string()], vec!["log4j".to_string()]));
        let d = diff("Gemfile", " source 'https://rubygems.org'\n-gem 'paperclip'\n+gem \"activestorage\", \"~> 7\"\n");
        assert_eq!(manifest_changes(&d), (vec!["activestorage".to_string()], vec!["paperclip".to_string()]));
        let d = diff("requirements-dev.txt", " # tools\n-nose==1.3\n+pytest>=8\n -r base.txt\n");
        assert_eq!(manifest_changes(&d), (vec!["pytest".to_string()], vec!["nose".to_string()]));
    }

    #[test]
    fn npm_dependencies_only_inside_their_sections() {
        let d = diff(
            "package.json",
            " {\n   \"name\": \"x\",\n   \"scripts\": {\n+    \"lint\": \"eslint .\"\n   },\n   \"dependencies\": {\n-    \"moment\": \"^2\",\n+    \"dayjs\": \"^1\"\n   }\n }\n",
        );
        assert_eq!(manifest_changes(&d), (vec!["dayjs".to_string()], vec!["moment".to_string()]));
    }

    #[test]
    fn trailers_name_entries() {
        let msg = "feat: x\n\nWhy.\n\nDecision: 2026-01-01-use-postgres\nLearning: 2026-01-02-cache\nCo-Authored-By: A <a@b>\n";
        assert_eq!(trailer_ids(msg, &default_trailer_keys()), vec!["2026-01-01-use-postgres", "2026-01-02-cache"]);
    }

    #[test]
    fn module_ties_break_by_path() {
        let mut c = BTreeMap::new();
        c.insert("src/b".to_string(), 2);
        c.insert("src/a".to_string(), 2);
        c.insert("src/c".to_string(), 1);
        assert_eq!(pick_module(c), Some("src/a".to_string()));
    }

    fn cand(sha: &str, module: &str) -> Candidate {
        Candidate { sha: sha.repeat(8), short: sha.repeat(2), module: Some(module.into()), score: 4.0, ..Default::default() }
    }

    #[test]
    fn cluster_ids_name_the_oldest_commit_and_survive_new_history() {
        let mods = vec![Module { path: "src".into(), ..Default::default() }];
        // newest first, as `git log` lists them
        let older: Vec<Candidate> = ["e", "d", "c", "b", "a"].iter().map(|s| cand(s, "src")).collect();
        let order: Vec<String> = older.iter().map(|c| c.short.clone()).collect();
        let first = cluster(&older, &mods, &order);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "dec:src-aaaaaaa");
        assert_eq!(first[0].commits[0], "aa");
        let mut newer: Vec<Candidate> = ["k", "j", "i", "h", "g", "f"].iter().map(|s| cand(s, "src")).collect();
        newer.extend(older);
        let order: Vec<String> = newer.iter().map(|c| c.short.clone()).collect();
        let second = cluster(&newer, &mods, &order);
        assert!(second.iter().any(|c| c.id == "dec:src-aaaaaaa" && c.commits.len() == 8), "{second:?}");
        assert!(second.iter().any(|c| c.id == "dec:src-iiiiiii"), "{second:?}");
        assert_eq!(second, cluster(&newer, &mods, &order));
    }
}
