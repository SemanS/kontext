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
/// A pattern with glob characters is first taken literally, as git does, so `app/[slug]/page.tsx`
/// names its own file whether or not it exists on disk; `\[` escapes a bracket for the glob form.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let pat = pattern.trim().trim_start_matches("./").trim_start_matches('/');
    if pat.is_empty() {
        return false;
    }
    let path = path.trim_start_matches("./").trim_start_matches('/');
    if !has_glob_chars(pat) {
        let pat = pat.trim_end_matches('/');
        return path == pat || path.starts_with(&format!("{pat}/"));
    }
    // literal first: brackets and braces in real file names (Next.js routes)
    let lit = unescape(pat);
    let lit = lit.trim_end_matches('/');
    let lit = lit.strip_suffix("/**").unwrap_or(lit);
    if !lit.is_empty() && (path == lit || path.starts_with(&format!("{lit}/"))) {
        return true;
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

/// The pattern with its `\x` escapes resolved: what it names when taken literally.
fn unescape(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let mut chars = p.chars();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(n) = chars.next()
        {
            out.push(n);
            continue;
        }
        out.push(c);
    }
    out
}

/// Does `path` (a file or directory being asked about) overlap the pattern in either direction:
/// the pattern covers it, or the directory could contain a match (`src/db` for `**/migrations/*.sql`
/// or `src/**/*.sql`). A path whose last segment has an extension is taken as a file, which
/// contains nothing. A leading `/` is stripped, as `path_matches` does.
pub fn path_overlaps(pattern: &str, path: &str) -> bool {
    if path_matches(pattern, path) {
        return true;
    }
    let path = path.trim().trim_start_matches("./").trim_start_matches('/').trim_end_matches('/');
    let a = Side::pattern(pattern);
    if a.segs.is_empty() {
        return false;
    }
    if path.is_empty() || path == "." {
        return true;
    }
    let b = Side::literal(path);
    unify(&a, &a.segs, &b, &b.segs)
}

/// Could one path exist that both patterns cover, or does one name a directory the other reaches
/// into? For a focus that is itself a glob (`src/adapters/**`) against an entry's paths.
pub fn patterns_overlap(a: &str, b: &str) -> bool {
    let (x, y) = (Side::pattern(a), Side::pattern(b));
    if x.segs.is_empty() || y.segs.is_empty() {
        return false;
    }
    unify(&x, &x.segs, &y, &y.segs)
}

/// Whether `p` has glob characters at all (an escaped one counts: it needs the glob reading).
pub fn is_glob(p: &str) -> bool {
    has_glob_chars(p)
}

/// One side of an overlap test: a pattern's or a path's segments.
struct Side<'a> {
    segs: Vec<&'a str>,
    /// a real path: its segments are names, never globs
    literal: bool,
    /// the last segment names a file (it has an extension), which contains nothing
    file: bool,
}

