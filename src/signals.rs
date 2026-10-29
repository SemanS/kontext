//! Heuristics that spot decision-shaped commits (used by `why` and by the init history miner).
use regex::Regex;
use serde::{Deserialize, Serialize};
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

/// Extra decision words, bundled for a detected language or set in `[init] decision_words`
/// (`strong`, `rationale`, `contrast`). Each is a word or phrase matched case-insensitively at the
/// start of a word, so a stem (`migr`) covers its inflections.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct DecisionWordsCfg {
    pub strong: Vec<String>,
    pub rationale: Vec<String>,
    pub contrast: Vec<String>,
}

const SK_STRONG: &[&str] = &[
    "prechod",
    "prešl",
    "presun",
    "migr",
    "nahrad",
    "nahrádz",
    "namiesto",
    "v prospech",
    "rozhodl",
    "rozhodnut",
    "zvolil",
    "zvolen",
    "prepís",
    "prepis",
    "zjednot",
    "jediný zdroj",
    "zavied",
    "zavádz",
    "opúšť",
    "opust",
    "deprek",
];
const SK_RATIONALE: &[&str] =
    &["lebo", "pretože", "keďže", "aby sa", "aby ", "kvôli", "inak", "preto", "takže", "z dôvodu", "dôvod", "vďaka"];
const SK_CONTRAST: &[&str] = &["namiesto", "nie cez", "nie z ", "nie ako", "už nie", "už ne", "nikdy", "radšej"];
const CS_STRONG: &[&str] = &[
    "přechod",
    "přešl",
    "přesun",
    "migr",
    "nahraz",
    "nahrad",
    "místo",
    "ve prospěch",
    "rozhodl",
    "rozhodnut",
    "zvolil",
    "zvolen",
    "přeps",
    "sjednot",
    "jediný zdroj",
    "zaved",
    "zavád",
    "opouš",
    "opušt",
    "deprek",
];
const CS_RATIONALE: &[&str] =
    &["protože", "jelikož", "neboť", "aby se", "aby ", "kvůli", "jinak", "proto", "takže", "z důvodu", "důvod", "díky"];
const CS_CONTRAST: &[&str] = &["místo", "ne přes", "ne z ", "ne jako", "už ne", "nikdy", "raději", "radši"];

/// The bundled words for a detected language (`sk`, `cs`); English is built in.
pub fn bundled_words(lang: &str) -> DecisionWordsCfg {
    let v = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match lang {
        "sk" => DecisionWordsCfg { strong: v(SK_STRONG), rationale: v(SK_RATIONALE), contrast: v(SK_CONTRAST) },
        "cs" => DecisionWordsCfg { strong: v(CS_STRONG), rationale: v(CS_RATIONALE), contrast: v(CS_CONTRAST) },
        _ => DecisionWordsCfg::default(),
    }
}

/// Letters and words that mark a subject as Slovak or Czech (and which of the two).
const SLAVIC_LETTERS: &str = "áäčďéěíĺľňóôŕřšťúůýž";
const SLAVIC_WORDS: &[&str] = &[" lebo ", " pretože ", " alebo ", " namiesto ", " kvôli ", " protože ", " nebo ", " místo ", " kvůli "];
const SK_MARKS: &[&str] = &["ä", "ô", "ľ", "ĺ", "ŕ", " lebo ", " pretože ", " alebo ", " namiesto ", " kvôli ", " pre ", " sa ", "pridan"];
const CS_MARKS: &[&str] = &["ř", "ě", "ů", " protože ", " nebo ", " místo ", " kvůli ", " pro ", " se ", "přidán"];

/// The language of the commit subjects when a good share is not English: `sk`, `cs` or None.
pub fn detect_language<'a>(subjects: impl IntoIterator<Item = &'a str>) -> Option<&'static str> {
    let (mut n, mut sk, mut cs, mut marked) = (0usize, 0usize, 0usize, 0usize);
    for s in subjects {
        n += 1;
        let l = format!(" {} ", s.to_lowercase().replace([',', '.', ':', ';'], " "));
        if l.chars().any(|c| SLAVIC_LETTERS.contains(c)) || SLAVIC_WORDS.iter().any(|w| l.contains(w)) {
            marked += 1;
        }
        sk += SK_MARKS.iter().filter(|m| l.contains(*m)).count();
        cs += CS_MARKS.iter().filter(|m| l.contains(*m)).count();
    }
    if n == 0 || marked < 2 || marked * 5 < n {
        return None;
    }
    Some(if cs > sk { "cs" } else { "sk" })
}

