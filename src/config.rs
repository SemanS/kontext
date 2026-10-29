//! Layered configuration.
//!
//! Layers, later wins (tables are deep-merged, arrays replaced):
//!   1. built-in defaults (`DEFAULTS` below)
//!   2. user global      `~/.config/kontext/config.toml`          (personal adapters: trusted)
//!   3. user per repo    `~/.config/kontext/repos/<slug>.toml`    (trusted)
//!   4. repo shared      `<root>/.kontext.toml` or `<root>/.ai/kontext.toml` (committed; adapters need `kontext trust`)
//!   5. clone local      `<git-common-dir>/kontext/config.toml`  (never committed: trusted)
//!
//! The shared layer is untrusted input. Its `adapters` load only while `trust.toml` pins
//! `sha256:<hex>` of its `adapters`, `vars` and `project.name` together (an old 12-hex FNV pin is
//! refused with a warning, never upgraded); its `[vars]` never override a var a private layer
//! (2, 3, 5) set for an adapter a private layer declares; its `project.name` must be a slug; and
//! `[verify] pass_env` is read from the user layers (2, 3) only.
use crate::repo::Repo;
use crate::util;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DEFAULTS: &str = r#"
[store]
dir = ".ai"
style = "frontmatter"
index_file = true
max_body_lines = 80

[store.kinds.decision]
dir = "decisions"
numbering = "date"
trailer = "Decision"

[store.kinds.convention]
dir = "conventions"
trailer = "Convention"

[store.kinds.learning]
dir = "learnings"
trailer = "Learning"

[store.kinds.incident]
dir = "incidents"
trailer = "Incident"

[store.kinds.architecture]
dir = "architecture"

[sources]
include = ["/*.md", "**/README.md", "**/AGENTS.md", "**/CLAUDE.md", "docs/**/*.md", "doc/**/*.md", "adr/**/*.md"]
exclude = ["**/node_modules/**", "**/CHANGELOG.md", "**/vendor/**", "**/fixtures/**", "**/testdata/**", "**/.github/ISSUE_TEMPLATE/**"]
max_bytes = 300000

[index]
commits = true
max_commits = 5000

[brief]
budget_tokens = 1400
max_decisions = 12
max_modules = 14

[freshness]
threshold_commits = 20

[search]
adapters = "always"
min_local_hits = 3

[capture]
default_visibility = "team"
redact = true

[hooks]
validate = true
trailers = true
remind_inbox = true

[secrets]
scan = "store"
allow = []
redact = []

