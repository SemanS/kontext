//! Cheap, regex-based structure extraction (exports and imports). Precise code intelligence is the
//! job of code adapters (LSP / code graphs); this only feeds module summaries and dependency edges.
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Default, Clone)]
pub struct FileFacts {
    pub symbols: Vec<String>,
    pub imports: Vec<String>,
}

struct Pat {
    re: Regex,
    label: &'static str,
    group: usize,
}

fn p(re: &str, label: &'static str, group: usize) -> Pat {
    Pat { re: Regex::new(re).expect("symbol regex"), label, group }
}

static TS_SYMBOLS: LazyLock<Vec<Pat>> = LazyLock::new(|| {
    vec![
        p(r"(?m)^\s*export\s+(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)", "fn", 1),
        p(r"(?m)^\s*export\s+(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)", "class", 1),
        p(r"(?m)^\s*export\s+(?:declare\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)", "const", 1),
        p(r"(?m)^\s*export\s+(?:declare\s+)?(?:interface|type)\s+([A-Za-z_$][\w$]*)", "type", 1),
        p(r"(?m)^\s*export\s+(?:declare\s+)?(?:const\s+)?enum\s+([A-Za-z_$][\w$]*)", "enum", 1),
    ]
});
static TS_IMPORTS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r#"(?m)^\s*import\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?['"]([^'"]+)['"]"#).unwrap(),
        Regex::new(r#"(?m)^\s*export\s+[^'"]*?\s+from\s+['"]([^'"]+)['"]"#).unwrap(),
        Regex::new(r#"\brequire\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
        Regex::new(r#"\bimport\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
    ]
});
static PY_SYMBOLS: LazyLock<Vec<Pat>> =
    LazyLock::new(|| vec![p(r"(?m)^(?:async\s+)?def\s+([A-Za-z][\w]*)", "def", 1), p(r"(?m)^class\s+([A-Za-z][\w]*)", "class", 1)]);
static PY_IMPORTS: LazyLock<Vec<Regex>> =
    LazyLock::new(|| vec![Regex::new(r"(?m)^\s*from\s+([\w.]+)\s+import").unwrap(), Regex::new(r"(?m)^\s*import\s+([\w.]+)").unwrap()]);
static RS_SYMBOLS: LazyLock<Vec<Pat>> = LazyLock::new(|| {
    vec![
        p(r"(?m)^\s*pub(?:\([^)]*\))?\s+(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_]\w*)", "fn", 1),
        p(r"(?m)^\s*pub(?:\([^)]*\))?\s+(?:struct|enum|trait|type)\s+([A-Za-z_]\w*)", "type", 1),
    ]
});
static RS_IMPORTS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![Regex::new(r"(?m)^\s*(?:pub\s+)?use\s+([A-Za-z_]\w*)::").unwrap()]);
static GO_SYMBOLS: LazyLock<Vec<Pat>> = LazyLock::new(|| {
    vec![p(r"(?m)^func\s+(?:\([^)]*\)\s*)?([A-Z]\w*)", "func", 1), p(r"(?m)^type\s+([A-Z]\w*)\s+(?:struct|interface)", "type", 1)]
});
static GO_IMPORTS: LazyLock<Vec<Regex>> =
    LazyLock::new(|| vec![Regex::new(r#"(?m)^\s*(?:import\s+)?(?:[\w.]+\s+)?"([\w.\-/]+)"\s*$"#).unwrap()]);
static JVM_SYMBOLS: LazyLock<Vec<Pat>> = LazyLock::new(|| {
    vec![p(
        r"(?m)^\s*(?:public\s+)?(?:final\s+|abstract\s+|sealed\s+|data\s+|open\s+)*(?:class|interface|enum|record|object)\s+([A-Z]\w*)",
        "class",
        1,
    )]
});
static JVM_IMPORTS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![Regex::new(r"(?m)^\s*import\s+([\w.]+)").unwrap()]);

pub fn extract(lang: &str, text: &str) -> FileFacts {
    let (syms, imps): (&[Pat], &[Regex]) = match lang {
        "TypeScript" | "JavaScript" | "Vue" | "Svelte" | "Astro" => (&TS_SYMBOLS, &TS_IMPORTS),
        "Python" => (&PY_SYMBOLS, &PY_IMPORTS),
        "Rust" => (&RS_SYMBOLS, &RS_IMPORTS),
        "Go" => (&GO_SYMBOLS, &GO_IMPORTS),
        "Java" | "Kotlin" | "Scala" => (&JVM_SYMBOLS, &JVM_IMPORTS),
        _ => return FileFacts::default(),
    };
    let mut f = FileFacts::default();
    for pat in syms {
        for c in pat.re.captures_iter(text) {
            if let Some(m) = c.get(pat.group) {
                let name = m.as_str();
                if lang == "Python" && name.starts_with('_') {
                    continue;
                }
                f.symbols.push(format!("{} {name}", pat.label));
                if f.symbols.len() >= 40 {
                    break;
                }
            }
        }
    }
    for re in imps {
        for c in re.captures_iter(text) {
            if let Some(m) = c.get(1) {
                f.imports.push(m.as_str().to_string());
            }
        }
    }
    f.imports.sort();
    f.imports.dedup();
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts() {
        let src = "import { a } from './a';\nimport type { B } from '@org/lib-b';\nexport async function computeTotal() {}\nexport class OrderService {}\nexport const X = 1;\nconst y = require('lodash');\n";
        let f = extract("TypeScript", src);
        assert_eq!(f.symbols, vec!["fn computeTotal", "class OrderService", "const X"]);
        assert_eq!(f.imports, vec!["./a", "@org/lib-b", "lodash"]);
    }

    #[test]
    fn rust_and_python() {
        let rs = "use storage_core::Store;\npub struct Parser;\npub(crate) fn run() {}\nfn private() {}\n";
        let f = extract("Rust", rs);
        assert_eq!(f.symbols, vec!["fn run", "type Parser"]);
        assert_eq!(f.imports, vec!["storage_core"]);
        let py = "from shop.io import read\nimport os\ndef main():\n    pass\ndef _hidden(): pass\nclass Agent:\n    pass\n";
        let f = extract("Python", py);
        assert_eq!(f.symbols, vec!["def main", "class Agent"]);
        assert_eq!(f.imports, vec!["os", "shop.io"]);
    }
}
