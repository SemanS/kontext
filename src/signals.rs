//! Heuristics that spot decision-shaped commits (used by `why` and by the init history miner).
use regex::Regex;
use std::sync::LazyLock;

static STRONG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(migrat(?:e|ed|es|ing|ion)|switch(?:ed|es|ing)? (?:to|from|over)|replac(?:e|ed|es|ing)\b|in favou?r of|instead of|rather than|adopt(?:ed|s|ing)?\b|deprecat(?:e|ed|es|ing)|decid(?:e|ed|es)|decision|chose|trade-?offs?|rewr(?:ite|ote|itten)|redesign(?:ed)?|re-?architect|consolidat(?:e|ed|ing)|standardi[sz](?:e|ed)|single source of truth|source of truth)\b").unwrap()
});
static MEDIUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(introduc(?:e|ed|es|ing)|drop(?:ped|s)?\b|remov(?:e|ed|es) (?:the|all|support)|mov(?:e|ed|es) (?:to|into)|split (?:into|out)|extract(?:ed)? (?:into|to)|no longer|from now on|always|never|must|policy|convention|contract|breaking|limit|guard|fallback)\b").unwrap()
});
static RATIONALE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(because|so that|to avoid|to prevent|the reason|otherwise|which means|trade)\b").unwrap());
static CONVENTIONAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([a-z]+)(\([^)]*\))?(!)?:").unwrap());

#[derive(Debug, Clone, Default)]
pub struct Signal {
    pub score: f32,
    pub reasons: Vec<String>,
}

pub fn decision_score(subject: &str, body: &str) -> Signal {
    let mut s = Signal::default();
    let text = format!("{subject}\n{body}");
    if let Some(m) = STRONG.find(&text) {
        s.score += 3.0;
        s.reasons.push(format!("\"{}\"", m.as_str().to_lowercase()));
    }
    let medium = MEDIUM.find_iter(&text).count();
    if medium > 0 {
        s.score += (medium.min(3) as f32) * 0.75;
        s.reasons.push(format!("{medium} policy words"));
    }
    if RATIONALE.is_match(body) {
        s.score += 1.0;
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
            "fix" | "chore" | "test" | "style" | "ci" | "build" | "docs" => s.score -= 0.5,
            _ => {}
        }
    }
    let blen = body.trim().len();
    if blen > 600 {
        s.score += 1.0;
    } else if blen > 200 {
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
    }
}
