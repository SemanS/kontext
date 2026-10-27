//! The verification ledger: `<store>/confirmations.log`, one tab-separated line per look at an
//! entry — when (UTC, to the second), which entry, how (`read`, `agent`, `bulk`, `verify`), the
//! commit HEAD named at that moment, who, whether a `verify` command passed, and why. Append-only and
//! union-merged, so a merge keeps both sides' lines instead of choosing one; the `checked:` date in
//! an entry's file is only a cache of it.
use crate::app::App;
use crate::repo::Repo;
use crate::util;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Write;

pub const FILE: &str = "confirmations.log";
const ATTRIBUTE: &str = "confirmations.log merge=union";
const INDEX_ATTRIBUTE: &str = "README.md merge=union";

/// How an entry was looked at. Only a person's `read` and a passed `verify` count as verified;
/// `agent` and `bulk` are kept and shown as what they are: stamps.
pub const KINDS: &[&str] = &["read", "agent", "bulk", "verify"];

/// More ids than this in one confirmation and it is a bulk stamp, whoever made it.
pub const BULK_OVER: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// `YYYY-MM-DDTHH:MM:SSZ`
    pub at: String,
    pub id: String,
    pub kind: String,
    /// Full sha of HEAD at the moment, None before the first commit.
    pub commit: Option<String>,
    pub by: Option<String>,
    /// For `verify`: whether the command passed.
    pub passed: Option<bool>,
    pub because: String,
}

/// One field of a line: no tabs or line breaks, and `-` stands for nothing.
fn field(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ").trim().to_string()
}

fn opt(s: Option<&str>) -> String {
    s.map(field).filter(|s| !s.is_empty()).unwrap_or_else(|| "-".into())
}

impl Line {
    pub fn to_text(&self) -> String {
        let passed = match self.passed {
            Some(true) => "1",
            Some(false) => "0",
            None => "-",
        };
        format!(
            "{}\t{}\t{}\t{}\t{}\t{passed}\t{}\n",
            field(&self.at),
            field(&self.id),
            field(&self.kind),
            opt(self.commit.as_deref()),
            opt(self.by.as_deref()),
            field(&self.because)
        )
    }

    /// A line as written by [`Line::to_text`]; anything else (a conflict marker, a hand edit gone
    /// wrong) is skipped rather than failing every reader.
    pub fn parse(text: &str) -> Option<Line> {
        let text = text.trim_end_matches(['\r', '\n']);
        let mut parts = text.splitn(7, '\t');
        let at = parts.next()?.trim();
        crate::store::parse_checked(at).filter(|c| matches!(c, crate::store::Checked::At(_)))?;
        let id = parts.next()?.trim();
        let kind = parts.next()?.trim();
        if id.is_empty() || !KINDS.contains(&kind) {
            return None;
        }
        let none = |s: &str| (s != "-" && !s.is_empty()).then(|| s.to_string());
        let commit = none(parts.next()?.trim());
        let by = none(parts.next()?.trim());
        let passed = match parts.next()?.trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
        let because = parts.next().unwrap_or("").trim().to_string();
        Some(Line { at: at.into(), id: id.into(), kind: kind.into(), commit, by, passed, because })
    }

    /// Whether this line is a confirmation, which the `checked:` cache in the entry mirrors. A
    /// `verify` line is seen through the entry's verification instead.
    pub fn is_confirmation(&self) -> bool {
        self.kind != "verify"
    }
}

/// The ledger's path relative to the repository root.
pub fn rel_path(app: &App) -> String {
    format!("{}/{FILE}", app.store().dir())
}

/// Every readable line, oldest first (the order they were appended in; a union merge may
/// interleave, which is why readers compare `at`).
pub fn read(app: &App) -> Vec<Line> {
    std::fs::read_to_string(app.repo.abs(&rel_path(app))).map(|t| parse_all(&t)).unwrap_or_default()
}

pub fn parse_all(text: &str) -> Vec<Line> {
    text.lines().filter_map(Line::parse).collect()
}

/// The newest line per entry id (by time; of equal times the one written later), optionally only
/// confirmations.
pub fn latest(lines: &[Line], confirmations_only: bool) -> HashMap<&str, &Line> {
    let mut out: HashMap<&str, &Line> = HashMap::new();
    for l in lines.iter().filter(|l| !confirmations_only || l.is_confirmation()) {
        match out.get(l.id.as_str()) {
            Some(prev) if prev.at > l.at => {}
            _ => {
                out.insert(&l.id, l);
            }
        }
    }
    out
}

