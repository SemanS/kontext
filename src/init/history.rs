//! Phase 2 — mine git history: decision-shaped commits, dependency swaps, churn, conventions.
use super::scan::Module;
use crate::repo::Repo;
use crate::signals::decision_score;
use crate::util;
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub module: Option<String>,
    pub title: String,
    pub commits: Vec<String>,
    pub score: f32,
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
    "node",
    "npm",
    "bin",
    "files",
    "scripts",
    "sideEffects",
    "exports",
    "imports",
    "browserslist",
];

static DEP_LINE_NPM: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"^([+-])\s+"(@?[A-Za-z0-9][\w./-]*)":\s*"(?:[~^<>=]|workspace:|npm:|\d)"#).unwrap());
static DEP_LINE_TOML: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"^([+-])\s*([A-Za-z0-9][\w-]*)\s*=\s*(?:\{|")"#).unwrap());
static TICKET: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| Regex::new(r"\b[A-Z][A-Z0-9]{1,9}-\d{1,6}\b").unwrap());
static CONVENTIONAL: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| Regex::new(r"^[a-z]+(\([^)]*\))?!?: ").unwrap());

/// `knowledge_dirs`: commits that only touch these (ADRs, `.ai/`) are knowledge already, not candidates.
pub fn mine(repo: &Repo, modules: &[Module], max_commits: usize, knowledge_dirs: &[String]) -> Result<History> {
    let mut h = History::default();
    if repo.head().is_none() {
        return Ok(h);
    }
    let out = repo.git(&[
        "log",
        &format!("-n{}", max_commits.max(1)),
        "--no-merges",
        "--date=short",
        "--name-only",
        "--format=%x1e%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1f%b%x1f",
    ])?;
    let mut authors = BTreeSet::new();
    let mut conventional = 0usize;
    let mut tickets = 0usize;
    let mut all: Vec<Candidate> = Vec::new();
    for rec in out.split('\x1e') {
        let p: Vec<&str> = rec.splitn(7, '\x1f').collect();
        if p.len() < 7 {
            continue;
        }
        let files: Vec<String> = p[6].lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
        let (subject, body) = (p[4].to_string(), p[5].trim().to_string());
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
        let sig = decision_score(&subject, &body);
        all.push(Candidate {
            sha: p[0].trim().to_string(),
            short: p[1].to_string(),
            date: p[3].to_string(),
            author: p[2].to_string(),
            subject,
            body: util::truncate_chars(&body, 1500),
            files: files.into_iter().take(40).collect(),
            score: sig.score,
            reasons: sig.reasons,
            module: None,
        });
    }
    h.contributors = authors.len();
    if h.commits > 0 {
        h.conventional_ratio = conventional as f32 / h.commits as f32;
        h.ticket_ratio = tickets as f32 / h.commits as f32;
    }

    // dependency swaps from manifest diffs
    let swaps = dependency_changes(repo, 500);
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

    // module attribution
    let mut mod_paths: Vec<&Module> = modules.iter().collect();
    mod_paths.sort_by_key(|x| std::cmp::Reverse(x.path.len()));
    let owner = |f: &str| mod_paths.iter().find(|m| f.starts_with(&format!("{}/", m.path))).map(|m| m.path.clone());
    for c in all.iter_mut() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for f in &c.files {
            if let Some(o) = owner(f) {
                *counts.entry(o).or_default() += 1;
            }
        }
        c.module = counts.into_iter().max_by_key(|(_, n)| *n).map(|(m, _)| m);
    }

    let threshold = 3.5;
    let is_knowledge = |f: &String| knowledge_dirs.iter().any(|d| f.starts_with(&format!("{}/", d.trim_end_matches('/'))));
    let mut cands: Vec<Candidate> =
        all.into_iter().filter(|c| c.score >= threshold).filter(|c| c.files.is_empty() || !c.files.iter().all(is_knowledge)).collect();
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate(400);

    let mut by_module: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
    for c in &cands {
        by_module.entry(c.module.clone().unwrap_or_else(|| "(repository)".into())).or_default().push(c);
    }
    let mut clusters: Vec<Cluster> = by_module
        .into_iter()
        .map(|(m, list)| {
            let top: Vec<&&Candidate> = list.iter().take(10).collect();
            let score: f32 = top.iter().map(|c| c.score).sum();
            let module = if m == "(repository)" { None } else { Some(m.clone()) };
            Cluster {
                id: format!("dec:{}", if module.is_some() { util::slugify(&m, 60) } else { "repository".into() }),
                title: format!("decisions in {m}"),
                module,
                commits: top.iter().map(|c| c.short.clone()).collect(),
                score,
            }
        })
        .collect();
    clusters.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    h.candidates = cands;
    h.clusters = clusters;
    Ok(h)
}

/// Per commit: dependencies added / removed in any manifest (package.json, Cargo.toml, pyproject.toml).
fn dependency_changes(repo: &Repo, max: usize) -> HashMap<String, (Vec<String>, Vec<String>)> {
    let mut map: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    let out = repo
        .git(&[
            "log",
            &format!("-n{max}"),
            "--no-merges",
            "-p",
            "--unified=0",
            "--format=%x1e%h",
            "--",
            "package.json",
            "*/package.json",
            "Cargo.toml",
            "*/Cargo.toml",
            "pyproject.toml",
            "*/pyproject.toml",
        ])
        .unwrap_or_default();
    for rec in out.split('\x1e') {
        let mut lines = rec.lines();
        let Some(sha) = lines.next().map(str::trim).filter(|s| !s.is_empty()) else { continue };
        let mut added: BTreeSet<String> = BTreeSet::new();
        let mut removed: BTreeSet<String> = BTreeSet::new();
        let mut toml_file = false;
        for l in lines {
            if l.starts_with("diff --git") {
                toml_file = l.ends_with(".toml");
                continue;
            }
            if l.starts_with("+++") || l.starts_with("---") {
                continue;
            }
            let caps = if toml_file { DEP_LINE_TOML.captures(l) } else { DEP_LINE_NPM.captures(l) };
            if let Some(c) = caps {
                let name = c[2].to_string();
                if MANIFEST_KEYS_STOP.contains(&name.as_str()) || name.starts_with("@types/") {
                    continue;
                }
                if &c[1] == "+" {
                    added.insert(name);
                } else {
                    removed.insert(name);
                }
            }
        }
        // a version bump shows up as both - and +: not a change of dependency
        let both: Vec<String> = added.intersection(&removed).cloned().collect();
        for b in both {
            added.remove(&b);
            removed.remove(&b);
        }
        if !added.is_empty() || !removed.is_empty() {
            map.insert(sha.to_string(), (added.into_iter().collect(), removed.into_iter().collect()));
        }
    }
    map
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
