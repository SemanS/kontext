//! Phase 1 — deterministic repository scan: languages, manifests, stack, modules, dependencies.
use super::symbols;
use crate::glob::GlobSet;
use crate::repo::Repo;
use crate::util;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub generated_at: String,
    pub files: usize,
    pub source_files: usize,
    pub lines: usize,
    pub languages: Vec<LangStat>,
    pub manifests: Vec<Manifest>,
    pub workspace: Vec<String>,
    pub stack: Vec<String>,
    pub tooling: Vec<String>,
    pub ci: Vec<String>,
    pub modules: Vec<Module>,
    pub doc_dirs: Vec<String>,
    pub adr_dirs: Vec<AdrDir>,
    pub agent_files: Vec<String>,
    pub env_names: Vec<String>,
    pub submodules: Vec<String>,
    pub scripts: Vec<(String, String)>,
    pub readme_summary: Option<String>,
    pub root_description: Option<String>,
    /// Name from the root manifest (package.json, Cargo.toml, pyproject.toml), when it reads like a project name.
    pub root_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LangStat {
    pub lang: String,
    pub files: usize,
    pub lines: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub path: String,
    pub kind: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub deps: Vec<String>,
    pub dev_deps: Vec<String>,
    pub scripts: Vec<(String, String)>,
    pub local_deps: Vec<String>,
    pub project_type: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Module {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub manifests: Vec<String>,
    pub language: String,
    pub files: usize,
    pub lines: usize,
    pub tests: usize,
    pub description: Option<String>,
    pub docs: Vec<String>,
    pub key_files: Vec<(String, usize)>,
    pub symbols: Vec<String>,
    pub deps: Vec<String>,
    pub dependents: Vec<String>,
    pub ext_deps: Vec<String>,
    pub fingerprint: String,
    pub churn: usize,
    pub last_change: Option<String>,
    pub rank: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdrDir {
    pub path: String,
    pub count: usize,
    pub style: String,
    pub numbering: String,
}

pub fn language_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == "Dockerfile" || name.starts_with("Dockerfile.") {
        return Some("Dockerfile");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase())?;
    Some(match ext.as_str() {
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "rs" => "Rust",
        "py" | "pyi" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "scala" => "Scala",
        "swift" => "Swift",
        "rb" => "Ruby",
        "php" => "PHP",
        "cs" => "C#",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "C++",
        "dart" => "Dart",
        "ex" | "exs" => "Elixir",
        "lua" => "Lua",
        "sh" | "bash" | "zsh" => "Shell",
        "sql" => "SQL",
        "tf" | "hcl" => "Terraform",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "astro" => "Astro",
        "css" | "scss" | "sass" | "less" => "CSS",
        "html" | "htm" => "HTML",
        "md" | "mdx" => "Markdown",
        "json" | "jsonc" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "proto" => "Protobuf",
        "graphql" | "gql" => "GraphQL",
        _ => return None,
    })
}

pub fn is_code(lang: &str) -> bool {
    !matches!(lang, "Markdown" | "JSON" | "YAML" | "TOML" | "HTML" | "CSS" | "Dockerfile")
}

pub fn is_test_path(p: &str) -> bool {
    let l = p.to_lowercase();
    l.contains("/test/")
        || l.contains("/tests/")
        || l.contains("/__tests__/")
        || l.starts_with("test/")
        || l.starts_with("tests/")
        || l.contains(".test.")
        || l.contains(".spec.")
        || l.contains("_test.")
        || l.rsplit('/').next().is_some_and(|n| n.starts_with("test_"))
        || l.contains("/fixtures/")
        || l.contains("/__fixtures__/")
        || l.contains("/testdata/")
        || l.contains("/e2e/")
        || l.starts_with("e2e/")
        || l.contains("/__mocks__/")
        || l.contains("/mocks/")
        || l.contains("/stories/")
}

const DEFAULT_EXCLUDE: &[&str] = &[
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/target/**",
    "**/.next/**",
    "**/vendor/**",
    "**/__generated__/**",
    "**/generated/**",
    "**/*.min.js",
    "**/*.map",
    "**/*.lock",
    "**/package-lock.json",
    "**/*.snap",
    "**/*.d.ts",
    "**/.venv/**",
    "**/coverage/**",
];

const MARKERS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "setup.py",
    "go.mod",
    "project.json",
    "moon.yml",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
    "mix.exs",
    "deno.json",
];

type Labels = &'static [(&'static str, &'static str)];