/// Make sure `<store>/.gitattributes` union-merges the ledger (and the generated index, which init
/// has always asked for), keeping every line already there. Returns whether the file changed.
pub fn ensure_attributes(repo: &Repo, store_dir: &str) -> Result<bool> {
    let path = repo.abs(&format!("{store_dir}/.gitattributes"));
    let current = std::fs::read_to_string(&path).ok();
    let mut text = current.clone().unwrap_or_else(|| {
        format!(
            "# the generated index is re-rendered on every commit; union-merge keeps parallel branches conflict-free\n{INDEX_ATTRIBUTE}\n"
        )
    });
    let has = |t: &str| t.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ATTRIBUTE.split_whitespace().collect::<Vec<_>>());
    if !has(&text) {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("# verifications are appended, never rewritten: a merge keeps both sides' lines\n");
        text.push_str(ATTRIBUTE);
        text.push('\n');
    }
    if current.as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    util::write_atomic(&path, &text)?;
    Ok(true)
}

/// The UTC second, as ledger lines and `checked:` write it.
pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// A line for `id` looked at now, from this repository's HEAD and git user.
pub fn line(app: &App, at: &str, id: &str, kind: &str, passed: Option<bool>, because: Option<&str>) -> Line {
    Line {
        at: at.to_string(),
        id: id.to_string(),
        kind: kind.to_string(),
        commit: app.repo.head(),
        by: app.repo.user_name(),
        passed,
        because: because.map(util::one_line).unwrap_or_default(),
    }
}

/// Append lines to the ledger and stage it (with the attributes that make it merge), so the
/// verification is reviewed with the change that prompted it. Callers hold the write guard.
pub fn append(app: &App, lines: &[Line]) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let dir = app.store().dir();
    let attributes = ensure_attributes(&app.repo, &dir)?;
    let rel = rel_path(app);
    let path = app.repo.abs(&rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // a file that lost its last newline (a hand edit) must not glue the next line onto it
    let needs_newline = std::fs::read(&path).ok().is_some_and(|b| !b.is_empty() && !b.ends_with(b"\n"));
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).with_context(|| format!("open {rel}"))?;
    let mut text: String = lines.iter().map(Line::to_text).collect();
    if needs_newline {
        text.insert(0, '\n');
    }
    file.write_all(text.as_bytes()).with_context(|| format!("append to {rel}"))?;
    file.sync_all().ok();
    let mut add = vec!["add", "--", rel.as_str()];
    let ga = format!("{dir}/.gitattributes");
    if attributes || app.repo.git_opt(&["ls-files", "--error-unmatch", "--", &ga]).is_none() {
        add.push(&ga);
    }
    app.repo.git(&add)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_round_trips_and_odd_lines_are_skipped() {
        let l = Line {
            at: "2026-10-27T09:30:00Z".into(),
            id: "cache".into(),
            kind: "verify".into(),
            commit: Some("a".repeat(40)),
            by: Some("Ana\tB".into()),
            passed: Some(false),
            because: "tests\tfail\non main".into(),
        };
        let text = l.to_text();
        assert_eq!(text.matches('\t').count(), 6);
        assert!(text.ends_with("Ana B\t0\ttests fail on main\n"));
        let back = Line::parse(&text).unwrap();
        assert_eq!(back.by.as_deref(), Some("Ana B"));
        assert_eq!(back.passed, Some(false));
        assert_eq!(back.because, "tests fail on main");
        let bare = Line { commit: None, by: None, passed: None, because: String::new(), kind: "read".into(), ..l.clone() };
        assert_eq!(bare.to_text(), "2026-10-27T09:30:00Z\tcache\tread\t-\t-\t-\t\n");
        assert_eq!(Line::parse(&bare.to_text()).unwrap(), bare);
        for odd in ["<<<<<<< HEAD", "2026-10-27\tcache\tread\t-\t-\t-\t", "2026-10-27T09:30:00Z\tcache\tglance\t-\t-\t-\t", ""] {
            assert!(Line::parse(odd).is_none(), "{odd}");
        }
    }

    #[test]
    fn the_latest_line_wins_by_time_not_by_position() {
        let mk = |at: &str, id: &str, kind: &str| Line {
            at: at.into(),
            id: id.into(),
            kind: kind.into(),
            commit: None,
            by: None,
            passed: None,
            because: String::new(),
        };
        // a union merge puts the other branch's older line after ours
        let lines = vec![
            mk("2026-10-27T10:00:00Z", "a", "read"),
            mk("2026-10-20T10:00:00Z", "a", "agent"),
            mk("2026-10-28T10:00:00Z", "a", "verify"),
        ];
        assert_eq!(latest(&lines, false)["a"].kind, "verify");
        assert_eq!(latest(&lines, true)["a"].kind, "read");
    }
}
