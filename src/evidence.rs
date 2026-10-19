//! Where an entry comes from. Evidence starts without a schema change: references live in the
//! entry's `external_refs` list, a short quote with its verdict in a `## Evidence` section of the
//! body, where the reviewer of the pull request reads it. The reference shapes:
//!
//! * `pr:<id>`, `issue:<id>`, `task:<id>` — a pull request, an issue, a task (`task:krk-335`);
//! * `code:<repo>@<rev>:<path>[#L<a>[-<b>]]` — lines of a file at a revision, which does not move;
//! * `url:<http(s) URL>` — anything else that can be opened.
//!
//! A quote of a `code:` reference into this repository is checked with `git show <rev>:<path>`.
//! Anything that cannot be checked is marked `unverifiable`, never refused.
use crate::app::App;
use anyhow::{Result, bail};
use serde::Serialize;

pub const MAX_QUOTE_CHARS: usize = 200;
const MAX_REF_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRef {
    pub repo: String,
    pub rev: String,
    pub path: String,
    pub lines: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    Pr(String),
    Issue(String),
    Task(String),
    Code(CodeRef),
    Url(String),
}

impl Ref {
    pub fn kind(&self) -> &'static str {
        match self {
            Ref::Pr(_) => "pr",
            Ref::Issue(_) => "issue",
            Ref::Task(_) => "task",
            Ref::Code(_) => "code",
            Ref::Url(_) => "url",
        }
    }
}

fn token(kind: &str, value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 120 || !value.chars().all(|c| c.is_ascii_alphanumeric() || "-_./#".contains(c)) {
        bail!("{kind}: reference must be an id such as {kind}:123 (letters, digits, - _ . / #)");
    }
    Ok(value.to_string())
}

/// Parse one reference in one of the agreed shapes.
pub fn parse(raw: &str) -> Result<Ref> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().count() > MAX_REF_CHARS || raw.contains(char::is_whitespace) {
        bail!("evidence reference must be one token of at most {MAX_REF_CHARS} characters");
    }
    let Some((scheme, rest)) = raw.split_once(':') else {
        bail!("evidence reference '{raw}' has no type (pr:, issue:, task:, code:, url:)")
    };
    Ok(match scheme {
        "pr" => Ref::Pr(token("pr", rest)?),
        "issue" => Ref::Issue(token("issue", rest)?),
        "task" => Ref::Task(token("task", rest)?),
        "url" => {
            if !(rest.starts_with("https://") || rest.starts_with("http://")) || rest.len() < 12 {
                bail!("url: reference must be an http(s) URL");
            }
            Ref::Url(rest.to_string())
        }
        "code" => Ref::Code(parse_code(rest)?),
        other => bail!("unknown evidence reference type '{other}:' (pr:, issue:, task:, code:, url:)"),
    })
}

fn parse_code(rest: &str) -> Result<CodeRef> {
    const SHAPE: &str = "code: reference must be code:<repo>@<rev>:<path>[#L<a>[-<b>]]";
    let (repo, rest) = rest.split_once('@').ok_or_else(|| anyhow::anyhow!(SHAPE))?;
    let (rev, rest) = rest.split_once(':').ok_or_else(|| anyhow::anyhow!(SHAPE))?;
    let (path, anchor) = match rest.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (rest, None),
    };
    let clean = |s: &str| !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if !clean(repo) || !clean(rev) || path.is_empty() || path.starts_with('/') || path.split('/').any(|p| p.is_empty() || p == "..") {
        bail!(SHAPE);
    }
    let lines = match anchor {
        None => None,
        Some(a) => {
            let a = a.strip_prefix('L').ok_or_else(|| anyhow::anyhow!(SHAPE))?;
            let (from, to) = match a.split_once('-') {
                Some((x, y)) => (x, y.trim_start_matches('L')),
                None => (a, a),
            };
            let (from, to): (usize, usize) = (from.parse()?, to.parse()?);
            if from == 0 || to < from {
                bail!(SHAPE);
            }
            Some((from, to))
        }
    };
    Ok(CodeRef { repo: repo.into(), rev: rev.into(), path: path.into(), lines })
}

/// One reference after checking, as clients get it back.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Checked {
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    /// `verified`, `quote_not_found`, `unverifiable`, or `none` (no quote to check).
    pub verification: String,
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Whether `label` names this repository: its project name, its directory or the last part of its id.
fn is_this_repository(app: &App, label: &str) -> bool {
    let id_tail = app.repo.id.rsplit('/').next().unwrap_or_default();
    [app.project_name(), app.repo.name.clone(), id_tail.to_string()].iter().any(|n| n.eq_ignore_ascii_case(label))
}

