//! Secret detection and redaction for anything that may become shared knowledge. Redaction fails
//! closed: a `secrets.redact` pattern that does not compile stops whatever would send text to a
//! model ([`compile_patterns`], [`pattern_errors`]), and the private config layers' lists add to
//! the repository's, never the other way round ([`redact_list`], [`allow_list`]).
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    High,
    Medium,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: &'static str,
    pub line: usize,
    pub excerpt: String,
    pub severity: Severity,
}

struct Rule {
    name: &'static str,
    re: Regex,
    severity: Severity,
    /// When set, capture group 1 is the value and must pass this check (placeholders, randomness).
    value_ok: Option<fn(&str) -> bool>,
}

/// A private key, BEGIN to END; without an END line, to the end of the paragraph.
const PRIVATE_KEY: &str = r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----(?:[\s\S]*?-----END (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----|[\s\S]*?(?:\n[ \t]*\n|\z))";

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |name, pat: &str, severity| Rule { name, re: Regex::new(pat).expect("secret rule"), severity, value_ok: None };
    let checked = |name, pat: &str, severity, ok: fn(&str) -> bool| Rule {
        name,
        re: Regex::new(pat).expect("secret rule"),
        severity,
        value_ok: Some(ok),
    };
    vec![
        r("private-key", PRIVATE_KEY, Severity::High),
        r("gcp-service-account", r#""private_key"\s*:\s*"-----BEGIN"#, Severity::High),
        r("aws-access-key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", Severity::High),
        r("github-token", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b", Severity::High),
        r("github-pat", r"\bgithub_pat_[A-Za-z0-9_]{40,}\b", Severity::High),
        r("gitlab-token", r"\bglpat-[A-Za-z0-9_-]{20,}\b", Severity::High),
        r("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b", Severity::High),
        r("slack-webhook", r"https://hooks\.slack\.com/(?:services|workflows|triggers)/[A-Za-z0-9/_-]{20,}", Severity::High),
        r("anthropic-key", r"\bsk-ant-[A-Za-z0-9_-]{20,}", Severity::High),
        r("openai-key", r"\bsk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{32,}", Severity::High),
        r("stripe-key", r"\b(?:sk|rk)_live_[0-9A-Za-z]{20,}\b", Severity::High),
        r("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b", Severity::Medium),
        r("google-oauth-secret", r"\bGOCSPX-[A-Za-z0-9_-]{28}\b", Severity::High),
        r("npm-token", r"\bnpm_[A-Za-z0-9]{36}\b", Severity::High),
        r("pypi-token", r"\bpypi-AgE[A-Za-z0-9_-]{50,}", Severity::High),
        r("huggingface-token", r"\bhf_[A-Za-z0-9]{34,}\b", Severity::High),
        r("sendgrid-key", r"\bSG\.[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43}\b", Severity::High),
        r("azure-storage-key", r"(?i)\bAccountKey=[A-Za-z0-9+/]{40,}={0,2}", Severity::High),
        r("azure-sas-signature", r"[?&]sig=[A-Za-z0-9%+/]{30,}(?:=|%3D){0,2}", Severity::High),
        r("twilio-key", r"\bSK[0-9a-fA-F]{32}\b", Severity::High),
        r("digitalocean-token", r"\bdo[por]_v1_[a-f0-9]{64}\b", Severity::High),
        r("databricks-token", r"\bdapi[a-f0-9]{32}(?:-\d)?\b", Severity::High),
        r("shopify-token", r"\bshp(?:at|ca|pa|ss)_[a-fA-F0-9]{32}\b", Severity::High),
        r("dotenv-vault-key", r"dotenv://:key_[0-9a-f]{32,}", Severity::High),
        r("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", Severity::Medium),
        r("bearer-token", r"(?i)\bauthorization:\s*bearer\s+[A-Za-z0-9._~+/-]{20,}", Severity::High),
        r("url-credentials", r"\b[a-z][a-z0-9+.-]*://[^/\s:@]{1,64}:[^/\s:@]{6,128}@[^\s/]+", Severity::High),
        checked(
            "assigned-secret",
            r#"(?i)\b[A-Z0-9_]*(?:api[_-]?key|secret|token|passw(?:or)?d|client[_-]?secret|access[_-]?key)[A-Z0-9_]*\b\s*[:=]\s*["']?([A-Za-z0-9/+_=.\-]{12,})"#,
            Severity::Medium,
            |v| !looks_like_placeholder(v),
        ),
        // notes put secrets into sentences: "the secret is …", "rotate the token Xy9…"
        checked(
            "secret-in-text",
            r#"(?i)\b(?:secret|token|password|passwd|passphrase|api[ _-]?key|access[ _-]?key|private[ _-]?key|credentials?)\b(?:\s+(?:is|was|of|for|value|key|now)){0,3}\s+["'`]?([A-Za-z0-9/+_=\-]{24,})"#,
            Severity::Medium,
            |v| !looks_like_placeholder(v) && looks_random(v),
        ),
        // lowercase hex keys in prose: "the api key 3f9a…", "token: deadbeef…"
        checked(
            "hex-secret",
            r#"(?i:\b(?:secret|token|password|passwd|api[ _-]?key|access[ _-]?key|signing[ _-]?key|private[ _-]?key|client[ _-]?secret)\b)[^\n]{0,24}?\b([0-9a-f]{32,64})\b"#,
            Severity::Medium,
            |v| !looks_like_placeholder(v) && looks_hex_random(v),
        ),
    ]
});

/// A hex value with digits and letters and some entropy (not `0000…`, not `abababab…`).
fn looks_hex_random(v: &str) -> bool {
    v.chars().any(|c| c.is_ascii_digit()) && v.chars().any(|c| c.is_ascii_alphabetic()) && entropy(v) >= 3.0
}

/// Mixed case and digits, high entropy, and not a path like `src/auth/TokenRefresh2`.
fn looks_random(v: &str) -> bool {
    let word = |seg: &str| {
        let mut c = seg.chars();
        c.next().is_some_and(|f| f.is_ascii_alphabetic())
            && seg.chars().filter(|c| c.is_ascii_digit()).count() <= 1
            && seg.chars().zip(seg.chars().skip(1)).filter(|(a, b)| a.is_ascii_uppercase() && b.is_ascii_uppercase()).count() == 0
    };
    if v.contains('/') && v.split('/').all(word) {
        return false;
    }
    let (digit, upper, lower) =
        (v.chars().any(|c| c.is_ascii_digit()), v.chars().any(|c| c.is_ascii_uppercase()), v.chars().any(|c| c.is_ascii_lowercase()));
    digit && upper && lower && entropy(v) >= 3.5
}

/// Shannon entropy in bits per character.
fn entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let n = s.chars().count() as f64;
    counts.values().map(|&k| k as f64 / n).map(|p| -p * p.log2()).sum()
}

