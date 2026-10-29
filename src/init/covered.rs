//! `<store>/init-covered.log`: the commits `kontext init` decision tasks have covered (distilled or
//! skipped), one per line. It is committed and union-merged like the verification ledger, so a
//! teammate who pulls, or a fresh clone, sees the same `dec:` tasks as done.
use crate::app::App;
use crate::repo::Repo;
use crate::util;
use anyhow::Result;
use std::collections::HashSet;

/// File name inside the store directory.
pub const COVERED_FILE: &str = "init-covered.log";
const ATTRIBUTE: &str = "init-covered.log merge=union";
const HEADER: &str = "# commits covered by `kontext init` decision tasks: <sha> <date> <task> — appended, union-merged";

fn rel(app: &App) -> String {
    format!("{}/{COVERED_FILE}", app.cfg().store.dir.trim_end_matches('/'))
}

/// Full shas listed in the log.
pub fn load(app: &App) -> HashSet<String> {
    parse(&std::fs::read_to_string(app.repo.abs(&rel(app))).unwrap_or_default())
}

fn parse(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_lowercase)
        .collect()
}

/// Append the shas `task` covered (those not listed yet). Returns how many were added.
pub fn append(app: &App, shas: &[String], task: &str) -> Result<usize> {
    let path = app.repo.abs(&rel(app));
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let known = parse(&current);
    let mut text = if current.trim().is_empty() { format!("{HEADER}\n") } else { current.clone() };
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let mut added = 0;
    let mut seen: HashSet<String> = HashSet::new();
    for s in shas {
        let s = s.trim().to_lowercase();
        if s.is_empty() || known.contains(&s) || !seen.insert(s.clone()) {
            continue;
        }
        text.push_str(&format!("{s} {} {task}\n", util::today()));
        added += 1;
    }
    if added > 0 {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        util::write_atomic(&path, &text)?;
        ensure_attribute(&app.repo, app.cfg().store.dir.trim_end_matches('/'))?;
    }
    Ok(added)
}

/// Add `init-covered.log merge=union` to `<store>/.gitattributes` when it is missing.
pub fn ensure_attribute(repo: &Repo, store_dir: &str) -> Result<bool> {
    let path = repo.abs(&format!("{store_dir}/.gitattributes"));
    let current = std::fs::read_to_string(&path).ok();
    let mut text = current.clone().unwrap_or_default();
    let want: Vec<&str> = ATTRIBUTE.split_whitespace().collect();
    if text.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == want) {
        return Ok(false);
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# commits init decision tasks covered are appended, never rewritten: a merge keeps both sides' lines\n");
    text.push_str(ATTRIBUTE);
    text.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    util::write_atomic(&path, &text)?;
    Ok(true)
}

/// Whether `sha` (full or abbreviated, at least 7 characters) is in `covered`, which holds full
/// shas or 7-character prefixes.
pub fn contains(covered: &HashSet<String>, sha: &str) -> bool {
    let s = sha.to_lowercase();
    covered.contains(&s) || s.get(..7).is_some_and(|p| covered.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shas_and_ignores_comments() {
        let t = format!("{HEADER}\nABCDEF1234567890 2026-10-29 dec:src-abcdef1\n\nnot-a-sha x\n1234567 2026-10-29 dec:x\n");
        let s = parse(&t);
        assert!(s.contains("abcdef1234567890") && s.contains("1234567"));
        assert_eq!(s.len(), 2);
        let mut c: HashSet<String> = HashSet::new();
        c.insert("abcdef1".into());
        assert!(contains(&c, "ABCDEF1234567890"));
        assert!(!contains(&c, "abcdef2234567890"));
    }
}
