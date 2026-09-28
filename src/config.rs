//! Layered configuration.
//!
//! Layers, later wins (tables are deep-merged, arrays replaced):
//!   1. built-in defaults (`DEFAULTS` below)
//!   2. user global      `~/.config/kontext/config.toml`          (personal adapters: trusted)
//!   3. user per repo    `~/.config/kontext/repos/<slug>.toml`    (trusted)
//!   4. repo shared      `<root>/.kontext.toml` or `<root>/.ai/kontext.toml` (committed; adapters need `kontext trust`)
//!   5. clone local      `<git-common-dir>/kontext/config.toml`  (never committed: trusted)
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

[init]
max_module_docs = 40
min_module_files = 3
jobs = 2
task_max_files = 12
task_inline_chars = 24000
max_decision_tasks = 10
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
    pub capture: CaptureCfg,
    pub hooks: HooksCfg,
    pub secrets: SecretsCfg,
    pub init: InitCfg,
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
    /// Hash of the repo-shared adapters table when it is present but not trusted.
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

fn trust_path() -> PathBuf {
    config_home().join("trust.toml")
}

pub fn adapters_hash(table: &toml::Value) -> String {
    util::short_hash(&toml::to_string(table).unwrap_or_default())
}

pub fn is_trusted(repo: &Repo, hash: &str) -> bool {
    if std::env::var("KONTEXT_TRUST_REPO_ADAPTERS").is_ok_and(|v| v == "1") {
        return true;
    }
    let Ok(text) = std::fs::read_to_string(trust_path()) else { return false };
    let Ok(v) = text.parse::<toml::Table>() else { return false };
    v.get("repos").and_then(|r| r.get(&repo.slug)).and_then(|r| r.get("adapters")).and_then(|h| h.as_str()).is_some_and(|h| h == hash)
}

pub fn trust(repo: &Repo, hash: &str) -> Result<PathBuf> {
    let path = trust_path();
    let mut root: toml::Table = std::fs::read_to_string(&path).ok().and_then(|t| t.parse().ok()).unwrap_or_default();
    let repos = root.entry("repos").or_insert_with(|| toml::Value::Table(Default::default()));
    if let Some(t) = repos.as_table_mut() {
        let mut entry = toml::Table::new();
        entry.insert("adapters".into(), toml::Value::String(hash.to_string()));
        entry.insert("trusted_at".into(), toml::Value::String(util::now_iso()));
        t.insert(repo.slug.clone(), toml::Value::Table(entry));
    }
    util::write_atomic(&path, &toml::to_string_pretty(&root)?)?;
    Ok(path)
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

pub fn load(repo: &Repo) -> Result<LoadedConfig> {
    let mut merged: toml::Table = DEFAULTS.parse().expect("built-in defaults parse");
    let mut warnings = Vec::new();
    let mut layers = Vec::new();
    let mut untrusted = None;

    let mut apply = |path: PathBuf, merged: &mut toml::Table, shared: bool, warnings: &mut Vec<String>| -> Result<bool> {
        let Some(mut t) = read_layer(&path)? else { return Ok(false) };
        if shared && let Some(adapters) = t.get("adapters") {
            let h = adapters_hash(adapters);
            if !is_trusted(repo, &h) {
                t.remove("adapters");
                warnings.push(format!(
                    "adapters in {} are not trusted yet and were skipped — review them, then run `kontext trust`",
                    path.display()
                ));
                untrusted = Some(h);
            }
        }
        deep_merge(merged, t);
        layers.push(path);
        Ok(true)
    };

    apply(user_config_path(), &mut merged, false, &mut warnings)?;
    apply(user_repo_config_path(repo), &mut merged, false, &mut warnings)?;
    let mut shared_path = None;
    for cand in shared_config_candidates(repo) {
        if apply(cand.clone(), &mut merged, true, &mut warnings)? {
            shared_path = Some(cand);
            break;
        }
    }
    apply(local_config_path(repo), &mut merged, false, &mut warnings)?;

    let cfg: Config = toml::Value::Table(merged).try_into().context("configuration does not match the expected schema")?;
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