impl<'a> Side<'a> {
    fn pattern(p: &'a str) -> Side<'a> {
        let t = p.trim().trim_start_matches("./");
        let anchored = t.starts_with('/');
        let t = t.trim_start_matches('/').trim_end_matches('/');
        let mut segs: Vec<&str> = t.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
        // `*.md` and `README.md` match at any depth, as `glob_to_regex` reads them
        if !anchored && segs.len() == 1 && segs[0] != "**" {
            segs.insert(0, "**");
        }
        let file = segs.last().is_some_and(|s| file_like(s));
        Side { segs, literal: false, file }
    }

    fn literal(p: &'a str) -> Side<'a> {
        let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
        let file = segs.last().is_some_and(|s| file_like(s));
        Side { segs, literal: true, file }
    }
}

/// `retry.rs`, `*.sql`: a name with an extension. `.github` and `src` are not.
fn file_like(seg: &str) -> bool {
    match seg.rfind('.') {
        Some(i) if i > 0 => {
            let ext = &seg[i + 1..];
            !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

fn unify(sa: &Side, a: &[&str], sb: &Side, b: &[&str]) -> bool {
    let only_any = |s: &[&str]| s.iter().all(|x| *x == "**");
    match (a.first(), b.first()) {
        (None, None) => true,
        // one side ends: it names a directory the other reaches into, unless it is a file
        (None, Some(_)) => only_any(b) || !sa.file,
        (Some(_), None) => only_any(a) || !sb.file,
        (Some(&"**"), _) if !sa.literal => unify(sa, &a[1..], sb, b) || unify(sa, a, sb, &b[1..]),
        (_, Some(&"**")) if !sb.literal => unify(sa, a, sb, &b[1..]) || unify(sa, &a[1..], sb, b),
        (Some(x), Some(y)) => segment_overlap(x, sa.literal, y, sb.literal) && unify(sa, &a[1..], sb, &b[1..]),
    }
}

/// Can one name satisfy both segments? Literally first, then as globs.
fn segment_overlap(x: &str, x_literal: bool, y: &str, y_literal: bool) -> bool {
    let lx = if x_literal { x.to_string() } else { unescape(x) };
    let ly = if y_literal { y.to_string() } else { unescape(y) };
    if lx == ly {
        return true;
    }
    let gx = !x_literal && has_glob_chars(x);
    let gy = !y_literal && has_glob_chars(y);
    let matches = |pat: &str, name: &str| compiled(&format!("/{pat}")).is_some_and(|re| re.is_match(name));
    match (gx, gy) {
        (false, false) => false,
        (true, false) => matches(x, &ly),
        (false, true) => matches(y, &lx),
        // two globs: compatible literal heads are enough to say they may meet
        (true, true) => {
            let head = |s: &str| s.split(['*', '?', '[', '{']).next().unwrap_or("").replace('\\', "");
            let (hx, hy) = (head(x), head(y));
            hx.starts_with(&hy) || hy.starts_with(&hx)
        }
    }
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
            // `\[` is a literal bracket
            '\\' if i + 1 < chars.len() => {
                re.push_str(&regex::escape(&chars[i + 1].to_string()));
                i += 2;
                continue;
            }
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

    #[test]
    fn a_directory_overlaps_a_pattern_with_an_early_wildcard() {
        // `**/x`: any directory could hold it, a file that is not it cannot
        assert!(path_overlaps("**/migrations/*.sql", "src/db"));
        assert!(path_overlaps("**/migrations/*.sql", "src/db/migrations"));
        assert!(path_overlaps("**/migrations/*.sql", "src/db/migrations/001.sql"));
        assert!(!path_overlaps("**/migrations/*.sql", "src/db/schema.rs"));
        assert!(!path_overlaps("**/migrations/*.sql", "src/db/migrations/notes.md"));
        // `a/**/b`
        assert!(path_overlaps("src/**/*.sql", "src/db"));
        assert!(path_overlaps("src/**/mod.rs", "src/adapters"));
        assert!(!path_overlaps("src/**/mod.rs", "docs"));
        assert!(!path_overlaps("src/**/mod.rs", "src/adapters/lib.rs"));
        // `/a/**`: the leading slash is stripped both ways
        assert!(path_overlaps("/src/adapters/**", "src"));
        assert!(path_overlaps("/src/adapters/**", "src/adapters/mod.rs"));
        assert!(path_overlaps("src/adapters/**", "/src"));
        assert!(!path_overlaps("/src/adapters/**", "docs"));
        // a slashless name matches at any depth
        assert!(path_overlaps("*.sql", "src/db"));
        assert!(!path_overlaps("*.sql", "src/db/schema.rs"));
    }

    #[test]
    fn brackets_are_literal_first() {
        // a Next.js route names its own file, existing or not
        assert!(path_matches("app/[slug]/page.tsx", "app/[slug]/page.tsx"));
        assert!(path_matches("app/[slug]/**", "app/[slug]/page.tsx"));
        assert!(path_matches("app/[slug]", "app/[slug]/page.tsx"));
        assert!(path_overlaps("app/[slug]/page.tsx", "app"));
        assert!(path_overlaps("app/[slug]/page.tsx", "app/[slug]"));
        // and still a class as a glob
        assert!(path_matches("app/[slug]/page.tsx", "app/s/page.tsx"));
        // escaped, it is only literal
        assert!(path_matches("app/\\[slug\\]/page.tsx", "app/[slug]/page.tsx"));
        assert!(!path_matches("app/\\[slug\\]/page.tsx", "app/s/page.tsx"));
        // a real path's brackets are names, not a class
        assert!(!path_overlaps("app/s/page.tsx", "app/[slug]"));
    }

    #[test]
    fn two_patterns_overlap() {
        assert!(patterns_overlap("src/adapters/**", "src/adapters/**"));
        assert!(patterns_overlap("src/adapters/mod.rs", "src/adapters/**"));
        assert!(patterns_overlap("src/**", "src/adapters/**"));
        assert!(patterns_overlap("**/mod.rs", "src/adapters/**"));
        assert!(!patterns_overlap("docs/**", "src/adapters/**"));
        assert!(!patterns_overlap("src/*.rs", "src/adapters/*.ts"));
        assert!(patterns_overlap("src/db/*.sql", "src/*/*.sql"));
    }
}