[init]
max_module_docs = 40
min_module_files = 3
jobs = 2
task_max_files = 12
task_inline_chars = 24000
max_decision_tasks = 20
history_max_commits = 3000
exclude = []
"#;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Config {
    pub project: ProjectCfg,
    pub store: StoreCfg,
    pub sources: SourcesCfg,
    pub index: IndexCfg,
    pub brief: BriefCfg,
    pub freshness: FreshnessCfg,
    pub search: SearchCfg,
    pub capture: CaptureCfg,
    pub hooks: HooksCfg,
    pub secrets: SecretsCfg,
    pub init: InitCfg,
    pub verify: VerifyCfg,
    pub vars: BTreeMap<String, String>,
    pub adapters: BTreeMap<String, AdapterCfg>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ProjectCfg {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    #[default]
    Frontmatter,
    Fields,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Numbering {
    #[default]
    Date,
    Sequential,
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct StoreCfg {
    pub dir: String,
    pub style: Style,
    pub index_file: bool,
    pub max_body_lines: usize,
    pub kinds: BTreeMap<String, KindCfg>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct KindCfg {
    /// Directory relative to `store.dir`.
    pub dir: Option<String>,
    /// Directory relative to the repository root (wins over `dir`), e.g. an existing `docs/adr`.
    pub path: Option<String>,
    pub style: Option<Style>,
    pub numbering: Option<Numbering>,
    /// Commit trailer key added by the prepare-commit-msg hook, e.g. `Decision`.
    pub trailer: Option<String>,
    /// Default review date for new entries of this kind, relative to their date: `90d`, `12w`,
    /// `6m` (months of 30 days) or `1y`. Unset (the default): no review date.
    pub review_after: Option<String>,
}

/// Days in a `review_after` period (`90d`, `12w`, `6m`, `1y`).
pub fn period_days(text: &str) -> Option<i64> {
    let t = text.trim();
    let (n, unit) = t.split_at(t.find(|c: char| !c.is_ascii_digit())?);
    let n: i64 = n.parse().ok().filter(|n| *n > 0)?;
    Some(
        n * match unit.trim() {
            "d" => 1,
            "w" => 7,
            "m" => 30,
            "y" => 365,
            _ => return None,
        },
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SourcesCfg {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct IndexCfg {
    pub commits: bool,
    pub max_commits: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct BriefCfg {
    pub budget_tokens: usize,
    pub max_decisions: usize,
    pub max_modules: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct FreshnessCfg {
    /// Flag an active decision once this many commits touched its `paths` since its date (0 = off).
    pub threshold_commits: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SearchCfg {
    /// When `ctx_search` asks the adapters with a search op: "always", or "fallback" — only when the
    /// local sources found fewer than `min_local_hits` hits (or the caller asks for them).
    pub adapters: String,
    pub min_local_hits: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CaptureCfg {
    pub default_visibility: String,
    pub redact: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct HooksCfg {
    pub validate: bool,
    pub trailers: bool,
    pub remind_inbox: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SecretsCfg {
    /// "store" (only knowledge files), "staged" (every staged text file) or "off".
    pub scan: String,
    pub allow: Vec<String>,
    /// Extra values (regexes) masked in agent threads and what is distilled from them — a client's
    /// name, an integration id. Best kept in a private config layer.
    pub redact: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct VerifyCfg {
    /// Environment variables a `verify` command sees on top of PATH, HOME, USER, LANG, LC_*,
    /// TMPDIR and TERM. Read from the user layers only; a repository's value is ignored.
    pub pass_env: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct InitCfg {
    pub max_module_docs: usize,
    pub min_module_files: usize,
    /// Adapter (with an `llm` op) used by `kontext init --deepen` when no agent drives it.
    pub llm: Option<String>,
    pub jobs: usize,
    pub task_max_files: usize,
    pub task_inline_chars: usize,
    pub max_decision_tasks: usize,
    pub history_max_commits: usize,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct AdapterCfg {
    /// "mcp" | "http" | "command"
    pub driver: String,
    pub enabled: Option<bool>,
    pub description: Option<String>,
    /// Activation conditions; an adapter whose conditions fail is silently skipped.
    pub when: WhenCfg,
    pub vars: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    /// Result weight when merging search hits from several sources.
    pub weight: Option<f32>,
    // --- process drivers (mcp over stdio, command) ---
    pub command: Option<Vec<String>>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    // --- network drivers (mcp over streamable HTTP, http) ---
    pub url: Option<String>,
    pub base_url: Option<String>,
    pub headers: BTreeMap<String, String>,
    // --- MCP federation ---
    /// "none" (default), "all", or a list of tool names to re-expose through kontext's own MCP server.
    pub expose: Option<Value>,
    pub prefix: Option<String>,
    /// Capability operations: search, read, store, history, code, brief, llm, health.
    pub ops: BTreeMap<String, OpCfg>,
    /// Event subscriptions (capture, promote, sync).
    pub on: Vec<OnCfg>,
    /// Tools declared purely in config, backed by one of this adapter's calls.
    pub tools: Vec<ToolCfg>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct WhenCfg {
    /// Executable that must be on PATH.
    pub command: Option<String>,
    /// Path (relative to the repo root) that must exist.
    pub file: Option<String>,
    /// Regex that the repo id (`github.com/org/name`) must match.
    pub repo: Option<String>,
    /// Environment variable that must be set.
    pub env: Option<String>,
    /// Path that must exist (templated; `*` allowed in the last segment), e.g. a per-repo opt-in file.
    pub exists: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct OpCfg {
    // mcp
    pub tool: Option<String>,
    pub args: Option<Value>,
    // http
    pub method: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub query: BTreeMap<String, String>,
    pub body: Option<Value>,
    pub headers: BTreeMap<String, String>,
    // command
    pub command: Option<Vec<String>>,
    pub stdin: Option<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    /// Command writes its answer to `{{output_file}}` instead of stdout.
    pub output_file: bool,
    /// Output format: "auto" (default), "json", "jsonl", "lines", "text".
    pub format: Option<String>,
    // result mapping
    /// Path to the list of result items, e.g. `result.*[*]`.
    pub items: Option<String>,
    /// Split a text result into items at lines starting with this prefix (e.g. `### `).
    pub split: Option<String>,
    /// Keep only items whose field (a path) equals the value, e.g. `where = { type = "match" }`.
    #[serde(rename = "where")]
    pub filter: BTreeMap<String, String>,
    /// Hit fields from item paths: title, snippet, uri, score, kind, date. `a|b` = fallback, `re:<regex>` = extract.
    pub map: BTreeMap<String, String>,
    /// Path to the text answer (read, brief, llm, store receipts).
    pub text: Option<String>,
    /// URI prefixes this adapter can `read`, e.g. `viking://`.
    pub owns: Vec<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct OnCfg {
    pub event: String,
    pub op: String,
    pub visibility: Option<String>,
    pub kinds: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ToolCfg {
    pub name: String,
    pub description: String,
    /// Simplified schema: `{ query = "string!", limit = "integer" }` (`!` = required), or a full JSON schema.
    pub params: Option<Value>,
    /// Name of an op of this adapter, or an inline op.
    pub op: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub cfg: Config,
    pub warnings: Vec<String>,
    pub shared_path: Option<PathBuf>,
    /// Trust hash (`sha256:<hex>`) of the shared layer when it declares adapters that are not trusted.
    pub untrusted_adapters: Option<String>,
    pub layers: Vec<PathBuf>,
}

pub fn config_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("KONTEXT_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("kontext");
    }
    util::home_dir().join(".config").join("kontext")
}

pub fn user_config_path() -> PathBuf {
    config_home().join("config.toml")
}

pub fn user_repo_config_path(repo: &Repo) -> PathBuf {
    config_home().join("repos").join(format!("{}.toml", repo.slug))
}

pub fn local_config_path(repo: &Repo) -> PathBuf {
    repo.state_dir().join("config.toml")
}

/// Where a repository's shared config may live: `.kontext.toml` at the root, or `kontext.toml`
/// inside the store directory (`.ai/` unless `store.dir` says otherwise in a user or clone-local layer).
pub fn shared_config_candidates(repo: &Repo) -> Vec<PathBuf> {
    let store_dir = |t: Option<toml::Table>| t.and_then(|t| t.get("store")?.get("dir")?.as_str().map(str::to_string));
    let dir = store_dir(read_layer(&local_config_path(repo)).ok().flatten())
        .or_else(|| store_dir(read_layer(&user_repo_config_path(repo)).ok().flatten()))
        .or_else(|| store_dir(read_layer(&user_config_path()).ok().flatten()))
        .unwrap_or_else(|| ".ai".to_string());
    let mut v = vec![repo.root.join(".kontext.toml"), repo.root.join(dir.trim_end_matches('/')).join("kontext.toml")];
    let default = repo.root.join(".ai").join("kontext.toml");
    if !v.contains(&default) {
        v.push(default);
    }
    v
}

// ---------------------------------------------------------------------------------------------
// Trust pins. `trust.toml` lives in the user's config directory, never in a repository:
//
//   pin_version = 2
//   [repos.<slug>]
//   adapters = "sha256:<hex>"   # SHA-256 over the canonical TOML of the shared layer's
//   trusted_at = "<iso>"        # `adapters`, `vars` and `project.name` together
//   [repos.<slug>.verify.<sha256(entry rel path, NUL, command)>]
//   id = "<entry id>"  path = "<entry rel path>"  command = "<command>"  pinned_at = "<iso>"
//
// Only a person at a terminal pins (`kontext trust`, `kontext trust --verify`, `kontext verify`
// on a TTY); no MCP tool and no API operation writes this file. Against a process that can edit
// it the gate is honour-based. A pin covers the text it hashes, not the scripts or the code that
// text runs from the working tree.
// ---------------------------------------------------------------------------------------------

/// Format of the pins `trust.toml` holds; older files carry 12-hex FNV adapter pins, which are refused.
pub const PIN_VERSION: i64 = 2;

#[cfg(test)]
thread_local! {
    static TEST_TRUST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Point this test thread's trust pins at `dir` instead of the user's config directory.
#[cfg(test)]
pub fn set_test_trust_dir(dir: &Path) {
    TEST_TRUST_DIR.with(|d| *d.borrow_mut() = Some(dir.to_path_buf()));
}

pub fn trust_path() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_TRUST_DIR.with(|d| d.borrow().clone()) {
        return dir.join("trust.toml");
    }
    config_home().join("trust.toml")
}

fn read_trust() -> toml::Table {
    std::fs::read_to_string(trust_path()).ok().and_then(|t| t.parse().ok()).unwrap_or_default()
}

fn repo_pins<'t>(root: &'t toml::Table, repo: &Repo) -> Option<&'t toml::Table> {
    root.get("repos")?.get(&repo.slug)?.as_table()
}

/// Read-modify-write this repository's table in `trust.toml`, keeping every other key.
fn update_repo_pins(repo: &Repo, f: impl FnOnce(&mut toml::Table)) -> Result<PathBuf> {
    let path = trust_path();
    let mut root = read_trust();
    root.insert("pin_version".into(), toml::Value::Integer(PIN_VERSION));
    let repos = root.entry("repos").or_insert_with(|| toml::Value::Table(Default::default()));
    let repos = repos.as_table_mut().context("trust.toml: `repos` is not a table")?;
    let entry = repos.entry(repo.slug.clone()).or_insert_with(|| toml::Value::Table(Default::default()));
    let entry = entry.as_table_mut().context("trust.toml: the repository's pins are not a table")?;
    f(entry);
    util::write_atomic(&path, &toml::to_string_pretty(&root)?)?;
    Ok(path)
}

/// The value with every table's keys sorted, so its TOML text does not depend on file order.
fn canonical(v: &toml::Value) -> toml::Value {
    match v {
        toml::Value::Table(t) => {
            let mut keys: Vec<&String> = t.keys().collect();
            keys.sort();
            toml::Value::Table(keys.into_iter().map(|k| (k.clone(), canonical(&t[k]))).collect())
        }
        toml::Value::Array(a) => toml::Value::Array(a.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// The trust pin of a shared config layer: `sha256:<hex>` over the canonical TOML of its
/// `adapters`, `vars` and `project.name` together — everything shared that reaches an adapter.
pub fn adapters_hash(layer: &toml::Table) -> String {
    let mut t = toml::Table::new();
    for key in ["adapters", "vars"] {
        if let Some(v) = layer.get(key) {
            t.insert(key.into(), v.clone());
        }
    }
    if let Some(name) = layer.get("project").and_then(|p| p.get("name")) {
        let mut p = toml::Table::new();
        p.insert("name".into(), name.clone());
        t.insert("project".into(), toml::Value::Table(p));
    }
    let text = toml::to_string(&canonical(&toml::Value::Table(t))).unwrap_or_default();
    format!("sha256:{}", util::sha256_hex(text.as_bytes()))
}

/// What `trust.toml` holds for a repository's shared adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    /// Pinned to this exact hash.
    Trusted,
    /// A 12-hex FNV pin from before pin_version 2: refused, never upgraded silently.
    Legacy,
    /// No pin, or a pin of another table.
    Untrusted,
}

pub fn adapters_pin(repo: &Repo, hash: &str) -> Pin {
    let root = read_trust();
    match repo_pins(&root, repo).and_then(|r| r.get("adapters")).and_then(|h| h.as_str()) {
        Some(h) if h == hash && h.starts_with("sha256:") => Pin::Trusted,
        Some(h) if h.len() == 12 && h.bytes().all(|b| b.is_ascii_hexdigit()) => Pin::Legacy,
        _ => Pin::Untrusted,
    }
}

pub fn is_trusted(repo: &Repo, hash: &str) -> bool {
    if std::env::var("KONTEXT_TRUST_REPO_ADAPTERS").is_ok_and(|v| v == "1") {
        return true;
    }
    adapters_pin(repo, hash) == Pin::Trusted
}

/// Pin the shared adapters (with the vars and project name the hash covers). The caller has
/// shown the person what it pins.
pub fn trust(repo: &Repo, hash: &str) -> Result<PathBuf> {
    update_repo_pins(repo, |entry| {
        entry.insert("adapters".into(), toml::Value::String(hash.to_string()));
        entry.insert("trusted_at".into(), toml::Value::String(util::now_iso()));
    })
}

/// The key of a verify pin: SHA-256 over the entry's repository-relative path, NUL, the command.
/// Editing the command, or moving the entry, revokes its pin.
pub fn verify_pin_hash(rel_path: &str, command: &str) -> String {
    util::sha256_hex(format!("{rel_path}\0{command}").as_bytes())
}

pub fn is_verify_pinned(repo: &Repo, rel_path: &str, command: &str) -> bool {
    let root = read_trust();
    let key = verify_pin_hash(rel_path, command);
    repo_pins(&root, repo).and_then(|r| r.get("verify")).and_then(|v| v.get(&key)).is_some()
}

/// An entry's verify command a person approved.
#[derive(Debug, Clone)]
pub struct VerifyPin {
    pub id: String,
    pub rel_path: String,
    pub command: String,
}

/// Record verify pins. Only terminal commands call this, after showing each command.
pub fn pin_verify(repo: &Repo, pins: &[VerifyPin]) -> Result<PathBuf> {
    update_repo_pins(repo, |entry| {
        let verify = entry.entry("verify").or_insert_with(|| toml::Value::Table(Default::default()));
        if !verify.is_table() {
            *verify = toml::Value::Table(Default::default());
        }
        let verify = verify.as_table_mut().expect("a table");
        for p in pins {
            let mut t = toml::Table::new();
            t.insert("id".into(), toml::Value::String(p.id.clone()));
            t.insert("path".into(), toml::Value::String(p.rel_path.clone()));
            t.insert("command".into(), toml::Value::String(p.command.clone()));
            t.insert("pinned_at".into(), toml::Value::String(util::now_iso()));
            verify.insert(verify_pin_hash(&p.rel_path, &p.command), toml::Value::Table(t));
        }
    })
}

fn read_layer(path: &Path) -> Result<Option<toml::Table>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let t: toml::Table = text.parse().with_context(|| format!("invalid TOML in {}", path.display()))?;
            Ok(Some(t))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

pub fn deep_merge(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => deep_merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// What the private layers (user global, user per repo, clone local) declare that the shared
/// layer must not change: their `[vars]` and the names of their adapters.
#[derive(Default)]
struct Private {
    vars: BTreeMap<String, String>,
    adapters: std::collections::BTreeSet<String>,
    /// `[verify] pass_env` from the user layers only.
    pass_env: Option<Vec<String>>,
}

impl Private {
    fn note(&mut self, t: &toml::Table, user: bool) {
        if let Some(vars) = t.get("vars").and_then(|v| v.as_table()) {
            for (k, v) in vars {
                if let Some(s) = v.as_str() {
                    self.vars.insert(k.clone(), s.to_string());
                }
            }
        }
        if let Some(adapters) = t.get("adapters").and_then(|v| v.as_table()) {
            self.adapters.extend(adapters.keys().cloned());
        }
        if user && let Some(list) = t.get("verify").and_then(|v| v.get("pass_env")).and_then(|v| v.as_array()) {
            self.pass_env = Some(list.iter().filter_map(|v| v.as_str().map(String::from)).collect());
        }
    }
}

/// Check the shared layer before it is merged: drop untrusted adapters (with a warning naming
/// `kontext trust`) and replace a `project.name` that is not a slug. Returns the untrusted hash.
fn gate_shared(repo: &Repo, path: &Path, t: &mut toml::Table, warnings: &mut Vec<String>) -> Option<String> {
    let mut untrusted = None;
    if t.contains_key("adapters") {
        let h = adapters_hash(t);
        let pin = if is_trusted(repo, &h) { Pin::Trusted } else { adapters_pin(repo, &h) };
        if pin != Pin::Trusted {
            t.remove("adapters");
            warnings.push(match pin {
                Pin::Legacy => format!(
                    "adapters in {} were skipped: their trust pin is in the old format and no longer counts — review them, then run `kontext trust`",
                    path.display()
                ),
                _ => format!("adapters in {} are not trusted yet and were skipped — review them, then run `kontext trust`", path.display()),
            });
            untrusted = Some(h);
        }
    }
    let bad_name = t.get("project").and_then(|p| p.get("name")).and_then(|n| n.as_str()).filter(|n| !util::is_slug(n)).map(String::from);
    if let Some(name) = bad_name {
        let mut slug = util::slugify(&repo.name, 64);
        if slug.is_empty() {
            slug = "project".into();
        }
        warnings.push(format!("project.name {name:?} in {} is not a slug (lowercase a-z, 0-9, -): using {slug:?}", path.display()));
        if let Some(p) = t.get_mut("project").and_then(|p| p.as_table_mut()) {
            p.insert("name".into(), toml::Value::String(slug));
        }
    }
    untrusted
}

pub fn load(repo: &Repo) -> Result<LoadedConfig> {
    let mut merged: toml::Table = DEFAULTS.parse().expect("built-in defaults parse");
    let mut warnings = Vec::new();
    let mut layers = Vec::new();
    let mut untrusted = None;
    let mut private = Private::default();

    let mut apply = |path: PathBuf, merged: &mut toml::Table, shared: bool, user: bool, warnings: &mut Vec<String>| -> Result<bool> {
        let Some(mut t) = read_layer(&path)? else { return Ok(false) };
        if shared {
            if let Some(h) = gate_shared(repo, &path, &mut t, warnings) {
                untrusted = Some(h);
            }
        } else {
            private.note(&t, user);
        }
        deep_merge(merged, t);
        layers.push(path);
        Ok(true)
    };

    apply(user_config_path(), &mut merged, false, true, &mut warnings)?;
    apply(user_repo_config_path(repo), &mut merged, false, true, &mut warnings)?;
    let mut shared_path = None;
    for cand in shared_config_candidates(repo) {
        if apply(cand.clone(), &mut merged, true, false, &mut warnings)? {
            shared_path = Some(cand);
            break;
        }
    }
    apply(local_config_path(repo), &mut merged, false, false, &mut warnings)?;

    let mut cfg: Config = toml::Value::Table(merged).try_into().context("configuration does not match the expected schema")?;
    // shared [vars] never override what a private layer set, for the adapters a private layer declares
    for name in &private.adapters {
        if let Some(a) = cfg.adapters.get_mut(name) {
            for (k, v) in &private.vars {
                a.vars.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }
    // the environment a verify command sees is the user's choice, never the repository's
    cfg.verify.pass_env = private.pass_env.unwrap_or_default();
    Ok(LoadedConfig { cfg, warnings, shared_path, untrusted_adapters: untrusted, layers })
}

impl Config {
    pub fn kind_names(&self) -> Vec<String> {
        self.store.kinds.keys().cloned().collect()
    }

    /// Repo-relative directory holding entries of `kind`.
    pub fn kind_path(&self, kind: &str) -> String {
        let store = self.store.dir.trim_end_matches('/');
        match self.store.kinds.get(kind) {
            Some(k) => {
                if let Some(p) = &k.path {
                    return p.trim_end_matches('/').to_string();
                }
                let dir = k.dir.clone().unwrap_or_else(|| format!("{kind}s"));
                format!("{store}/{}", dir.trim_end_matches('/'))
            }
            None => format!("{store}/{kind}s"),
        }
    }

    pub fn kind_style(&self, kind: &str) -> Style {
        self.store.kinds.get(kind).and_then(|k| k.style).unwrap_or(self.store.style)
    }

    pub fn kind_numbering(&self, kind: &str) -> Numbering {
        self.store.kinds.get(kind).and_then(|k| k.numbering).unwrap_or(Numbering::None)
    }

    pub fn trailer_for(&self, kind: &str) -> Option<String> {
        self.store.kinds.get(kind).and_then(|k| k.trailer.clone())
    }

    pub fn project_name(&self, repo: &Repo) -> String {
        self.project.name.clone().unwrap_or_else(|| repo.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_merge() {
        let mut base: toml::Table = DEFAULTS.parse().unwrap();
        let over: toml::Table = r#"
            [store.kinds.decision]
            path = "docs/adr"
            style = "fields"
            numbering = "sequential"
        "#
        .parse()
        .unwrap();
        deep_merge(&mut base, over);
        let cfg: Config = toml::Value::Table(base).try_into().unwrap();
        assert_eq!(cfg.kind_path("decision"), "docs/adr");
        assert_eq!(cfg.kind_path("learning"), ".ai/learnings");
        assert_eq!(cfg.kind_style("decision"), Style::Fields);
        assert_eq!(cfg.kind_numbering("decision"), Numbering::Sequential);
        assert_eq!(cfg.trailer_for("decision").as_deref(), Some("Decision"));
        assert_eq!(cfg.brief.budget_tokens, 1400);
        assert_eq!(cfg.freshness.threshold_commits, 20);
    }

    #[test]
    fn the_trust_hash_covers_adapters_vars_and_project_name_in_any_order() {
        let layer = |text: &str| -> toml::Table { text.parse().unwrap() };
        let a = layer("[project]\nname = \"x\"\n[vars]\nu = \"1\"\nv = \"2\"\n[adapters.m]\ndriver = \"command\"\ncommand = [\"rg\"]\n");
        let reordered =
            layer("[adapters.m]\ncommand = [\"rg\"]\ndriver = \"command\"\n[vars]\nv = \"2\"\nu = \"1\"\n[project]\nname = \"x\"\n");
        let h = adapters_hash(&a);
        assert!(h.starts_with("sha256:") && h.len() == 7 + 64, "{h}");
        assert_eq!(h, adapters_hash(&reordered), "key order does not change the pin");
        let other_var =
            layer("[project]\nname = \"x\"\n[vars]\nu = \"evil\"\nv = \"2\"\n[adapters.m]\ndriver = \"command\"\ncommand = [\"rg\"]\n");
        let other_name =
            layer("[project]\nname = \"y\"\n[vars]\nu = \"1\"\nv = \"2\"\n[adapters.m]\ndriver = \"command\"\ncommand = [\"rg\"]\n");
        assert_ne!(h, adapters_hash(&other_var));
        assert_ne!(h, adapters_hash(&other_name));
        // what does not reach an adapter does not move the pin
        let mut with_brief = a.clone();
        with_brief.insert("brief".into(), toml::Value::Table(layer("budget_tokens = 9")));
        assert_eq!(h, adapters_hash(&with_brief));
        assert_ne!(verify_pin_hash(".ai/a.md", "true"), verify_pin_hash(".ai/a.md", "true "));
        assert_ne!(verify_pin_hash(".ai/a.md", "true"), verify_pin_hash(".ai/b.md", "true"));
    }

    #[test]
    fn adapter_tables_deserialize() {
        let t: toml::Table = r#"
            [adapters.viking]
            driver = "http"
            base_url = "http://127.0.0.1:1933"
            headers = { "X-User" = "{{vars.user}}" }
            vars = { user = "alice" }
            [adapters.viking.ops.search]
            method = "POST"
            path = "/api/v1/search/find"
            body = { query = "{{query}}", limit = "{{limit}}" }
            items = "result.*[*]"
            map = { snippet = "abstract" }
            [[adapters.viking.on]]
            event = "sync"
            op = "store"
        "#
        .parse()
        .unwrap();
        let mut base: toml::Table = DEFAULTS.parse().unwrap();
        deep_merge(&mut base, t);
        let cfg: Config = toml::Value::Table(base).try_into().unwrap();
        let a = &cfg.adapters["viking"];
        assert_eq!(a.driver, "http");
        assert_eq!(a.ops["search"].body.as_ref().unwrap()["query"], "{{query}}");
        assert_eq!(a.on[0].event, "sync");
    }
}