/// Extra decision words, compiled once per mining run.
#[derive(Debug, Clone, Default)]
pub struct Words {
    strong: Option<Regex>,
    rationale: Option<Regex>,
    contrast: Option<Regex>,
}

fn alternation(words: &[String]) -> Option<Regex> {
    let alts: Vec<String> = words.iter().map(|w| w.trim_start()).filter(|w| !w.trim().is_empty()).map(regex::escape).collect();
    if alts.is_empty() {
        return None;
    }
    Regex::new(&format!(r"(?i)\b(?:{})", alts.join("|"))).ok()
}

impl Words {
    /// The bundled set for `lang` plus the configured words.
    pub fn new(lang: Option<&str>, cfg: &DecisionWordsCfg) -> Words {
        let mut w = lang.map(bundled_words).unwrap_or_default();
        w.strong.extend(cfg.strong.iter().cloned());
        w.rationale.extend(cfg.rationale.iter().cloned());
        w.contrast.extend(cfg.contrast.iter().cloned());
        Words { strong: alternation(&w.strong), rationale: alternation(&w.rationale), contrast: alternation(&w.contrast) }
    }
}

/// The part of a commit body people wrote: bots append release notes and changelogs after a
/// `Release notes` heading or inside `<details>`.
pub fn human_body(body: &str) -> &str {
    let mut cut = body.len();
    for marker in ["release notes", "<details>"] {
        if let Some(i) = find_ascii_ci(body, marker) {
            cut = cut.min(i);
        }
    }
    &body[..cut]
}

/// Byte offset of an ASCII `needle` in `hay`, ignoring ASCII case (safe on any UTF-8 `hay`).
fn find_ascii_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| hay.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// A dependency bump rather than a choice: `chore(deps): …`, `build(deps-dev): …`, `Bump x from 1 to 2`.
pub fn is_dependency_bump(subject: &str) -> bool {
    static BUMP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:(?:chore|build|fix|ci)\((?:deps|deps-dev|dependencies)[^)]*\)!?:|bump\s+\S+\s+from\s|update (?:dependency|module|rust crate)\s)")
            .unwrap()
    });
    BUMP.is_match(subject.trim())
}

/// Authors whose commits are never decisions: bots by default, plus `[init] bot_authors`.
pub fn is_bot_author(author: &str, extra: &[String]) -> bool {
    let a = author.to_lowercase();
    ["[bot]", "renovate", "dependabot", "github-actions", "greenkeeper", "snyk-bot"].iter().any(|b| a.contains(b))
        || extra.iter().map(|b| b.trim().to_lowercase()).any(|b| !b.is_empty() && a.contains(&b))
}

#[derive(Debug, Clone, Default)]
pub struct Signal {
    pub score: f32,
    pub reasons: Vec<String>,
}

pub fn decision_score(subject: &str, body: &str) -> Signal {
    decision_score_with(subject, body, &Words::default())
}

