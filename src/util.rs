use anyhow::{Context, Result};
use std::path::Path;

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Cheap token estimate (~4 chars per token) used for budgeting agent-facing output.
pub fn est_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lowercase ASCII slug, with Central European diacritics folded.
pub fn slugify(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in s.chars() {
        // "team's" → "teams", not "team-s"
        if matches!(ch, '\'' | '’' | '`') {
            continue;
        }
        let folded = fold_char(ch);
        for c in folded.chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c.to_ascii_lowercase());
                dash = false;
            } else if !dash && !out.is_empty() {
                out.push('-');
                dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > max {
        out.truncate(max);
        if let Some(pos) = out.rfind('-')
            && pos > max / 2
        {
            out.truncate(pos);
        }
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

fn fold_char(c: char) -> String {
    let s = match c {
        'á' | 'ä' | 'à' | 'â' | 'ã' | 'å' | 'ą' => "a",
        'Á' | 'Ä' | 'À' | 'Â' | 'Ã' | 'Å' | 'Ą' => "a",
        'č' | 'ć' | 'ç' | 'Č' | 'Ć' | 'Ç' => "c",
        'ď' | 'Ď' => "d",
        'é' | 'ě' | 'è' | 'ê' | 'ë' | 'ę' | 'É' | 'Ě' | 'È' | 'Ê' | 'Ë' | 'Ę' => "e",
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => "i",
        'ľ' | 'ĺ' | 'ł' | 'Ľ' | 'Ĺ' | 'Ł' => "l",
        'ň' | 'ń' | 'ñ' | 'Ň' | 'Ń' | 'Ñ' => "n",
        'ó' | 'ô' | 'ö' | 'ò' | 'õ' | 'ő' | 'ø' | 'Ó' | 'Ô' | 'Ö' | 'Ò' | 'Õ' | 'Ő' | 'Ø' => "o",
        'ŕ' | 'ř' | 'Ŕ' | 'Ř' => "r",
        'š' | 'ś' | 'Š' | 'Ś' => "s",
        'ť' | 'Ť' => "t",
        'ú' | 'ů' | 'ü' | 'ù' | 'û' | 'ű' | 'Ú' | 'Ů' | 'Ü' | 'Ù' | 'Û' | 'Ű' => "u",
        'ý' | 'ÿ' | 'Ý' => "y",
        'ž' | 'ź' | 'ż' | 'Ž' | 'Ź' | 'Ż' => "z",
        'ß' => "ss",
        _ => return c.to_string(),
    };
    s.to_string()
}

/// FNV-1a 64-bit: stable across Rust versions, used for fingerprints and keys (not security).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn short_hash(s: &str) -> String {
    format!("{:012x}", fnv1a64(s.as_bytes()) & 0xffff_ffff_ffff)
}

pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!("{}.tmp{}", path.extension().and_then(|e| e.to_str()).unwrap_or(""), std::process::id()));
    std::fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

/// Plain text from a Markdown fragment: drops heading marks, emphasis, code ticks and link targets.
pub fn strip_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let t = line.trim_start_matches(['#', '>', ' ']).trim();
        let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
        out.push_str(t);
        out.push(' ');
    }
    let re_link = regex_link();
    let s = re_link.replace_all(&out, "$1");
    one_line(&s.replace("**", "").replace("__", "").replace('`', ""))
}

fn regex_link() -> &'static regex::Regex {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
    &RE
}

/// Paragraphs of Markdown prose (headings, badges, front matter, HTML, tables and code skipped), each as one line.
pub fn paragraphs(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut in_code = false;
    let flush = |para: &mut Vec<String>, out: &mut Vec<String>| {
        if !para.is_empty() {
            out.push(strip_md(&para.join(" ")));
            para.clear();
        }
    };
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let skip = t.starts_with('#')
            || t.starts_with("<!--")
            || t.starts_with('<')
            || t.starts_with("[![")
            || t.starts_with("![")
            || t.starts_with('|')
            || t.starts_with("---")
            || t.starts_with("**Status")
            || t.starts_with("**Date");
        if t.is_empty() || skip {
            flush(&mut para, &mut out);
            continue;
        }
        para.push(t.to_string());
    }
    flush(&mut para, &mut out);
    out
}

/// First meaningful paragraph of Markdown (skipping headings, badges, front matter, HTML), as one line.
pub fn first_paragraph(md: &str, max: usize) -> String {
    paragraphs(md).into_iter().next().map(|p| truncate_chars(&p, max)).unwrap_or_default()
}