const NPM_STACK: &[(&str, &str)] = &[
    ("next", "Next.js"),
    ("react", "React"),
    ("react-native", "React Native"),
    ("expo", "Expo"),
    ("vue", "Vue"),
    ("nuxt", "Nuxt"),
    ("svelte", "Svelte"),
    ("@sveltejs/kit", "SvelteKit"),
    ("@angular/core", "Angular"),
    ("astro", "Astro"),
    ("solid-js", "Solid"),
    ("express", "Express"),
    ("fastify", "Fastify"),
    ("@nestjs/core", "NestJS"),
    ("koa", "Koa"),
    ("hono", "Hono"),
    ("elysia", "Elysia"),
    ("firebase", "Firebase"),
    ("firebase-admin", "Firebase"),
    ("firebase-functions", "Firebase Functions"),
    ("@google-cloud/bigquery", "BigQuery"),
    ("@google-cloud/firestore", "Firestore"),
    ("@google-cloud/storage", "Cloud Storage"),
    ("@google-cloud/pubsub", "Pub/Sub"),
    ("@google-cloud/tasks", "Cloud Tasks"),
    ("@supabase/supabase-js", "Supabase"),
    ("@prisma/client", "Prisma"),
    ("prisma", "Prisma"),
    ("drizzle-orm", "Drizzle"),
    ("typeorm", "TypeORM"),
    ("mongoose", "MongoDB"),
    ("pg", "PostgreSQL"),
    ("mysql2", "MySQL"),
    ("better-sqlite3", "SQLite"),
    ("redis", "Redis"),
    ("ioredis", "Redis"),
    ("graphql", "GraphQL"),
    ("@trpc/server", "tRPC"),
    ("zod", "Zod"),
    ("@tanstack/react-query", "TanStack Query"),
    ("tailwindcss", "Tailwind CSS"),
    ("vite", "Vite"),
    ("webpack", "webpack"),
    ("electron", "Electron"),
    ("openai", "OpenAI SDK"),
    ("@anthropic-ai/sdk", "Anthropic SDK"),
    ("langchain", "LangChain"),
    ("ai", "Vercel AI SDK"),
    ("stripe", "Stripe"),
    ("telegraf", "Telegram bot"),
    ("grammy", "Telegram bot"),
    ("discord.js", "Discord bot"),
    ("puppeteer", "Puppeteer"),
    ("playwright", "Playwright"),
    ("bullmq", "BullMQ"),
    ("socket.io", "Socket.IO"),
    ("leaflet", "Leaflet"),
    ("maplibre-gl", "MapLibre"),
    ("mapbox-gl", "Mapbox"),
    ("@aws-sdk/client-s3", "AWS"),
    ("aws-sdk", "AWS"),
];
const NPM_TOOLING: &[(&str, &str)] = &[
    ("jest", "Jest"),
    ("vitest", "Vitest"),
    ("@playwright/test", "Playwright"),
    ("cypress", "Cypress"),
    ("mocha", "Mocha"),
    ("eslint", "ESLint"),
    ("prettier", "Prettier"),
    ("@biomejs/biome", "Biome"),
    ("typescript", "TypeScript compiler"),
    ("nx", "Nx"),
    ("turbo", "Turborepo"),
    ("lerna", "Lerna"),
    ("husky", "husky"),
    ("lint-staged", "lint-staged"),
    ("@swc/core", "SWC"),
    ("storybook", "Storybook"),
    ("@storybook/react", "Storybook"),
];
const CARGO_STACK: &[(&str, &str)] = &[
    ("tokio", "Tokio"),
    ("axum", "Axum"),
    ("actix-web", "Actix Web"),
    ("warp", "warp"),
    ("rocket", "Rocket"),
    ("tonic", "gRPC (tonic)"),
    ("sqlx", "SQLx"),
    ("diesel", "Diesel"),
    ("rusqlite", "SQLite"),
    ("sea-orm", "SeaORM"),
    ("tikv-client", "TiKV"),
    ("redis", "Redis"),
    ("reqwest", "reqwest"),
    ("serde", "Serde"),
    ("clap", "clap"),
    ("tantivy", "Tantivy"),
    ("tauri", "Tauri"),
    ("chromiumoxide", "Chromium CDP"),
    ("headless_chrome", "Chromium CDP"),
    ("scraper", "scraper"),
    ("wasm-bindgen", "WebAssembly"),
];
const PY_STACK: &[(&str, &str)] = &[
    ("django", "Django"),
    ("flask", "Flask"),
    ("fastapi", "FastAPI"),
    ("pydantic", "Pydantic"),
    ("sqlalchemy", "SQLAlchemy"),
    ("pandas", "pandas"),
    ("numpy", "NumPy"),
    ("torch", "PyTorch"),
    ("transformers", "Transformers"),
    ("langchain", "LangChain"),
    ("openai", "OpenAI SDK"),
    ("anthropic", "Anthropic SDK"),
    ("playwright", "Playwright"),
    ("scrapy", "Scrapy"),
    ("celery", "Celery"),
    ("httpx", "httpx"),
    ("requests", "requests"),
    ("typer", "Typer"),
    ("click", "Click"),
    ("mlx", "MLX"),
    ("ollama", "Ollama"),
];
const PY_TOOLING: &[(&str, &str)] = &[("pytest", "pytest"), ("ruff", "Ruff"), ("mypy", "mypy"), ("black", "Black")];

fn read_small(repo: &Repo, rel: &str, max: u64) -> Option<String> {
    let abs = repo.abs(rel);
    let md = std::fs::metadata(&abs).ok()?;
    if md.len() > max {
        return None;
    }
    std::fs::read_to_string(abs).ok()
}

/// Strip `//` and `/* */` comments and trailing commas so tsconfig-style JSONC parses.
pub fn strip_jsonc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    let re = regex::Regex::new(r",(\s*[}\]])").unwrap();
    re.replace_all(&out, "$1").into_owned()
}

