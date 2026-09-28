//! Secret detection and redaction for anything that may become shared knowledge.
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
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |name, pat: &str, severity| Rule { name, re: Regex::new(pat).expect("secret rule"), severity };
    vec![
        r("private-key", r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY(?: BLOCK)?-----", Severity::High),
        r("gcp-service-account", r#""private_key"\s*:\s*"-----BEGIN"#, Severity::High),
        r("aws-access-key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", Severity::High),
        r("github-token", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b", Severity::High),
        r("github-pat", r"\bgithub_pat_[A-Za-z0-9_]{40,}\b", Severity::High),
        r("gitlab-token", r"\bglpat-[A-Za-z0-9_-]{20,}\b", Severity::High),
        r("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b", Severity::High),
        r("anthropic-key", r"\bsk-ant-[A-Za-z0-9_-]{20,}", Severity::High),
        r("openai-key", r"\bsk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{32,}", Severity::High),
        r("stripe-key", r"\b(?:sk|rk)_live_[0-9A-Za-z]{20,}\b", Severity::High),
        r("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b", Severity::Medium),
        r("dotenv-vault-key", r"dotenv://:key_[0-9a-f]{32,}", Severity::High),
        r("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", Severity::Medium),
        r("bearer-token", r"(?i)\bauthorization:\s*bearer\s+[A-Za-z0-9._~+/-]{20,}", Severity::High),
        r("url-credentials", r"\b[a-z][a-z0-9+.-]*://[^/\s:@]{1,64}:[^/\s:@]{6,128}@[^\s/]+", Severity::High),
        r(
            "assigned-secret",
            r#"(?i)\b[A-Z0-9_]*(?:api[_-]?key|secret|token|passw(?:or)?d|client[_-]?secret|access[_-]?key)[A-Z0-9_]*\b\s*[:=]\s*["']?([A-Za-z0-9/+_=.\-]{12,})"#,
            Severity::Medium,
        ),
    ]
});

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

pub fn scan(text: &str, allow: &[Regex]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.contains("kontext:allow-secret") || allow.iter().any(|a| a.is_match(line)) {
            continue;
        }
        for rule in RULES.iter() {
            if let Some(m) = rule.re.captures(line) {
                let value = m.get(1).map(|g| g.as_str()).unwrap_or_else(|| m.get(0).unwrap().as_str());
                if rule.name == "assigned-secret" && looks_like_placeholder(value) {
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
        if rule.name == "assigned-secret" {
            let replaced = rule.re.replace_all(&out, |c: &regex::Captures| {
                let whole = c.get(0).unwrap().as_str();
                let value = c.get(1).map(|g| g.as_str()).unwrap_or("");
                if looks_like_placeholder(value) {
                    whole.to_string()
                } else {
                    n += 1;
                    whole.replace(value, "[redacted]")
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
}