/// `decision_score` with extra (non-English or configured) decision words.
pub fn decision_score_with(subject: &str, body: &str, words: &Words) -> Signal {
    let mut s = Signal::default();
    let body = human_body(body);
    let text = format!("{subject}\n{body}");
    let strong = STRONG.find(&text).or_else(|| words.strong.as_ref().and_then(|r| r.find(&text)));
    if let Some(m) = strong {
        s.score += 3.0;
        s.reasons.push(format!("\"{}\"", m.as_str().trim().to_lowercase()));
    }
    let contrast_in = |t: &str| CONTRAST.is_match(t) || words.contrast.as_ref().is_some_and(|r| r.is_match(t));
    let contrast = contrast_in(subject);
    let medium = MEDIUM.find_iter(&text).count();
    if medium > 0 {
        s.score += (medium.min(3) as f32) * 0.75;
        s.reasons.push(format!("{medium} policy words"));
    }
    if contrast {
        s.score += 2.0;
        s.reasons.push("chooses between alternatives".into());
    } else if contrast_in(body) {
        s.score += 0.75;
        s.reasons.push("weighs alternatives".into());
    }
    let mut why: std::collections::BTreeSet<String> = RATIONALE.find_iter(body).map(|m| m.as_str().to_lowercase()).collect();
    if let Some(r) = &words.rationale {
        why.extend(r.find_iter(body).map(|m| m.as_str().trim().to_lowercase()));
    }
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
    } else if subject.starts_with("Revert \"") {
        s.score += 1.0;
        s.reasons.push("revert".into());
    }
    // language-neutral structure: a reasoned body has paragraphs or a list of points
    let paragraphs = body.split("\n\n").filter(|p| p.trim().chars().count() > 60).count();
    let bullets = body.lines().filter(|l| matches!(l.trim_start().chars().next(), Some('-' | '*' | '•'))).count();
    if paragraphs >= 2 || bullets >= 3 {
        s.score += 0.5;
        s.reasons.push("reasoned body".into());
    }
    // the reasoning lives in the body: a long one is usually an explanation (counted in characters,
    // so diacritics do not inflate it)
    let blen = body.trim().chars().count();
    if blen > 1200 {
        s.score += 1.5;
    } else if blen > 700 {
        s.score += 1.0;
    } else if blen > 300 {
        s.score += 0.5;
    }
    // a dependency bump is housekeeping, whatever its release notes say
    if is_dependency_bump(subject) {
        s.score -= 4.0;
        s.reasons.push("dependency bump".into());
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

    /// The English fixtures score exactly what they scored before languages and structure were added.
    #[test]
    fn english_scores_unchanged() {
        let a = decision_score("refactor(api): replace moment with dayjs", "Because moment is deprecated and heavy.");
        assert_eq!(a.score, 3.0 + 1.0 + 1.0, "{a:?}");
        assert_eq!(decision_score("fix(ui): typo", "").score, -0.5);
        assert_eq!(decision_score("feat!: drop Node 16", "").score, 0.75 + 2.0 + 0.5);
        let w = Words::new(None, &DecisionWordsCfg::default());
        let x = decision_score_with("refactor(api): replace moment with dayjs", "Because moment is deprecated and heavy.", &w);
        assert_eq!(x.score, a.score);
    }

    #[test]
    fn slovak_words_score_like_english_ones() {
        let subject = "Prechod z MySQL na Postgres namiesto MariaDB";
        let body = "Prechádzame na Postgres, lebo potrebujeme JSONB a transakčné DDL.";
        let plain = decision_score(subject, body);
        assert!(plain.score < 3.5, "{plain:?}");
        let sk = decision_score_with(subject, body, &Words::new(Some("sk"), &DecisionWordsCfg::default()));
        assert!(sk.score >= 3.5, "{sk:?}");
        // configured words apply on top of any language
        let cfg = DecisionWordsCfg { strong: vec!["umstieg".into()], ..Default::default() };
        let de = decision_score_with("Umstieg auf Postgres", "", &Words::new(None, &cfg));
        assert!(de.score >= 3.0, "{de:?}");
    }

    #[test]
    fn detects_slovak_and_czech_subjects() {
        let sk = ["Prechod z MySQL na Postgres, lebo potrebujeme JSONB", "Oprava prihlásenia pre adminov", "Pridaný export do CSV"];
        assert_eq!(detect_language(sk.iter().copied()), Some("sk"));
        let cs = ["Přechod na Postgres, protože potřebujeme JSONB", "Oprava přihlášení pro adminy", "Přidán export do CSV"];
        assert_eq!(detect_language(cs.iter().copied()), Some("cs"));
        let en = ["feat: add export", "fix: login for admins", "refactor: move to Postgres"];
        assert_eq!(detect_language(en.iter().copied()), None);
    }

    #[test]
    fn bots_and_bumps_do_not_look_like_decisions() {
        assert!(is_bot_author("dependabot[bot]", &[]));
        assert!(is_bot_author("Renovate Bot", &[]));
        assert!(is_bot_author("release-robot", &["release-robot".into()]));
        assert!(!is_bot_author("Jana Nováková", &[]));
        let notes = format!(
            "Bumps vite from 4.5.0 to 5.0.0.\n<details>\n<summary>Release notes</summary>\n{}</details>",
            "Breaking: migrate the config, deprecated options removed, because the API changed. ".repeat(30)
        );
        let s = decision_score("chore(deps-dev): bump vite from 4.5.0 to 5.0.0", &notes);
        assert!(s.score < 2.5, "{s:?}");
        assert_eq!(human_body("Why we did it.\n\nRelease Notes\n- migrate everything"), "Why we did it.\n\n");
    }
}