fn looks_like_placeholder(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("xxx")
        || l.contains("example")
        || l.contains("changeme")
        || l.contains("placeholder")
        || l.contains("your_")
        || l.contains("your-")
        || l.starts_with("${")
        || l.starts_with('<')
        || l.starts_with("process.env")
        || l.starts_with("env.")
        || l.contains("redacted")
        || l.chars().all(|c| c == '*' || c == 'x' || c == '.')
}

/// Every match of every rule on every line (a placeholder earlier on a line hides nothing after it).
pub fn scan(text: &str, allow: &[Regex]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.contains("kontext:allow-secret") || allow.iter().any(|a| a.is_match(line)) {
            continue;
        }
        for rule in RULES.iter() {
            for m in rule.re.captures_iter(line) {
                let value = m.get(1).map(|g| g.as_str()).unwrap_or_else(|| m.get(0).unwrap().as_str());
                if rule.value_ok.is_some_and(|ok| !ok(value)) {
                    continue;
                }
                let whole = m.get(0).unwrap().as_str();
                out.push(Finding { rule: rule.name, line: i + 1, excerpt: mask(whole), severity: rule.severity });
            }
        }
    }
    out
}

fn mask(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 10 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..6].iter().collect();
    format!("{head}…({} chars)", chars.len())
}