fn parse_manifest(repo: &Repo, rel: &str) -> Option<Manifest> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let text = read_small(repo, rel, 2_000_000)?;
    let mut m = Manifest { path: rel.to_string(), ..Default::default() };
    match name {
        "package.json" => {
            let v: serde_json::Value = serde_json::from_str(&strip_jsonc(&text)).ok()?;
            m.kind = "npm".into();
            m.name = v.get("name").and_then(|x| x.as_str()).map(str::to_string);
            m.description = v.get("description").and_then(|x| x.as_str()).map(str::to_string).filter(|d| !d.is_empty());
            let keys =
                |k: &str| -> Vec<String> { v.get(k).and_then(|d| d.as_object()).map(|o| o.keys().cloned().collect()).unwrap_or_default() };
            m.deps = keys("dependencies");
            m.deps.extend(keys("peerDependencies"));
            m.dev_deps = keys("devDependencies");
            if let Some(s) = v.get("scripts").and_then(|s| s.as_object()) {
                m.scripts = s.iter().map(|(k, c)| (k.clone(), c.as_str().unwrap_or("").to_string())).collect();
            }
            for (k, ver) in v.get("dependencies").and_then(|d| d.as_object()).into_iter().flatten() {
                let ver = ver.as_str().unwrap_or("");
                if ver.starts_with("workspace:") || ver.starts_with("file:") || ver.starts_with("link:") {
                    m.local_deps.push(k.clone());
                }
            }
        }
        "Cargo.toml" => {
            let v: toml::Table = text.parse().ok()?;
            m.kind = "cargo".into();
            let pkg = v.get("package")?;
            m.name = pkg.get("name").and_then(|x| x.as_str()).map(str::to_string);
            m.description = pkg.get("description").and_then(|x| x.as_str()).map(str::to_string);
            for sect in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(t) = v.get(sect).and_then(|d| d.as_table()) {
                    for (k, spec) in t {
                        if sect == "dependencies" {
                            m.deps.push(k.clone());
                        } else {
                            m.dev_deps.push(k.clone());
                        }
                        if spec.get("path").is_some() {
                            m.local_deps.push(k.clone());
                        }
                        if spec.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                            m.local_deps.push(format!("workspace:{k}"));
                        }
                    }
                }
            }
            m.language = Some("Rust".into());
        }
        "pyproject.toml" => {
            let v: toml::Table = text.parse().ok()?;
            m.kind = "python".into();
            let proj = v.get("project");
            m.name = proj.and_then(|p| p.get("name")).and_then(|x| x.as_str()).map(str::to_string);
            m.description = proj.and_then(|p| p.get("description")).and_then(|x| x.as_str()).map(str::to_string);
            let dep_name =
                |s: &str| s.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.')).next().unwrap_or("").to_lowercase();
            if let Some(a) = proj.and_then(|p| p.get("dependencies")).and_then(|d| d.as_array()) {
                m.deps = a.iter().filter_map(|x| x.as_str()).map(dep_name).filter(|s| !s.is_empty()).collect();
            }
            if let Some(t) = v.get("tool").and_then(|t| t.get("poetry")).and_then(|p| p.get("dependencies")).and_then(|d| d.as_table()) {
                m.deps.extend(t.keys().map(|k| k.to_lowercase()).filter(|k| k != "python"));
            }
            if let Some(groups) = v.get("dependency-groups").and_then(|g| g.as_table()) {
                for (_, list) in groups {
                    if let Some(a) = list.as_array() {
                        m.dev_deps.extend(a.iter().filter_map(|x| x.as_str()).map(dep_name));
                    }
                }
            }
            m.language = Some("Python".into());
        }
        "setup.py" => {
            m.kind = "python".into();
            m.language = Some("Python".into());
        }
        "go.mod" => {
            m.kind = "go".into();
            m.language = Some("Go".into());
            for line in text.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("module ") {
                    m.name = Some(rest.trim().to_string());
                } else if !t.starts_with("//") && t.contains(" v") && !t.starts_with("go ") {
                    let dep = t.trim_start_matches("require").split_whitespace().next().unwrap_or("");
                    if dep.contains('.') {
                        m.deps.push(dep.to_string());
                    }
                }
            }
        }
        "project.json" => {
            let v: serde_json::Value = serde_json::from_str(&strip_jsonc(&text)).ok()?;
            m.kind = "nx".into();
            m.name = v.get("name").and_then(|x| x.as_str()).map(str::to_string);
            m.project_type = v.get("projectType").and_then(|x| x.as_str()).map(str::to_string);
            if let Some(deps) = v.get("implicitDependencies").and_then(|d| d.as_array()) {
                m.local_deps = deps.iter().filter_map(|d| d.as_str()).map(str::to_string).collect();
            }
        }
        "moon.yml" => {
            m.kind = "moon".into();
            let mut in_deps = false;
            for line in text.lines() {
                let t = line.trim_end();
                if let Some(rest) = t.strip_prefix("dependsOn:") {
                    in_deps = true;
                    let rest = rest.trim();
                    if rest.starts_with('[') {
                        m.local_deps.extend(crate::store::split_inline_list(rest.trim_matches(['[', ']'])));
                        in_deps = false;
                    }
                    continue;
                }
                if in_deps {
                    if let Some(item) = t.trim_start().strip_prefix("- ") {
                        let item = item.trim().trim_matches(['\'', '"']);
                        let item = item.strip_prefix("id:").map(str::trim).unwrap_or(item);
                        m.local_deps.push(item.trim_matches(['\'', '"']).to_string());
                        continue;
                    }
                    if !t.starts_with(' ') && !t.is_empty() {
                        in_deps = false;
                    }
                }
                if let Some(v) = t.strip_prefix("language:") {
                    m.language = Some(v.trim().trim_matches(['\'', '"']).to_string());
                }
                if let Some(v) = t.strip_prefix("type:").or_else(|| t.strip_prefix("layer:")) {
                    m.project_type = Some(v.trim().trim_matches(['\'', '"']).to_string());
                }
                if let Some(v) = t.trim_start().strip_prefix("description:")
                    && line.starts_with("  ")
                {
                    m.description = Some(v.trim().trim_matches(['\'', '"']).to_string());
                }
            }
        }
        "pom.xml" | "build.gradle" | "build.gradle.kts" => {
            m.kind = "jvm".into();
            let re = regex::Regex::new(r"<artifactId>([^<]+)</artifactId>").unwrap();
            m.name = re.captures(&text).map(|c| c[1].to_string());
        }
        other => m.kind = other.to_string(),
    }
    Some(m)
}

fn adr_style(repo: &Repo, files: &[&String]) -> (String, String) {
    let mut fm = 0;
    let mut fields = 0;
    for f in files.iter().take(5) {
        if let Some(t) = read_small(repo, f, 200_000) {
            if t.starts_with("---") {
                fm += 1;
            } else {
                fields += 1;
            }
        }
    }
    let numbered = files
        .iter()
        .filter(|f| {
            let n = f.rsplit('/').next().unwrap_or("");
            let d: String = n.chars().take_while(|c| c.is_ascii_digit()).collect();
            (3..=5).contains(&d.len()) && n[d.len()..].starts_with('-')
        })
        .count();
    let dated = files
        .iter()
        .filter(|f| {
            let n = f.rsplit('/').next().unwrap_or("");
            n.len() > 11 && chrono::NaiveDate::parse_from_str(&n[..10], "%Y-%m-%d").is_ok()
        })
        .count();
    let style = if fm >= fields { "frontmatter" } else { "fields" };
    let numbering = if numbered * 2 >= files.len() {
        "sequential"
    } else if dated * 2 >= files.len() {
        "date"
    } else {
        "none"
    };
    (style.into(), numbering.into())
}

