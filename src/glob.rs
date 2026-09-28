//! Minimal git-style globs for `sources.include`, entry `paths` and secret allowlists.
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Glob {
    re: Regex,
}

impl Glob {
    pub fn new(pattern: &str) -> Option<Glob> {
        Regex::new(&glob_to_regex(pattern)).ok().map(|re| Glob { re })
    }

    pub fn matches(&self, path: &str) -> bool {
        self.re.is_match(path)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GlobSet {
    globs: Vec<Glob>,
}

impl GlobSet {
    pub fn new<S: AsRef<str>>(patterns: &[S]) -> GlobSet {
        GlobSet { globs: patterns.iter().filter_map(|p| Glob::new(p.as_ref())).collect() }
    }

    pub fn matches(&self, path: &str) -> bool {
        self.globs.iter().any(|g| g.matches(path))
    }
}

fn has_glob_chars(p: &str) -> bool {
    p.contains(['*', '?', '[', '{'])
}

/// Does an entry's `paths` pattern cover `path`? Plain paths match exactly or as a directory prefix.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let pat = pattern.trim().trim_start_matches("./").trim_start_matches('/');
    if pat.is_empty() {
        return false;
    }
    if !has_glob_chars(pat) {
        let pat = pat.trim_end_matches('/');
        return path == pat || path.starts_with(&format!("{pat}/"));
    }
    // `dir/**` (the shape of most entry paths) is a plain prefix test
    if let Some(dir) = pat.strip_suffix("/**")
        && !has_glob_chars(dir)
    {
        return path == dir || path.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/'));
    }
    compiled(pat).is_some_and(|re| re.is_match(path))
}

/// Compiled globs, kept for the process: callers test one pattern against hundreds of paths.
fn compiled(pat: &str) -> Option<Regex> {
    static CACHE: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Option<Regex>>>> =
        std::sync::LazyLock::new(Default::default);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() > 4096 {
        cache.clear();
    }
    cache.entry(pat.to_string()).or_insert_with(|| Regex::new(&glob_to_regex(pat)).ok()).clone()
}

/// Does `path` (a file or directory being asked about) overlap the pattern in either direction?
pub fn path_overlaps(pattern: &str, path: &str) -> bool {
    if path_matches(pattern, path) {
        return true;
    }
    let base = pattern.trim().trim_start_matches("./").split(['*', '?', '[', '{']).next().unwrap_or("").trim_end_matches('/');
    !base.is_empty() && base.starts_with(&format!("{}/", path.trim_end_matches('/')))
}

pub fn glob_to_regex(pattern: &str) -> String {
    let mut pat = pattern.trim().trim_start_matches("./").to_string();
    let anchored = pat.starts_with('/');
    if anchored {
        pat.remove(0);
    }
    if pat.ends_with('/') {
        pat.push_str("**");
    }
    // "dir/**" matches the directory itself and everything below it
    let dir_suffix = pat.len() > 3 && pat.ends_with("/**");
    if dir_suffix {
        pat.truncate(pat.len() - 3);
    }
    let mut re = String::from("^");
    if !anchored && !pat.contains('/') && pat != "**" {
        re.push_str("(?:.*/)?");
    }
    let chars: Vec<char> = pat.chars().collect();
    let mut i = 0;
    let mut in_brace = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    let at_start = i == 0 || chars[i - 1] == '/';
                    let slash_after = i + 2 < chars.len() && chars[i + 2] == '/';
                    if at_start && slash_after {
                        re.push_str("(?:.*/)?");
                        i += 3;
                        continue;
                    }
                    re.push_str(".*");
                    i += 2;
                    continue;
                }
                re.push_str("[^/]*");
            }
            '?' => re.push_str("[^/]"),
            '[' => {
                let mut j = i + 1;
                let mut class = String::from("[");
                if j < chars.len() && chars[j] == '!' {
                    class.push('^');
                    j += 1;
                }
                while j < chars.len() && chars[j] != ']' {
                    if chars[j] == '\\' {
                        class.push('\\');
                    }
                    class.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() {
                    class.push(']');
                    re.push_str(&class);
                    i = j + 1;
                    continue;
                }
                re.push_str("\\[");
            }
            '{' => {
                in_brace = true;
                re.push_str("(?:");
            }
            '}' if in_brace => {
                in_brace = false;
                re.push(')');
            }
            ',' if in_brace => re.push('|'),
            _ => re.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    if dir_suffix {
        re.push_str("(?:/.*)?");
    }
    re.push('$');
    re
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        let g = |p: &str, s: &str| Glob::new(p).unwrap().matches(s);
        assert!(g("docs/**/*.md", "docs/a.md"));
        assert!(g("docs/**/*.md", "docs/x/y/a.md"));
        assert!(!g("docs/**/*.md", "src/docs/a.md"));
        assert!(g("README.md", "README.md"));
        assert!(g("README.md", "apps/web/README.md"));
        assert!(!g("/README.md", "apps/web/README.md"));
        assert!(g("**/AGENTS.md", "AGENTS.md"));
        assert!(g("**/AGENTS.md", "modules/gsc/AGENTS.md"));
        assert!(g("apps/*/src/**", "apps/web/src/a/b.ts"));
        assert!(g("apps/web/**", "apps/web"));
        assert!(g("*.{ts,tsx}", "a/b/c.tsx"));
        assert!(!g("*.{ts,tsx}", "a/b/c.rs"));
        assert!(g(".env*", "apps/api/.env.local"));
        assert!(g("src/", "src/main.rs"));
    }

    #[test]
    fn entry_paths() {
        assert!(path_matches("apps/billing-jobs", "apps/billing-jobs/src/main.ts"));
        assert!(path_matches("apps/billing-jobs/", "apps/billing-jobs/src/main.ts"));
        assert!(!path_matches("apps/billing-jobs", "apps/billing-jobs-2/x.ts"));
        assert!(path_matches("apps/*/src/**", "apps/web/src/x.ts"));
        // the `dir/**` shortcut agrees with the compiled glob
        for path in ["libs/cache", "libs/cache/src/lib.rs", "libs/cachex/a.rs", "libs", "x/libs/cache/a.rs", "libs/cache/"] {
            assert_eq!(path_matches("libs/cache/**", path), Glob::new("libs/cache/**").unwrap().matches(path), "{path}");
        }
        assert!(path_matches("libs/cache/**", "libs/cache/src/lib.rs") && !path_matches("libs/cache/**", "libs/cachex/a.rs"));
        assert!(path_overlaps("apps/web/src/**", "apps/web"));
        assert!(!path_overlaps("apps/web/src/**", "apps/api"));
    }
}