/// Replace every high-confidence secret with `[redacted:<rule>]`. Returns the text and the number of redactions.
pub fn redact(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut n = 0;
    for rule in RULES.iter() {
        if let Some(ok) = rule.value_ok {
            let replaced = rule.re.replace_all(&out, |c: &regex::Captures| {
                let whole = c.get(0).unwrap().as_str();
                let value = c.get(1).map(|g| g.as_str()).unwrap_or("");
                if ok(value) {
                    n += 1;
                    whole.replace(value, "[redacted]")
                } else {
                    whole.to_string()
                }
            });
            out = replaced.into_owned();
            continue;
        }
        let count = rule.re.find_iter(&out).count();
        if count > 0 {
            n += count;
            out = rule.re.replace_all(&out, format!("[redacted:{}]", rule.name).as_str()).into_owned();
        }
    }
    (out, n)
}

/// Compiled `secrets.redact` patterns (invalid ones are skipped). Prefer [`compile_patterns`],
/// which fails closed.
pub fn patterns(list: &[String]) -> Vec<Regex> {
    list.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

/// Every pattern compiled, or the errors of those that do not (`'<pattern>': <why>`). A caller
/// that masks text before it leaves the machine refuses to go on with any error.
pub fn compile_patterns(list: &[String]) -> Result<Vec<Regex>, Vec<String>> {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for p in list {
        match Regex::new(p) {
            Ok(re) => ok.push(re),
            Err(e) => errors.push(format!("'{p}': {}", crate::util::one_line(&e.to_string()))),
        }
    }
    if errors.is_empty() { Ok(ok) } else { Err(errors) }
}

/// `secrets.<key>` (`redact` or `allow`) of every config layer together: the merged value, plus
/// what each layer file lists, so a repository's list adds to a private one instead of replacing
/// it. In order, without duplicates.
fn layered(loaded: &crate::config::LoadedConfig, key: &str) -> Vec<String> {
    let merged = match key {
        "redact" => &loaded.cfg.secrets.redact,
        _ => &loaded.cfg.secrets.allow,
    };
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: &str| {
        if !out.iter().any(|x| x == p) {
            out.push(p.to_string());
        }
    };
    for layer in &loaded.layers {
        let Ok(text) = std::fs::read_to_string(layer) else { continue };
        let Ok(table) = text.parse::<toml::Table>() else { continue };
        let list = table.get("secrets").and_then(|s| s.get(key)).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for p in list.iter().filter_map(|v| v.as_str()) {
            add(p);
        }
    }
    for p in merged {
        add(p);
    }
    out
}

/// The `secrets.redact` patterns of every config layer together (private and repository).
pub fn redact_list(loaded: &crate::config::LoadedConfig) -> Vec<String> {
    layered(loaded, "redact")
}

/// The `secrets.allow` patterns of every config layer together.
pub fn allow_list(loaded: &crate::config::LoadedConfig) -> Vec<String> {
    layered(loaded, "allow")
}

/// Why redaction cannot be trusted with this configuration: every `secrets.redact` or
/// `secrets.allow` pattern of any layer that does not compile. For `kontext status`, `check` and
/// app warnings; distill, ctx_threads and autopilot refuse to run while it is not empty.
pub fn pattern_errors(loaded: &crate::config::LoadedConfig) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["redact", "allow"] {
        if let Err(errors) = compile_patterns(&layered(loaded, key)) {
            out.extend(errors.into_iter().map(|e| format!("secrets.{key} pattern {e}")));
        }
    }
    out
}

/// The `secrets.redact` patterns of every layer, compiled, or an error naming those that do not
/// compile: what a caller about to send text to a model uses.
pub fn redaction(loaded: &crate::config::LoadedConfig) -> anyhow::Result<Vec<Regex>> {
    compile_patterns(&redact_list(loaded)).map_err(|errors| {
        anyhow::anyhow!("secrets.redact has a pattern that does not compile, so nothing is sent until it is fixed: {}", errors.join("; "))
    })
}

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\b[a-zA-Z][a-zA-Z0-9+.-]*://[^\s"'<>`]+"#).unwrap());

/// A URL as an error may show it: scheme, host and path, without userinfo, query string or
/// fragment, and with secrets in the path (a webhook) redacted.
pub fn strip_url_secrets(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((s, r)) => (Some(s), r),
        None => (None, url),
    };
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let stripped = match scheme {
        Some(s) => format!("{s}://{host}{path}"),
        None => format!("{host}{path}"),
    };
    redact(&stripped).0
}