pub fn scan(repo: &Repo, exclude_extra: &[String], min_module_files: usize, key_files_max: usize) -> Result<Inventory> {
    let mut inv = Inventory { generated_at: util::now_iso(), ..Default::default() };
    let tracked = repo.tracked_files()?;
    let mut excl: Vec<String> = DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect();
    excl.extend(exclude_extra.iter().cloned());
    let exclude = GlobSet::new(&excl);

    let mut files: Vec<(String, &'static str, usize, u64)> = Vec::new(); // path, lang, lines, bytes
    let mut all_paths: Vec<String> = Vec::new();
    for t in &tracked {
        if t.submodule {
            inv.submodules.push(t.path.clone());
            continue;
        }
        all_paths.push(t.path.clone());
        if exclude.matches(&t.path) || crate::secrets::is_sensitive_path(&t.path) {
            continue;
        }
        inv.files += 1;
        let Some(lang) = language_of(&t.path) else { continue };
        let abs = repo.abs(&t.path);
        let Ok(md) = std::fs::metadata(&abs) else { continue };
        let bytes = md.len();
        let lines = if bytes < 1_500_000 { std::fs::read(&abs).map(|b| b.iter().filter(|c| **c == b'\n').count()).unwrap_or(0) } else { 0 };
        files.push((t.path.clone(), lang, lines, bytes));
    }

    // languages
    let mut langs: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (_, lang, lines, _) in &files {
        let e = langs.entry(lang).or_default();
        e.0 += 1;
        e.1 += lines;
    }
    inv.languages = langs.into_iter().map(|(l, (f, n))| LangStat { lang: l.to_string(), files: f, lines: n }).collect();
    inv.languages.sort_by_key(|x| std::cmp::Reverse(x.lines));
    inv.source_files = files.iter().filter(|f| is_code(f.1)).count();
    inv.lines = files.iter().filter(|f| is_code(f.1)).map(|f| f.2).sum();

    // manifests
    let path_set: BTreeSet<&str> = all_paths.iter().map(String::as_str).collect();
    for p in &all_paths {
        let name = p.rsplit('/').next().unwrap_or(p);
        if MARKERS.contains(&name)
            && !exclude.matches(p)
            && !p.contains("/fixtures/")
            && !p.contains("/testdata/")
            && let Some(m) = parse_manifest(repo, p)
        {
            inv.manifests.push(m);
        }
    }

    // stack / tooling / workspace
    let mut stack: Vec<String> = Vec::new();
    let mut tooling: Vec<String> = Vec::new();
    let push = |v: &mut Vec<String>, s: &str| {
        if !v.iter().any(|x| x == s) {
            v.push(s.to_string());
        }
    };
    let mut dep_count: HashMap<String, usize> = HashMap::new();
    for m in &inv.manifests {
        for d in m.deps.iter().chain(m.dev_deps.iter()) {
            *dep_count.entry(d.to_lowercase()).or_default() += 1;
        }
        let (stack_map, tool_map): (Labels, Labels) = match m.kind.as_str() {
            "npm" => (NPM_STACK, NPM_TOOLING),
            "cargo" => (CARGO_STACK, &[]),
            "python" => (PY_STACK, PY_TOOLING),
            _ => (&[], &[]),
        };
        for d in &m.deps {
            if let Some((_, label)) = stack_map.iter().find(|(k, _)| k.eq_ignore_ascii_case(d)) {
                push(&mut stack, label);
            }
            if d.starts_with("@google-cloud/") || d == "googleapis" {
                push(&mut stack, "Google Cloud");
            }
        }
        for d in m.deps.iter().chain(m.dev_deps.iter()) {
            if let Some((_, label)) = tool_map.iter().find(|(k, _)| k.eq_ignore_ascii_case(d)) {
                push(&mut tooling, label);
            }
            if let Some((_, label)) = stack_map.iter().find(|(k, _)| k.eq_ignore_ascii_case(d))
                && m.dev_deps.contains(d)
                && matches!(*label, "Playwright" | "Puppeteer")
            {
                push(&mut tooling, label);
            }
        }
    }
    let has = |p: &str| path_set.contains(p);
    let any_suffix = |suffix: &str| all_paths.iter().any(|p| p.ends_with(suffix));
    let files_stack: &[(&str, &str, bool)] = &[
        ("firebase.json", "Firebase", true),
        ("netlify.toml", "Netlify", true),
        ("vercel.json", "Vercel", true),
        ("wrangler.toml", "Cloudflare Workers", true),
        ("fly.toml", "Fly.io", true),
        ("app.yaml", "App Engine", true),
        ("cloudbuild.yaml", "Cloud Build", true),
        ("serverless.yml", "Serverless Framework", true),
        ("Procfile", "Heroku", true),
    ];
    for (f, label, _) in files_stack {
        if has(f) {
            push(&mut stack, label);
        }
    }
    if all_paths.iter().any(|p| p.ends_with(".tf")) {
        push(&mut stack, "Terraform");
    }
    if all_paths.iter().any(|p| p.rsplit('/').next().is_some_and(|n| n == "Dockerfile" || n.starts_with("Dockerfile."))) {
        push(&mut stack, "Docker");
    }
    if any_suffix("docker-compose.yml") || any_suffix("docker-compose.yaml") || any_suffix("compose.yaml") {
        push(&mut stack, "Docker Compose");
    }
    if any_suffix("Chart.yaml") {
        push(&mut stack, "Helm");
    }
    if all_paths.iter().any(|p| p.starts_with("bigquery/") || p.ends_with(".bq.sql")) {
        push(&mut stack, "BigQuery");
    }
    let mut workspace = Vec::new();
    for (f, label) in [
        ("nx.json", "Nx"),
        (".moon/workspace.yml", "moon"),
        ("turbo.json", "Turborepo"),
        ("pnpm-workspace.yaml", "pnpm workspaces"),
        ("lerna.json", "Lerna"),
        ("go.work", "Go workspace"),
        ("bun.lock", "Bun"),
        ("bun.lockb", "Bun"),
        ("uv.lock", "uv"),
        ("deno.json", "Deno"),
    ] {
        if has(f) && !workspace.iter().any(|w| w == label) {
            workspace.push(label.to_string());
        }
    }
    if let Some(t) = read_small(repo, "Cargo.toml", 500_000)
        && t.contains("[workspace]")
    {
        workspace.push("Cargo workspace".into());
    }
    if let Some(root_pkg) = inv.manifests.iter().find(|m| m.path == "package.json") {
        if let Some(t) = read_small(repo, "package.json", 2_000_000)
            && t.contains("\"workspaces\"")
        {
            workspace.push("npm workspaces".into());
        }
        let preferred = ["dev", "start", "build", "test", "lint", "typecheck", "e2e", "deploy", "format"];
        let mut scripts: Vec<(String, String)> =
            root_pkg.scripts.iter().filter(|(k, _)| preferred.iter().any(|p| k == p || k.starts_with(&format!("{p}:")))).cloned().collect();
        scripts.sort_by_key(|(k, _)| preferred.iter().position(|p| k == p || k.starts_with(&format!("{p}:"))).unwrap_or(99));
        scripts.truncate(10);
        inv.scripts = scripts;
        inv.root_description = root_pkg.description.clone();
    }
    for m in &inv.manifests {
        if (m.path == "Cargo.toml" || m.path == "pyproject.toml") && inv.root_description.is_none() {
            inv.root_description = m.description.clone();
        }
    }
    for root_manifest in ["package.json", "Cargo.toml", "pyproject.toml", "go.mod"] {
        if let Some(n) = inv.manifests.iter().find(|m| m.path == root_manifest).and_then(|m| m.name.clone()) {
            let n = n.rsplit('/').next().unwrap_or(&n).to_string();
            let generic = matches!(n.as_str(), "root" | "monorepo" | "workspace" | "app" | "source" | "src" | "project" | "main");
            if !n.is_empty() && !n.starts_with('@') && !generic {
                inv.root_name = Some(n);
                break;
            }
        }
    }
    if has("components.json") {
        push(&mut tooling, "shadcn/ui");
    }
    inv.workspace = workspace;
    inv.stack = stack;
    inv.tooling = tooling;
    inv.ci = all_paths
        .iter()
        .filter(|p| p.starts_with(".github/workflows/") && (p.ends_with(".yml") || p.ends_with(".yaml")))
        .map(|p| p.trim_start_matches(".github/workflows/").to_string())
        .collect();
    if has(".gitlab-ci.yml") {
        inv.ci.push("GitLab CI".into());
    }

    // docs, ADRs, agent rules, env names
    let mut doc_dirs: BTreeSet<String> = BTreeSet::new();
    let mut adr_candidates: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for p in &all_paths {
        if !p.ends_with(".md") {
            continue;
        }
        let lower = p.to_lowercase();
        if let Some(rest) = lower.strip_prefix("docs/").or_else(|| lower.strip_prefix("doc/"))
            && let Some((dir, _)) = rest.split_once('/')
        {
            doc_dirs.insert(format!("{}/{dir}", &p[..p.find('/').unwrap()]));
        }
        let dir = p.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let dl = dir.to_lowercase();
        let last = dl.rsplit('/').next().unwrap_or("");
        if matches!(last, "adr" | "adrs" | "decisions" | "decision-records" | "architecture-decisions")
            && !crate::store::is_non_entry_name(p.rsplit('/').next().unwrap_or(""))
        {
            adr_candidates.entry(dir.to_string()).or_default().push(p);
        }
        let name = p.rsplit('/').next().unwrap_or("");
        if matches!(name, "AGENTS.md" | "CLAUDE.md" | "Claude.md" | "GEMINI.md") || p == ".github/copilot-instructions.md" {
            inv.agent_files.push(p.clone());
        }
    }
    for f in [".cursorrules", ".windsurfrules"] {
        if has(f) {
            inv.agent_files.push(f.to_string());
        }
    }
    for (dir, list) in adr_candidates {
        if dir.starts_with(".ai") {
            continue;
        }
        let (style, numbering) = adr_style(repo, &list);
        inv.adr_dirs.push(AdrDir { path: dir, count: list.len(), style, numbering });
    }
    inv.adr_dirs.sort_by_key(|x| std::cmp::Reverse(x.count));
    inv.doc_dirs = doc_dirs.into_iter().collect();
    for p in &all_paths {
        let name = p.rsplit('/').next().unwrap_or("");
        if name.starts_with(".env")
            && (name.ends_with(".example") || name.ends_with(".sample") || name.ends_with(".template"))
            && let Some(t) = read_small(repo, p, 200_000)
        {
            for line in t.lines() {
                let l = line.trim().trim_start_matches("export ");
                if l.starts_with('#') {
                    continue;
                }
                if let Some((k, _)) = l.split_once('=') {
                    let k = k.trim();
                    if !k.is_empty()
                        && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                        && !inv.env_names.iter().any(|x| x == k)
                    {
                        inv.env_names.push(k.to_string());
                    }
                }
            }
        }
    }
    inv.readme_summary =
        read_small(repo, "README.md", 2_000_000).map(|t| util::truncate_chars(&util::readme_intro(&t), 320)).filter(|s| !s.is_empty());

    // modules
    inv.modules = detect_modules(repo, &inv, &files, min_module_files, key_files_max);
    Ok(inv)
}

fn module_kind(path: &str, manifest_type: Option<&str>) -> String {
    if let Some(t) = manifest_type {
        let t = t.to_lowercase();
        if t.contains("app") {
            return "app".into();
        }
        if t.contains("lib") {
            return "lib".into();
        }
        if t.contains("tool") || t.contains("script") {
            return "tool".into();
        }
        if t.contains("test") {
            return "tests".into();
        }
    }
    let first = path.split('/').next().unwrap_or(path).to_lowercase();
    match first.as_str() {
        "apps" | "app" | "applications" => "app",
        "libs" | "lib" | "packages" | "pkg" | "crates" => "lib",
        "services" | "svc" | "functions" => "service",
        "modules" => "module",
        "plugins" | "extensions" => "plugin",
        "tools" | "scripts" | "bin" | "cmd" => "tool",
        "e2e" | "tests" | "test" | "integration" => "tests",
        "infra" | "infrastructure" | "deploy" | "terraform" | "k8s" | "helm" | "ops" => "infra",
        "examples" | "example" | "samples" | "demo" => "example",
        "docs" | "doc" => "docs",
        _ => "module",
    }
    .into()
}

fn detect_modules(
    repo: &Repo,
    inv: &Inventory,
    files: &[(String, &'static str, usize, u64)],
    min_files: usize,
    key_max: usize,
) -> Vec<Module> {
    // 1. candidate roots from manifests
    let mut roots: BTreeMap<String, Vec<&Manifest>> = BTreeMap::new();
    for m in &inv.manifests {
        let dir = m.path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        if dir.is_empty() {
            continue;
        }
        if m.kind == "cargo" && m.name.is_none() {
            continue;
        }
        roots.entry(dir).or_default().push(m);
    }
    let code_files: Vec<&(String, &'static str, usize, u64)> = files.iter().filter(|f| is_code(f.1)).collect();
    let owner = |roots: &BTreeMap<String, Vec<&Manifest>>, p: &str| -> Option<String> {
        let mut best: Option<&String> = None;
        for r in roots.keys() {
            if p.starts_with(&format!("{r}/")) && best.is_none_or(|b| r.len() > b.len()) {
                best = Some(r);
            }
        }
        best.cloned()
    };
    // 2. top-level directories with code that no manifest root covers become modules too
    let mut uncovered: BTreeMap<String, usize> = BTreeMap::new();
    for f in &code_files {
        if owner(&roots, &f.0).is_none()
            && let Some((top, rest)) = f.0.split_once('/')
        {
            let key = if matches!(top, "src" | "packages" | "apps" | "libs" | "modules" | "services" | "crates" | "plugins") {
                match rest.split_once('/') {
                    Some((second, _)) => format!("{top}/{second}"),
                    None => top.to_string(),
                }
            } else {
                top.to_string()
            };
            *uncovered.entry(key).or_default() += 1;
        }
    }
    let mut plain_dirs: Vec<String> = Vec::new();
    for (dir, n) in uncovered {
        if n >= min_files.max(1) {
            plain_dirs.push(dir.clone());
            roots.entry(dir).or_default();
        }
    }
    // 2b. very large modules are split into areas (one level below the module, or below its src/);
    //     an area that is still very large is split once more
    let mut area_dirs: Vec<String> = Vec::new();
    for _pass in 0..2 {
        let mut per_root: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for f in &code_files {
            if let Some(o) = owner(&roots, &f.0) {
                per_root.entry(o).or_default().push(f.0.as_str());
            }
        }
        for (root, list) in per_root {
            if list.len() < 150 {
                continue;
            }
            // descend through wrapper directories (src/, src/lib/, …) that hold most of the files
            let mut base = root.clone();
            for _ in 0..3 {
                let mut child: BTreeMap<String, usize> = BTreeMap::new();
                for p in &list {
                    if let Some(rest) = p.strip_prefix(&format!("{base}/"))
                        && let Some((seg, _)) = rest.split_once('/')
                    {
                        *child.entry(seg.to_string()).or_default() += 1;
                    }
                }
                match child.into_iter().max_by_key(|(_, n)| *n) {
                    Some((seg, n)) if n * 10 >= list.len() * 8 => base = format!("{base}/{seg}"),
                    _ => break,
                }
            }
            let mut groups: BTreeMap<String, usize> = BTreeMap::new();
            for p in &list {
                if let Some(rest) = p.strip_prefix(&format!("{base}/"))
                    && let Some((seg, _)) = rest.split_once('/')
                {
                    *groups.entry(format!("{base}/{seg}")).or_default() += 1;
                }
            }
            for (dir, n) in groups {
                if n >= 8 && n * 10 < list.len() * 9 && !is_test_path(&format!("{dir}/")) && !roots.contains_key(&dir) {
                    area_dirs.push(dir.clone());
                    plain_dirs.push(dir.clone());
                    roots.entry(dir).or_default();
                }
            }
        }
    }
    // 3. assign files, drop tiny modules (their files fall back to the parent)
    loop {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for f in &code_files {
            if let Some(o) = owner(&roots, &f.0) {
                *counts.entry(o).or_default() += 1;
            }
        }
        // declared projects (a manifest) stay with a single code file; inferred directories need `min_files`
        let tiny: Vec<String> = roots
            .keys()
            .filter(|r| {
                let n = counts.get(*r).copied().unwrap_or(0);
                if plain_dirs.contains(r) { n < min_files.max(1) } else { n == 0 }
            })
            .cloned()
            .collect();
        if tiny.is_empty() {
            break;
        }
        for t in tiny {
            roots.remove(&t);
        }
    }
    // name lookups for dependency resolution: manifest names (all languages) and directory names
    // (only for languages whose imports name local packages by directory, e.g. Python, Rust, Go)
    let mut by_pkg_name: Vec<(String, String)> = Vec::new(); // (package name, module path)
    let mut by_dir_name: Vec<(String, String)> = Vec::new();
    for (dir, ms) in &roots {
        for m in ms {
            if let Some(n) = &m.name {
                by_pkg_name.push((n.clone(), dir.clone()));
                if m.kind == "cargo" || m.kind == "python" {
                    by_pkg_name.push((n.replace('-', "_"), dir.clone()));
                }
            }
        }
        let last = dir.rsplit('/').next().unwrap_or(dir);
        by_dir_name.push((last.replace('-', "_"), dir.clone()));
    }
    by_pkg_name.sort_by_key(|x| std::cmp::Reverse(x.0.len()));
    by_dir_name.sort_by_key(|x| std::cmp::Reverse(x.0.len()));
    let aliases = ts_path_aliases(repo);
    let owners: HashMap<&str, String> = code_files.iter().filter_map(|f| owner(&roots, &f.0).map(|o| (f.0.as_str(), o))).collect();

    let mut modules: Vec<Module> = Vec::new();
    let mut deps_by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (dir, ms) in &roots {
        let my_files: Vec<&&(String, &'static str, usize, u64)> =
            code_files.iter().filter(|f| owners.get(f.0.as_str()) == Some(dir)).collect();
        if my_files.is_empty() {
            continue;
        }
        let mut lang_lines: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &my_files {
            *lang_lines.entry(f.1).or_default() += f.2.max(1);
        }
        let language = lang_lines.iter().max_by_key(|(_, n)| **n).map(|(l, _)| l.to_string()).unwrap_or_default();
        let manifest_type = ms.iter().find_map(|m| m.project_type.clone());
        let name = ms.iter().find_map(|m| m.name.clone()).unwrap_or_else(|| dir.clone());
        let mut description = ms.iter().find_map(|m| m.description.clone()).filter(|d| !d.trim().is_empty() && !util::is_boilerplate(d));
        let mut docs = Vec::new();
        for doc in ["README.md", "AGENTS.md", "CLAUDE.md", "readme.md"] {
            let p = format!("{dir}/{doc}");
            // case-insensitive file systems report README.md and readme.md as the same file
            if docs.iter().any(|d: &String| d.eq_ignore_ascii_case(&p)) {
                continue;
            }
            if repo.abs(&p).is_file() {
                // READMEs describe; AGENTS.md / CLAUDE.md are rules and stay out of the description
                if description.is_none()
                    && doc.eq_ignore_ascii_case("readme.md")
                    && let Some(t) = read_small(repo, &p, 500_000)
                {
                    let para = util::readme_intro(&t);
                    if !para.is_empty() {
                        description = Some(util::truncate_chars(&para, 220));
                    }
                }
                docs.push(p);
            }
        }
        // no manifest description and no README: the package's own doc comment
        if description.is_none() {
            description = code_doc(repo, dir).filter(|d| !util::is_boilerplate(d)).map(|d| util::truncate_chars(&d, 220));
        }
        // key files + symbols + imports
        let mut scored: Vec<(f32, &String, usize, FileSym)> = Vec::new();
        let mut imports_all: Vec<(String, String)> = Vec::new(); // (importing file, spec)
        let tests = my_files.iter().filter(|f| is_test_path(&f.0)).count();
        for f in my_files.iter().filter(|f| !is_test_path(&f.0)).take(600) {
            let text = if f.3 < 400_000 { std::fs::read_to_string(repo.abs(&f.0)).unwrap_or_default() } else { String::new() };
            let facts = symbols::extract(f.1, &text);
            for imp in &facts.imports {
                imports_all.push((f.0.clone(), imp.clone()));
            }
            let fname = f.0.rsplit('/').next().unwrap_or("").to_lowercase();
            let stem = fname.split('.').next().unwrap_or("");
            let entry_bonus = if matches!(
                stem,
                "index"
                    | "main"
                    | "lib"
                    | "mod"
                    | "app"
                    | "server"
                    | "cli"
                    | "__init__"
                    | "handler"
                    | "handlers"
                    | "router"
                    | "routes"
                    | "api"
            ) {
                3.0
            } else {
                0.0
            };
            let depth = f.0.matches('/').count() as f32;
            let score = entry_bonus + (facts.symbols.len().min(20) as f32) * 0.3 + ((f.2 + 1) as f32).ln() * 0.6 - depth * 0.15;
            scored.push((score, &f.0, f.2, FileSym(facts.symbols)));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let key_files: Vec<(String, usize)> = scored.iter().take(key_max.max(3)).map(|(_, p, l, _)| ((*p).clone(), *l)).collect();
        let mut symbols_out: Vec<String> = Vec::new();
        for (_, _, _, syms) in scored.iter().take(12) {
            for s in &syms.0 {
                if !symbols_out.contains(s) {
                    symbols_out.push(s.clone());
                }
                if symbols_out.len() >= 24 {
                    break;
                }
            }
        }
        // dependencies
        let mut deps: BTreeSet<String> = BTreeSet::new();
        for m in ms.iter() {
            for d in &m.local_deps {
                let d = d.trim_start_matches("workspace:");
                if let Some((_, target)) =
                    by_pkg_name.iter().chain(by_dir_name.iter()).find(|(n, _)| n.replace('-', "_") == d.replace('-', "_"))
                    && target != dir
                {
                    deps.insert(target.clone());
                }
            }
            for d in m.deps.iter() {
                if let Some((_, target)) = by_pkg_name.iter().find(|(n, _)| n == d && n.contains(['@', '/', '-', '_']))
                    && target != dir
                {
                    deps.insert(target.clone());
                }
            }
        }
        for (file, spec) in &imports_all {
            let resolved: Option<String> = if spec.starts_with('.') {
                let base = file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                let joined = normalize_join(base, spec);
                owner(&roots, &format!("{joined}/x")).or_else(|| owner(&roots, &joined))
            } else if let Some((alias, target)) = aliases.iter().find(|(a, _)| {
                spec == a
                    || spec.starts_with(&format!("{}/", a.trim_end_matches('/')))
                    || (a.ends_with('/') && spec.starts_with(a.as_str()))
            }) {
                let rest = spec[alias.len().min(spec.len())..].trim_start_matches('/');
                let p = if rest.is_empty() { target.clone() } else { format!("{}/{rest}", target.trim_end_matches('/')) };
                owner(&roots, &format!("{p}/x")).or_else(|| owner(&roots, &p))
            } else {
                let head = spec.split("::").next().unwrap_or(spec);
                let matches =
                    |(n, _): &&(String, String)| head == n || head.starts_with(&format!("{n}/")) || head.starts_with(&format!("{n}."));
                let js = matches!(language_of(file), Some("TypeScript" | "JavaScript" | "Vue" | "Svelte" | "Astro"));
                by_pkg_name
                    .iter()
                    .find(matches)
                    .or_else(|| if js { None } else { by_dir_name.iter().find(matches) })
                    .map(|(_, t)| t.clone())
            };
            if let Some(t) = resolved
                && &t != dir
            {
                deps.insert(t);
            }
        }
        let mut ext: Vec<String> =
            ms.iter().flat_map(|m| m.deps.iter().cloned()).filter(|d| !by_pkg_name.iter().any(|(n, _)| n == d)).collect();
        ext.sort();
        ext.dedup();
        ext.truncate(20);
        let fp_src: String = my_files.iter().map(|f| format!("{}:{};", f.0, f.3 / 256)).collect();
        let kind = if area_dirs.contains(dir) {
            "area".to_string()
        } else if plain_dirs.contains(dir) {
            module_kind(dir, None)
        } else {
            module_kind(dir, manifest_type.as_deref())
        };
        deps_by_module.insert(dir.clone(), deps.clone());
        modules.push(Module {
            id: format!("mod-{}", util::slugify(dir, 70)),
            path: dir.clone(),
            name,
            kind,
            manifests: ms.iter().map(|m| m.path.clone()).collect(),
            language,
            files: my_files.len(),
            lines: my_files.iter().map(|f| f.2).sum(),
            tests,
            description,
            docs,
            key_files,
            symbols: symbols_out,
            deps: deps.into_iter().collect(),
            dependents: Vec::new(),
            ext_deps: ext,
            fingerprint: format!("{:012x}", util::fnv1a64(fp_src.as_bytes()) & 0xffff_ffff_ffff),
            churn: 0,
            last_change: None,
            rank: 0,
        });
    }
    // dependents
    let snapshot: Vec<(String, Vec<String>)> = modules.iter().map(|m| (m.path.clone(), m.deps.clone())).collect();
    for m in modules.iter_mut() {
        m.dependents = snapshot.iter().filter(|(_, d)| d.contains(&m.path)).map(|(p, _)| p.clone()).collect();
    }
    modules
}

struct FileSym(Vec<String>);

fn normalize_join(base: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for seg in rel.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// `compilerOptions.paths` aliases from the root tsconfig files: (alias prefix, target dir).
fn ts_path_aliases(repo: &Repo) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in ["tsconfig.base.json", "tsconfig.json", "jsconfig.json"] {
        let Some(text) = read_small(repo, f, 1_000_000) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(&text)) else { continue };
        let base_url = v
            .pointer("/compilerOptions/baseUrl")
            .and_then(|b| b.as_str())
            .unwrap_or(".")
            .trim_start_matches("./")
            .trim_end_matches('/')
            .to_string();
        if let Some(paths) = v.pointer("/compilerOptions/paths").and_then(|p| p.as_object()) {
            for (alias, targets) in paths {
                let Some(t) = targets.as_array().and_then(|a| a.first()).and_then(|t| t.as_str()) else { continue };
                let alias = alias.trim_end_matches('*').to_string();
                let mut target = t.trim_start_matches("./").trim_end_matches('*').trim_end_matches('/').to_string();
                if !base_url.is_empty() && base_url != "." {
                    target = format!("{base_url}/{target}");
                }
                // `libs/x/src/index.ts` → the directory
                if target.ends_with(".ts") || target.ends_with(".tsx") || target.ends_with(".js") {
                    target = target.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or(target);
                }
                if !alias.is_empty() {
                    out.push((alias, target));
                }
            }
        }
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.0.len()));
    out
}

pub fn rank_modules(modules: &mut [Module]) {
    let mut scored: Vec<(f32, usize)> = modules
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let kind_bonus = match m.kind.as_str() {
                "app" | "service" => 4.0,
                "lib" | "module" | "plugin" | "area" => 1.5,
                "tool" | "infra" => 0.5,
                "tests" | "example" | "docs" => -2.0,
                _ => 0.0,
            };
            let s = kind_bonus
                + ((m.lines + 1) as f32).ln() * 0.8
                + ((m.dependents.len() + 1) as f32).ln() * 2.0
                + ((m.churn + 1) as f32).ln() * 0.6
                + if m.docs.is_empty() { 0.0 } else { 0.5 };
            (s, i)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (rank, (_, i)) in scored.into_iter().enumerate() {
        modules[i].rank = rank + 1;
    }
}

/// A package's doc comment: a Python `__init__.py` docstring, Rust `//!` crate docs, Go's `doc.go`.
fn code_doc(repo: &Repo, dir: &str) -> Option<String> {
    let read = |rel: &str| read_small(repo, &format!("{dir}/{rel}"), 200_000);
    if let Some(t) = read("__init__.py") {
        let t = t.trim_start_matches('\u{feff}');
        let body: String = t.lines().skip_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
        for q in ["\"\"\"", "\'\'\'"] {
            if let Some(rest) = body.strip_prefix(q)
                && let Some(end) = rest.find(q)
            {
                let p = util::first_paragraph(&rest[..end], 400);
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
    }
    for rel in ["src/lib.rs", "src/main.rs", "lib.rs"] {
        if let Some(t) = read(rel) {
            let doc: Vec<&str> = t
                .lines()
                .skip_while(|l| l.trim().is_empty() || l.trim_start().starts_with("#!["))
                .take_while(|l| l.trim_start().starts_with("//!"))
                .map(|l| l.trim_start().trim_start_matches("//!").trim())
                .collect();
            let p = util::first_paragraph(&doc.join("\n"), 400);
            if !p.is_empty() {
                return Some(p);
            }
        }
    }
    if let Some(t) = read("doc.go") {
        let doc: Vec<&str> =
            t.lines().take_while(|l| l.trim_start().starts_with("//")).map(|l| l.trim_start().trim_start_matches("//").trim()).collect();
        let p = util::first_paragraph(&doc.join("\n"), 400);
        if !p.is_empty() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc() {
        let s = "{ // comment\n \"a\": \"http://x\", /* block */ \"b\": [1,2,],\n}";
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc(s)).unwrap();
        assert_eq!(v["a"], "http://x");
        assert_eq!(v["b"][1], 2);
    }

    #[test]
    fn joins() {
        assert_eq!(normalize_join("apps/web/src", "../lib/x"), "apps/web/lib/x");
        assert_eq!(normalize_join("a", "./b"), "a/b");
    }

    #[test]
    fn kinds_and_tests() {
        assert_eq!(module_kind("apps/web", None), "app");
        assert_eq!(module_kind("libs/backend/core", Some("library")), "lib");
        assert!(is_test_path("apps/web/src/a.spec.ts"));
        assert!(is_test_path("tests/test_x.py"));
        assert!(!is_test_path("src/testing.rs"));
    }
}