/// Text that generators put into READMEs — useless as a description.
pub fn is_boilerplate(p: &str) -> bool {
    let l = p.to_lowercase();
    let t = l.trim();
    let imperative = [
        "run ",
        "to run",
        "use ",
        "install ",
        "npm ",
        "npx ",
        "yarn ",
        "pnpm ",
        "bun ",
        "cargo ",
        "make ",
        "see ",
        "click ",
        "follow ",
        "read ",
        "more about",
        "learn more",
    ];
    t.len() < 25
        || t.ends_with(':')
        || imperative.iter().any(|w| t.starts_with(w))
        || [
            "generated with nx",
            "was generated with",
            "bootstrapped with",
            "to run the dev server",
            "getting started",
            "this template",
            "welcome to your",
            "run `nx",
            "npx nx",
            "nx generate",
            "create-react-app",
            "vite + react",
            "this project was",
            "table of contents",
            "todo",
            "lorem ipsum",
            "work in progress",
            "add your description",
            "nx test",
            "nx build",
            "nx serve",
            "nx run",
            "nx graph",
            "execute the unit tests",
            "these targets are",
            "inferred automatically",
            "to build the library",
            "nx.dev",
            "nx console",
            "✨",
            "smart monorepo",
            "powered by nx",
        ]
        .iter()
        .any(|b| t.contains(b))
}

/// The descriptive part of a README: the text between the title and the first `## ` heading, or a
/// section named Overview / About / Introduction / Purpose. Empty when the README opens with how-tos.
pub fn readme_intro(md: &str) -> String {
    let mut intro = String::new();
    let mut named = String::new();
    let mut section: Option<String> = None; // None = before the first ##
    let mut in_code = false;
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_code = !in_code;
        }
        if !in_code && t.starts_with("## ") {
            section = Some(t[3..].trim().to_lowercase());
            continue;
        }
        match &section {
            None => {
                intro.push_str(line);
                intro.push('\n');
            }
            Some(h) if named.is_empty() || named.ends_with("\u{0}") => {
                let wanted = ["overview", "about", "introduction", "intro", "purpose", "what is", "description", "what it does"];
                if wanted.iter().any(|w| h.starts_with(w)) {
                    named.push_str(line);
                    named.push('\n');
                }
            }
            _ => {}
        }
    }
    let p = meaningful_paragraph(&intro, 400);
    if !p.is_empty() {
        return p;
    }
    meaningful_paragraph(&named, 400)
}

/// First paragraph that says something (skips generator boilerplate and lead-ins).
pub fn meaningful_paragraph(md: &str, max: usize) -> String {
    for p in paragraphs(md).into_iter().take(8) {
        if !is_boilerplate(&p) {
            return truncate_chars(&p, max);
        }
        // "X is the one client of a process. Its users are:" — the sentences before the lead-in say it
        if let Some(lead) = before_lead_in(&p) {
            return truncate_chars(&lead, max);
        }
    }
    String::new()
}

/// The sentences of a paragraph that ends in a lead-in (`…:`), when they say something on their own.
fn before_lead_in(p: &str) -> Option<String> {
    let body = p.trim().strip_suffix(':')?;
    let cut = body.rfind(". ")?;
    let lead = body[..=cut].trim();
    (!is_boilerplate(lead)).then(|| lead.to_string())
}

pub fn first_sentence(s: &str, max: usize) -> String {
    let s = one_line(s);
    let mut end = s.len();
    for (i, _) in s.match_indices(". ") {
        if i > 20 {
            end = i + 1;
            break;
        }
    }
    truncate_chars(&s[..end], max)
}

pub fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub fn which(cmd: &str) -> Option<std::path::PathBuf> {
    if cmd.contains('/') {
        let p = std::path::PathBuf::from(cmd);
        return p.is_file().then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(cmd)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_folds_diacritics() {
        assert_eq!(slugify("Použiť Tantivy namiesto sqlite-vec!", 60), "pouzit-tantivy-namiesto-sqlite-vec");
        assert_eq!(slugify("  --Hello__World--  ", 60), "hello-world");
        assert_eq!(slugify("a very long title that keeps going and going", 20), "a-very-long-title");
        assert_eq!(slugify("The team's source of truth", 60), "the-teams-source-of-truth");
    }

    #[test]
    fn descriptions_survive_lead_ins() {
        let readme = "# Cache\n\n`Cache` is the only client of the key-value store in a process. Jobs, sessions and uploads share\nits connection but own separate key prefixes:\n\n| Domain | Crate |\n|---|---|\n| jobs | `libs/jobs` |\n";
        assert_eq!(meaningful_paragraph(readme, 200), "Cache is the only client of the key-value store in a process.");
        // a lead-in alone says nothing (nor does a stub list); the next paragraph does
        assert_eq!(
            meaningful_paragraph("# X\n\nIt provides:\n\n- a\n\nThe store of record for stays.", 200),
            "The store of record for stays."
        );
        assert_eq!(first_paragraph("```\ncode\n```\n\nProse here.", 100), "Prose here.");
    }

    #[test]
    fn paragraphs_and_sentences() {
        let md = "# Title\n\n[![badge](x)](y)\n\nThis is **the** [first](http://x) paragraph.\nStill first.\n\nSecond.";
        assert_eq!(first_paragraph(md, 200), "This is the first paragraph. Still first.");
        assert_eq!(first_sentence("Short one. And more text here that goes on.", 100), "Short one. And more text here that goes on.");
        assert_eq!(
            first_sentence("A sentence that is long enough to split. Second sentence.", 100),
            "A sentence that is long enough to split."
        );
    }

    #[test]
    fn tokens_and_truncate() {
        assert_eq!(est_tokens("abcd"), 1);
        assert_eq!(est_tokens("abcde"), 2);
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
    }
}