/// An error message fit for agents, notes and the outbox: every URL stripped of userinfo, query
/// and fragment, then the secret rules applied.
pub fn scrub_error(text: &str) -> String {
    let stripped = URL.replace_all(text, |c: &regex::Captures| strip_url_secrets(&c[0]));
    redact(&stripped).0
}

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})\b").unwrap());
static PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^\w/+])(\+\d{1,3}(?:[ .-]?\(?\d{1,4}\)?){2,5})").unwrap());
/// National formats: a leading 0 (`0905 123 456`, `02/1234 5678`) or a US area code
/// (`(555) 123-4567`), always with separators; the digits are counted before masking.
static NATIONAL_PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\w/+.:#-])(\(0\d{1,4}\)[ /-]?\d{2,4}(?:[ /-]\d{2,4}){1,3}|0\d{1,4}(?:[ /-]\d{2,4}){2,4}|\(\d{3}\) ?\d{3}-\d{4})")
        .unwrap()
});
static IBAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z]{2}\d{2}(?: ?[A-Z0-9]{4}){3,7}(?: ?[A-Z0-9]{1,3})?\b").unwrap());
static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d(?:[ -]?\d){12,18}\b").unwrap());
static IPV4: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").unwrap());
static IPV6: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[0-9a-f]{0,4}(?::[0-9a-f]{0,4}){2,7}").unwrap());

/// A candidate `text[start..end]` stands on its own: no word character, dot-digit or colon glued
/// to either side (a version, an id, a path, a Rust path).
fn standalone(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let mut after = text[end..].chars();
    let next = after.next();
    let glued_before = before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':');
    let glued_after = match next {
        Some(c) if c.is_alphanumeric() || c == '_' || c == ':' => true,
        Some('.' | '-' | '/') => after.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    };
    !glued_before && !glued_after
}