pub fn verify(app: &App, r: &Ref, quote: Option<&str>) -> &'static str {
    let Some(quote) = quote.map(squash).filter(|q| !q.is_empty()) else { return "none" };
    let Ref::Code(c) = r else { return "unverifiable" };
    if !is_this_repository(app, &c.repo) {
        return "unverifiable";
    }
    let Some(text) = app.repo.git_opt(&["show", &format!("{}:{}", c.rev, c.path)]) else { return "unverifiable" };
    let scope: String = match c.lines {
        Some((from, to)) => text.lines().skip(from - 1).take(to - from + 1).collect::<Vec<_>>().join("\n"),
        None => text,
    };
    if squash(&scope).contains(&quote) { "verified" } else { "quote_not_found" }
}

/// Check every reference; fails on a reference that is not one of the shapes or an overlong quote.
pub fn check(app: &App, items: &[(String, Option<String>)]) -> Result<Vec<Checked>> {
    if items.len() > 50 {
        bail!("at most 50 evidence references");
    }
    items
        .iter()
        .map(|(raw, quote)| {
            let r = parse(raw)?;
            let quote = quote.as_deref().map(|q| crate::util::one_line(q).trim().to_string()).filter(|q| !q.is_empty());
            if quote.as_ref().is_some_and(|q| q.chars().count() > MAX_QUOTE_CHARS) {
                bail!("evidence quote must be at most {MAX_QUOTE_CHARS} characters");
            }
            let verification = verify(app, &r, quote.as_deref()).to_string();
            Ok(Checked { reference: raw.trim().to_string(), kind: r.kind().into(), quote, verification })
        })
        .collect()
}

/// The `## Evidence` section for the quoted references: what the reviewer reads next to the claim.
pub fn section(checked: &[Checked]) -> String {
    let quoted: Vec<String> = checked
        .iter()
        .filter_map(|c| {
            let q = c.quote.as_ref()?;
            let verdict = match c.verification.as_str() {
                "verified" => "verified",
                "quote_not_found" => "quote not found at that revision",
                _ => "not verified",
            };
            Some(format!("- {} — \"{}\" ({verdict})", c.reference, q.replace('"', "'")))
        })
        .collect();
    if quoted.is_empty() { String::new() } else { format!("\n## Evidence\n\n{}\n", quoted.join("\n")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_shapes() {
        assert_eq!(parse("pr:123").unwrap(), Ref::Pr("123".into()));
        assert_eq!(parse("task:krk-335").unwrap(), Ref::Task("krk-335".into()));
        assert_eq!(parse("issue:45").unwrap().kind(), "issue");
        assert_eq!(parse("url:https://example.com/a?b=c").unwrap().kind(), "url");
        assert_eq!(
            parse("code:kontext@abc123:src/x.rs#L10-20").unwrap(),
            Ref::Code(CodeRef { repo: "kontext".into(), rev: "abc123".into(), path: "src/x.rs".into(), lines: Some((10, 20)) })
        );
        assert_eq!(
            parse("code:kontext@v0.2.1:src/x.rs#L7").unwrap(),
            Ref::Code(CodeRef { repo: "kontext".into(), rev: "v0.2.1".into(), path: "src/x.rs".into(), lines: Some((7, 7)) })
        );
        for bad in [
            "",
            "pr:",
            "pr:1 2",
            "mail:someone",
            "url:ftp://x",
            "code:kontext:src/x.rs",
            "code:kontext@abc:../x.rs",
            "code:kontext@-x:src/x.rs",
            "code:kontext@abc:src/x.rs#L5-2",
            "code:kontext@abc:src/x.rs#10",
            "noscheme",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn section_lists_quotes_with_their_verdicts() {
        let c = |r: &str, q: Option<&str>, v: &str| Checked {
            reference: r.into(),
            kind: "code".into(),
            quote: q.map(String::from),
            verification: v.into(),
        };
        let s = section(&[
            c("pr:1", None, "none"),
            c("code:a@b:c.rs", Some("say \"hi\""), "verified"),
            c("url:https://x.y/z", Some("q"), "unverifiable"),
        ]);
        assert_eq!(s, "\n## Evidence\n\n- code:a@b:c.rs — \"say 'hi'\" (verified)\n- url:https://x.y/z — \"q\" (not verified)\n");
        assert_eq!(section(&[c("pr:1", None, "none")]), "");
    }
}
