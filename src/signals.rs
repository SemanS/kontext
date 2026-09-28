//! Heuristics that spot decision-shaped commits (used by `why` and by the init history miner).
use regex::Regex;
use std::sync::LazyLock;

static STRONG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(migrat(?:e|ed|es|ing|ion)|switch(?:ed|es|ing)? (?:to|from|over)|replac(?:e|ed|es|ing)\b|in favou?r of|instead of|rather than|adopt(?:ed|s|ing)?\b|deprecat(?:e|ed|es|ing)|decid(?:e|ed|es)|decision|chose|trade-?offs?|rewr(?:ite|ote|itten)|redesign(?:ed)?|re-?architect|consolidat(?:e|ed|ing)|standardi[sz](?:e|ed)|single source of truth|source of truth)\b").unwrap()
});
static MEDIUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(introduc(?:e|ed|es|ing)|drop(?:ped|s)?\b|remov(?:e|ed|es) (?:the|all|support)|mov(?:e|ed|es) (?:to|into)|split (?:into|out)|extract(?:ed)? (?:into|to)|no longer|from now on|always|never|must|policy|convention|contract|breaking|limit|guard|fallback|rules?|now [a-z]+s|stays|remains)\b").unwrap()
});
static RATIONALE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(because|so that|to avoid|to prevent|the reason|otherwise|which means|trade|since|used to|the cause|to keep|as a result|so (?:the|a|an|it|its|they|we|no|each|every|one|nothing|none))\b").unwrap()
});
/// Choosing one option over another in the plain words commit messages use for it:
/// "X, not Y", "not through Y", "never Y", "no longer", "is gone".
static CONTRAST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:[,;—–]\s*not\s|\bnot\s+(?:by|through|via|from|with|as|in|on|at|for|one|its|their|any|until)\b|\bnever\b|\bno longer\b|\b(?:is|are) gone\b|\bno more\b|\binstead\b)").unwrap()
});
/// "so" as a conjunction ("…, so paging never got past the first page"), not "so far" / "so much".
static SO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bso\s+([a-z]+)").unwrap());
static CONVENTIONAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([a-z]+)(\([^)]*\))?(!)?:").unwrap());

#[derive(Debug, Clone, Default)]
pub struct Signal {
    pub score: f32,
    pub reasons: Vec<String>,
}

pub fn decision_score(subject: &str, body: &str) -> Signal {
    let mut s = Signal::default();
    let text = format!("{subject}\n{body}");
    let strong = STRONG.find(&text);
    if let Some(m) = strong {
        s.score += 3.0;
        s.reasons.push(format!("\"{}\"", m.as_str().to_lowercase()));
    }
    let contrast = CONTRAST.is_match(subject);
    let medium = MEDIUM.find_iter(&text).count();
    if medium > 0 {
        s.score += (medium.min(3) as f32) * 0.75;
        s.reasons.push(format!("{medium} policy words"));
    }
    if contrast {
        s.score += 2.0;
        s.reasons.push("chooses between alternatives".into());
    } else if CONTRAST.is_match(body) {
        s.score += 0.75;
        s.reasons.push("weighs alternatives".into());
    }
    let mut why: std::collections::BTreeSet<String> = RATIONALE.find_iter(body).map(|m| m.as_str().to_lowercase()).collect();
    if SO.captures_iter(body).any(|c| !matches!(c[1].to_lowercase().as_str(), "far" | "much" | "many" | "long" | "on" | "called" | "to")) {
        why.insert("so".into());
    }
    if !why.is_empty() {
        s.score += if why.len() >= 2 { 1.5 } else { 1.0 };
        s.reasons.push("explains why".into());
    }
    if text.contains("BREAKING CHANGE") {
        s.score += 2.0;
        s.reasons.push("breaking change".into());
    }
    if let Some(c) = CONVENTIONAL.captures(subject) {
        let ty = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let scope = c.get(2).map(|m| m.as_str()).unwrap_or("");
        if c.get(3).is_some() {
            s.score += 2.0;
            s.reasons.push("`!` breaking".into());
        }
        match ty {
            "refactor" => s.score += 1.0,
            "feat" | "perf" => s.score += 0.5,
            "revert" => s.score += 1.0,
            "docs" if scope.contains("adr") || scope.contains("decision") => s.score += 2.0,
            // a fix or a note that states a rule ("X, not Y") is a decision all the same
            "fix" | "docs" if contrast || strong.is_some() => {}
            "fix" | "chore" | "test" | "style" | "ci" | "build" | "docs" => s.score -= 0.5,
            _ => {}
        }
    }
    // the reasoning lives in the body: a long one is usually an explanation
    let blen = body.trim().len();
    if blen > 1200 {
        s.score += 1.5;
    } else if blen > 700 {
        s.score += 1.0;
    } else if blen > 300 {
        s.score += 0.5;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores() {
        let a = decision_score("refactor(api): replace moment with dayjs", "Because moment is deprecated and heavy.");
        assert!(a.score >= 5.0, "{a:?}");
        let b = decision_score("fix(ui): typo", "");
        assert!(b.score < 1.0, "{b:?}");
        let c = decision_score("feat!: drop Node 16", "");
        assert!(c.score >= 3.0, "{c:?}");
        // choices phrased as rules, with the reasoning in the body
        let body = "The worker shared the user's browser profile, so a login there logged the worker out. \
                    A separate profile is safer since it has its own cookies; the user's profile is never touched again. "
            .repeat(3);
        let d = decision_score("fix(worker): run in a profile of its own, never the user's", &body);
        assert!(d.score >= 3.5, "{d:?}");
        let why = "Offsets shift while new rows arrive.";
        let e = decision_score("fix(import): page through results by cursor, not by offset", why);
        assert!(e.score >= decision_score("fix(import): page through results by cursor", why).score + 1.5, "{e:?}");
        let f = decision_score("fix(ui): align the side column", "");
        assert!(f.score < 1.0, "{f:?}");
    }
}