fn luhn(digits: &str) -> bool {
    let mut sum = 0;
    for (i, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else { return false };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

/// Mask personal and infrastructure data that agent threads carry and team knowledge does not
/// need: email addresses, phone numbers (international and national with separators), IBANs, card
/// numbers, IPv4 and IPv6 addresses (loopback stays), and the `extra` patterns of `secrets.redact`.
/// Returns the text and the number of values masked.
pub fn redact_personal(text: &str, extra: &[Regex]) -> (String, usize) {
    let mut n = 0;
    let mut out = EMAIL
        .replace_all(text, |c: &regex::Captures| {
            let domain = c[1].to_lowercase();
            let fake = ["example.com", "example.org", "example.net", "localhost"].contains(&domain.as_str())
                || [".example", ".test", ".invalid", ".local"].iter().any(|s| domain.ends_with(s));
            if fake {
                c[0].to_string()
            } else {
                n += 1;
                "[email]".to_string()
            }
        })
        .into_owned();
    out = PHONE
        .replace_all(&out, |c: &regex::Captures| {
            if c[2].chars().filter(char::is_ascii_digit).count() >= 9 {
                n += 1;
                format!("{}[phone]", &c[1])
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    out = IBAN
        .replace_all(&out, |c: &regex::Captures| {
            if c[0].chars().filter(char::is_ascii_digit).count() >= 10 {
                n += 1;
                "[iban]".to_string()
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    out = CARD
        .replace_all(&out, |c: &regex::Captures| {
            let digits: String = c[0].chars().filter(char::is_ascii_digit).collect();
            if luhn(&digits) && !digits.chars().all(|d| d == digits.chars().next().unwrap_or('0')) {
                n += 1;
                "[card]".to_string()
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    let source = out.clone();
    out = NATIONAL_PHONE
        .replace_all(&source, |c: &regex::Captures| {
            let m = c.get(2).unwrap();
            let digits = m.as_str().chars().filter(char::is_ascii_digit).count();
            if (9..=12).contains(&digits) && standalone(&source, m.start(), m.end()) {
                n += 1;
                format!("{}[phone]", &c[1])
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    out = IPV4
        .replace_all(&out, |c: &regex::Captures| {
            let octets: Vec<u32> = (1..=4).filter_map(|i| c[i].parse().ok()).collect();
            let valid = octets.len() == 4 && octets.iter().all(|o| *o <= 255);
            if valid && c[0] != *"127.0.0.1" && c[0] != *"0.0.0.0" {
                n += 1;
                "[ip]".to_string()
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    let source = out.clone();
    out = IPV6
        .replace_all(&source, |c: &regex::Captures| {
            let m = c.get(0).unwrap();
            let s = m.as_str();
            let groups = s.split(':').filter(|g| !g.is_empty()).count();
            let address = s.parse::<std::net::Ipv6Addr>().ok();
            let real = address.is_some_and(|a| !a.is_loopback() && !a.is_unspecified())
                && (groups >= 3 || s.chars().any(|c| c.is_ascii_digit()))
                && standalone(&source, m.start(), m.end());
            if real {
                n += 1;
                "[ip]".to_string()
            } else {
                s.to_string()
            }
        })
        .into_owned();
    for re in extra {
        let k = re.find_iter(&out).count();
        if k > 0 {
            n += k;
            out = re.replace_all(&out, "[redacted]").into_owned();
        }
    }
    (out, n)
}

/// Files that must never be read into shared context.
pub fn is_sensitive_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    if name.starts_with(".env") {
        return !(name.ends_with(".example") || name.ends_with(".sample") || name.ends_with(".template") || name.ends_with(".dist"));
    }
    matches!(
        name.as_str(),
        "id_rsa" | "id_ed25519" | "id_ecdsa" | "credentials.json" | ".npmrc" | ".pypirc" | ".netrc" | "secrets.env" | "auth.json"
    ) || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".keystore")
        || name.ends_with(".tfstate")
        || name.ends_with(".tfvars")
        || (name.contains("service") && name.contains("account") && name.ends_with(".json"))
        || (name.contains("secret") && [".json", ".yaml", ".yml", ".env", ".txt", ".toml", ".ini"].iter().any(|ext| name.ends_with(ext)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_redacts() {
        let text = "token ghp_abcdefghijklmnopqrstuvwxyz0123456789AB here\nOPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345\nAPI_KEY=your_api_key_here\npassword: hunter2hunter2hunter2";
        let f = scan(text, &[]);
        let rules: Vec<_> = f.iter().map(|x| x.rule).collect();
        assert!(rules.contains(&"github-token"));
        assert!(rules.contains(&"openai-key"));
        assert!(rules.contains(&"assigned-secret"));
        assert!(!f.iter().any(|x| x.line == 3), "placeholder must not be flagged");
        let (red, n) = redact(text);
        assert!(n >= 3);
        assert!(!red.contains("ghp_abcdef"));
        assert!(red.contains("your_api_key_here"));
        assert!(!red.contains("hunter2hunter2"));
    }

    #[test]
    fn secrets_in_sentences() {
        let t = "Rotate it: the secret is wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ8kP2mNq now\nand the api key 3fA9kQ2mZx7LpR4sT8vW1yB6nC0dE5gH.";
        let f = scan(t, &[]);
        assert_eq!(f.iter().filter(|x| x.rule == "secret-in-text").count(), 2, "{f:?}");
        let (red, n) = redact(t);
        assert!(n >= 2 && !red.contains("wJalrXUtn") && !red.contains("3fA9kQ2m"), "{red}");
        // prose about secrets, paths and placeholders stay
        for ok in [
            "The token budget is 1400 and secret management lives in Vault.",
            "The token refresher lives in src/auth/TokenRefreshHandler2 now.",
            "Set the api key to YOUR_API_KEY_GOES_HERE_1234567 first.",
            "password hashing uses argon2id with a per-user salt",
        ] {
            assert!(scan(ok, &[]).is_empty(), "{ok}: {:?}", scan(ok, &[]));
        }
    }

    #[test]
    fn personal_data() {
        let t = "Write to jane.doe@acme-corp.com or call +421 905 123 456; test@example.com is fine.\n\
                 IBAN SK31 1200 0000 1987 4263 7541, card 4111 1111 1111 1111, host 34.117.59.81, local 127.0.0.1, v1.2.3.\n\
                 The integration id is ACME-4471.";
        let extra = patterns(&["ACME-\\d+".to_string()]);
        let (out, n) = redact_personal(t, &extra);
        for gone in ["jane.doe@", "905 123 456", "SK31", "4111", "34.117.59.81", "ACME-4471"] {
            assert!(!out.contains(gone), "{gone} survived: {out}");
        }
        for kept in ["test@example.com", "127.0.0.1", "v1.2.3"] {
            assert!(out.contains(kept), "{kept} was masked: {out}");
        }
        assert_eq!(n, 6, "{out}");
    }

    #[test]
    fn allow_marker() {
        assert!(scan("AKIAABCDEFGHIJKLMNOP kontext:allow-secret", &[]).is_empty());
        assert_eq!(scan("AKIAABCDEFGHIJKLMNOP", &[]).len(), 1);
    }

    #[test]
    fn sensitive_paths() {
        assert!(is_sensitive_path(".env.keys"));
        assert!(is_sensitive_path("apps/api/.env.local"));
        assert!(!is_sensitive_path(".env.example"));
        assert!(is_sensitive_path("infra/prod.tfvars"));
        assert!(!is_sensitive_path("src/main.rs"));
    }

    fn rules(text: &str) -> Vec<&'static str> {
        scan(text, &[]).iter().map(|f| f.rule).collect()
    }

    #[test]
    fn rules_table() {
        let hex32 = "3f9a2c7e1b4d6f8a0c2e4a6b8d0f1a3c";
        let alnum36 = &["aB3dE5fG7hJ9kL1mN3pQ", "5rS7tU9vW1xY3zA5"].concat();
        let cases: Vec<(&str, String, String)> = vec![
            ("private-key", ["-----", "BEGIN OPENSSH PRIVATE KEY-----"].concat(), ["-----", "BEGIN PUBLIC KEY-----"].concat()),
            ("gcp-service-account", r#""private_key": "-----BEGIN PRIVATE KEY-----\nMIIE""#.into(), r#""private_key_id": "abc""#.into()),
            ("aws-access-key", ["key A", "KIAIOSFODNN7EXAMPLE here"].concat(), "AKIA1234".into()),
            ("github-token", format!("ghp_{alnum36}"), "ghp_short".into()),
            ("github-pat", format!("github_pat_{alnum36}_abcdefgh"), "github_pat_abc".into()),
            ("gitlab-token", format!("glpat-{}", &alnum36[..20]), "glpat-short".into()),
            ("slack-token", ["xoxb-", "1234567890-abcdefghij"].concat(), ["xoxz-", "1234567890-abcdefghij"].concat()),
            (
                "slack-webhook",
                ["https://hooks.slack.com/services/", "T0000AAAA/B0000BBBB/aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat(),
                ["https", "://hooks.slack.com/"].concat(),
            ),
            ("anthropic-key", ["sk-an", "t-api03-aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat(), "sk-ant-short".into()),
            ("openai-key", format!("sk-proj-{alnum36}"), "sk-123".into()),
            ("stripe-key", ["sk_li", "ve_aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat(), ["sk_te", "st_aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat()),
            ("google-api-key", ["AIzaS", "yA1bC2dE3fG4hI5jK6lM7nO8pQ9rS0tU1v"].concat(), "AIza123".into()),
            ("google-oauth-secret", ["GOCSP", "X-aB3dE5fG7hJ9kL1mN3pQ5rS7tU9v"].concat(), "GOCSPX-short".into()),
            ("npm-token", format!("npm_{alnum36}"), "npm_install".into()),
            ("pypi-token", format!("pypi-AgEIcHlwaS5vcmc{alnum36}{alnum36}"), "pypi-AgEshort".into()),
            ("huggingface-token", ["hf_aB", "3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3z"].concat(), "hf_hub".into()),
            ("sendgrid-key", ["SG.aB", "3dE5fG7hJ9kL1mN3pQ5r.aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9f"].concat(), "SG.short.x".into()),
            (
                "azure-storage-key",
                ["Defau", "ltEndpointsProtocol=https;AccountName=acct;AccountKey=aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9fG1hJ3kL=="].concat(),
                "AccountKey=short".into(),
            ),
            (
                "azure-sas-signature",
                ["https", "://acct.blob.core.windows.net/c?sv=2022-11-02&sig=aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3%3D"].concat(),
                "&sig=abc".into(),
            ),
            ("twilio-key", format!("SK{hex32}"), "SKU12345".into()),
            ("digitalocean-token", format!("dop_v1_{hex32}{hex32}"), "dop_v1_short".into()),
            ("databricks-token", format!("dapi{hex32}"), "dapi_docs".into()),
            ("shopify-token", format!("shpat_{hex32}"), "shpat_123".into()),
            ("dotenv-vault-key", format!("dotenv://:key_{hex32}"), "dotenv://:key_12".into()),
            (
                "jwt",
                ["eyJhbGciOiJIUzI1NiJ9.e", "yJzdWIiOiIxMjM0NTY3ODk", "wIn0.dozjgNryP4J3jVmNH", "l0w5N_XgL0n3I9PlFUP0TH", "sR8U"].concat(),
                "eyJ.eyJ.x".into(),
            ),
            ("bearer-token", ["Autho", "rization: Bearer aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat(), ["Autho", "rization: Bearer x"].concat()),
            ("url-credentials", ["postg", "res://admin:s3cretpass@db.internal:5432/app"].concat(), ["https", "://example.com/path"].concat()),
            ("assigned-secret", ["API_K", "EY=Zx9Lq2Wr8Tb4Np6M"].concat(), ["API_K", "EY=your_api_key_here"].concat()),
            ("secret-in-text", ["the s", "ecret is wJalrXUtnFEMIK7MDENGbPxRfiCYzQ8kP2mNq"].concat(), ["the s", "ecret is stored in Vault"].concat()),
            ("hex-secret", format!("the api key {hex32} works"), format!("commit {hex32} works")),
        ];
        for (rule, positive, negative) in &cases {
            assert!(rules(positive).contains(rule), "{rule} misses {positive}: {:?}", rules(positive));
            assert!(!rules(negative).contains(rule), "{rule} flags {negative}");
            let (red, n) = redact(positive);
            assert!(n >= 1 && red.contains("[redacted"), "{rule} not redacted: {red}");
        }
        let names: std::collections::BTreeSet<&str> = RULES.iter().map(|r| r.name).collect();
        let tested: std::collections::BTreeSet<&str> = cases.iter().map(|c| c.0).collect();
        assert_eq!(names, tested, "every rule has a case");
    }

    #[test]
    fn a_placeholder_hides_nothing_after_it_on_the_line() {
        let line = &format!("Setup: api_key: example-123456789 secret: {}{}", "wJalrXUtnFEMIK7MDENG", "bPxRfiCYzQ8kP2mNq");
        let f = scan(line, &[]);
        assert_eq!(f.iter().filter(|x| x.rule == "assigned-secret").count(), 1, "{f:?}");
        let two = format!("ghp_{0} and ghp_{0}", ["aB3dE5fG7hJ9kL1mN3pQ", "5rS7tU9vW1xY3zA5"].concat());
        assert_eq!(scan(&two, &[]).iter().filter(|x| x.rule == "github-token").count(), 2);
    }

    #[test]
    fn the_whole_private_key_block_is_redacted() {
        let text = "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA1b2c3d4\nZm9vYmFyYmF6cXV4\n-----END RSA PRIVATE KEY-----\nand after it.";
        let (red, _) = redact(text);
        assert_eq!(red, "Here is the key:\n[redacted:private-key]\nand after it.");
        // no END line: to the end of the paragraph
        let cut = "-----BEGIN PRIVATE KEY-----\nMIIEowIBAAKCAQEA1b2c3d4\nZm9vYmFyYmF6cXV4\n\nNext paragraph stays.";
        let (red, _) = redact(cut);
        assert!(!red.contains("MIIEow") && !red.contains("Zm9vYmFy") && red.ends_with("Next paragraph stays."), "{red}");
    }

    #[test]
    fn false_positive_corpus() {
        for ok in [
            "commit 9fceb02d0ae598e95dc970b74767f19372d61af8 fixed it",
            "id 123e4567-e89b-12d3-a456-426614174000",
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
            "\"integrity\": \"sha512-Fq7ByhX9t6F4Gn2xBYH5u3Hj8tg3Vj0JZvqXBUwH8x3mT7Z1ae8Qw\"",
            "The token budget is 1400 and secret management lives in Vault.",
            "Use std::fs::read_to_string and https://docs.rs/regex/1.11.1/regex/",
            "version 1.2.3 released 2026-10-29 at 09:30",
            "the cache key is the sha256 of the path",
            "SKU-1234 ships in 3 days",
        ] {
            assert!(scan(ok, &[]).is_empty(), "{ok}: {:?}", scan(ok, &[]));
        }
    }

    #[test]
    fn national_phones_and_ipv6_are_masked_versions_and_ids_stay() {
        let t = "call 0905 123 456 or (555) 123-4567 or 02/1234 5678; host fe80::1ff:fe23:4567:890a and 2001:db8:85a3::8a2e:370:7334";
        let (out, n) = redact_personal(t, &[]);
        for gone in ["0905 123 456", "(555) 123-4567", "02/1234 5678", "fe80::1ff", "2001:db8"] {
            assert!(!out.contains(gone), "{gone} survived: {out}");
        }
        assert_eq!(n, 5, "{out}");
        for kept in [
            "v1.2.3 and 0.12.3",
            "released 2026-10-29 at 09:30:00",
            "std::fs::read_to_string and a::b",
            "loopback ::1 stays",
            "order 0905123456 has no separators",
            "build 01.02.2026",
            "mac 00:1a:2b:3c:4d:5e",
            "ticket ACME-0905 123",
        ] {
            assert_eq!(redact_personal(kept, &[]).0, kept, "{kept}");
        }
    }

    #[test]
    fn invalid_patterns_fail_closed() {
        assert_eq!(compile_patterns(&["ACME-\\d+".into()]).unwrap().len(), 1);
        let errors = compile_patterns(&["\\bAcme Corp(\\b".into(), "ok".into()]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("'\\bAcme Corp(\\b': "), "{errors:?}");
    }

    #[test]
    fn private_lists_add_to_the_repository_lists() {
        let dir = std::env::temp_dir().join(format!("kontext-secrets-layers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let user = dir.join("user.toml");
        let shared = dir.join("kontext.toml");
        std::fs::write(&user, "[secrets]\nredact = [\"ACME-\\\\d+\"]\nallow = [\"FAKE-\"]\n").unwrap();
        std::fs::write(&shared, "[secrets]\nredact = [\"INTEG-\\\\d+\", \"(broken\"]\n").unwrap();
        let mut cfg = crate::config::Config::default();
        // what the merge leaves: the repository's list replaced the private one
        cfg.secrets.redact = vec!["INTEG-\\d+".into(), "(broken".into()];
        let loaded = crate::config::LoadedConfig {
            cfg,
            warnings: Vec::new(),
            shared_path: Some(shared.clone()),
            untrusted_adapters: None,
            layers: vec![user, shared],
        };
        assert_eq!(redact_list(&loaded), vec!["ACME-\\d+", "INTEG-\\d+", "(broken"]);
        assert_eq!(allow_list(&loaded), vec!["FAKE-"]);
        let errors = pattern_errors(&loaded);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("secrets.redact pattern '(broken'"), "{errors:?}");
        assert!(redaction(&loaded).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn urls_in_errors_lose_their_secrets() {
        assert_eq!(strip_url_secrets(&format!("https://{}@api.example.com/v1/search?key={}#frag", "user:pw", "abc123")), "https://api.example.com/v1/search");
        let webhook = strip_url_secrets(&["https://hooks.slack.com/services/", "T0000AAAA/B0000BBBB/aB3dE5fG7hJ9kL1mN3pQ5rS7"].concat());
        assert_eq!(webhook, "[redacted:slack-webhook]");
        let err = scrub_error(&format!("adapter 'x': GET https://api.example.com/q?api_key={} failed: HTTP 401", "Zx9Lq2Wr8Tb4Np6M"));
        assert_eq!(err, "adapter 'x': GET https://api.example.com/q failed: HTTP 401");
    }
}
