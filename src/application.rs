//! Stable embedding facade. Inspection parses trusted committed bytes without executing adapters.
use crate::config::{self, Config};
use crate::store::{Entry, FmValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// A canonical entry accepted for import from an independently verified review revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedEntry {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub summary: Option<String>,
    pub status: Option<String>,
    /// The last day someone confirmed the entry still holds (`YYYY-MM-DD`), as written; it is
    /// validated by `kontext check`, not here.
    #[serde(default)]
    pub checked: Option<String>,
    pub paths: Vec<String>,
    pub tags: Vec<String>,
    pub file: String,
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Knowledge format policy from defaults and reviewed repository configuration.
/// This object performs no filesystem, network, model or adapter operations.
pub struct KnowledgePolicy {
    pub(crate) cfg: Config,
}

fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('-')
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && Path::new(path).components().all(|part| matches!(part, Component::Normal(_)))
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

impl KnowledgePolicy {
    /// Configuration locations in the same precedence order as the committed export service.
    pub fn config_candidates(default_store_dir: &str) -> Result<Vec<String>, String> {
        if !relative(default_store_dir) {
            return Err("invalid repository-relative knowledge store".into());
        }
        let mut paths = Vec::new();
        for path in [".kontext.toml".into(), format!("{default_store_dir}/kontext.toml"), ".ai/kontext.toml".into()] {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    /// The caller must read `committed_config` from the independently verified revision.
    pub fn from_config(default_store_dir: &str, committed_config: Option<&str>) -> Result<Self, String> {
        Self::config_candidates(default_store_dir)?;
        let mut table: toml::Table = config::DEFAULTS.parse().map_err(|err: toml::de::Error| err.to_string())?;
        table
            .get_mut("store")
            .and_then(toml::Value::as_table_mut)
            .expect("built-in store config")
            .insert("dir".into(), toml::Value::String(default_store_dir.into()));
        if let Some(text) = committed_config {
            let layer: toml::Table = text.parse().map_err(|err: toml::de::Error| err.to_string())?;
            config::deep_merge(&mut table, layer);
        }
        let cfg: Config = toml::Value::Table(table).try_into().map_err(|err: toml::de::Error| err.to_string())?;
        if !relative(&cfg.store.dir) || cfg.kind_names().iter().any(|kind| !relative(&cfg.kind_path(kind))) {
            return Err("configured knowledge paths must remain inside the repository".into());
        }
        Ok(Self { cfg })
    }

    /// Resolve a Markdown entry's configured kind, including custom kinds and ADR directories.
    pub fn kind_for_path(&self, path: &str) -> Option<String> {
        if !relative(path) || !path.ends_with(".md") || crate::store::is_non_entry_name(path.rsplit('/').next()?) {
            return None;
        }
        let mut kinds = self.cfg.kind_names();
        kinds.sort_by_key(|kind| std::cmp::Reverse(self.cfg.kind_path(kind).len()));
        kinds.into_iter().find(|kind| path.starts_with(&format!("{}/", self.cfg.kind_path(kind))))
    }

    /// Inspect committed bytes. The caller separately proves ref, revision, blob and regular-file mode.
    /// An accepted metadata field alone is never publication evidence.
    pub fn inspect(&self, path: &str, markdown: &str) -> Result<CommittedEntry, String> {
        let kind = self.kind_for_path(path).ok_or_else(|| "file is not in a configured knowledge kind directory".to_string())?;
        if markdown.len() > 1024 * 1024 {
            return Err("knowledge blob exceeds 1 MiB".into());
        }
        let entry = Entry::parse(path, markdown, &kind);
        if !self.cfg.store.kinds.contains_key(&entry.kind) {
            return Err("unknown configured knowledge kind".into());
        }
        if entry.id.is_empty() || entry.title.trim().is_empty() || entry.body.trim().is_empty() {
            return Err("knowledge needs an ID, title and body".into());
        }
        if !reviewable(&entry) {
            return Err("entry is not active accepted team knowledge".into());
        }
        let extra = entry
            .extra
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        FmValue::Str(text) => serde_json::json!(text),
                        FmValue::List(items) => serde_json::json!(items),
                    },
                )
            })
            .collect();
        Ok(CommittedEntry {
            id: entry.id,
            kind: entry.kind,
            title: entry.title,
            body: entry.body,
            summary: entry.summary,
            status: entry.status,
            checked: entry.checked,
            paths: entry.paths,
            tags: entry.tags,
            file: path.into(),
            extra,
        })
    }
}

pub(crate) fn reviewable(entry: &Entry) -> bool {
    (entry.kind != "decision" || entry.status.as_deref() == Some("accepted"))
        && matches!(entry.status.as_deref(), None | Some("accepted"))
        && entry.visibility.as_deref().is_none_or(|value| !value.eq_ignore_ascii_case("private"))
}

/// Invoke the same versioned application service used by CLI and MCP in a repository/worktree.
pub fn request(dir: &Path, operation: &str, input: &serde_json::Value) -> serde_json::Value {
    match crate::app::App::open(dir) {
        Ok(app) => crate::api::execute(&app, operation, input),
        Err(err) => crate::api::failure(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_policy_supports_custom_adr_and_keeps_review_boundary() {
        let config = "[store]\ndir='knowledge'\n[store.kinds.decision]\npath='docs/adr'\nstyle='fields'\n[store.kinds.policy]\npath='knowledge/policies'\n";
        let policy = KnowledgePolicy::from_config(".ai", Some(config)).unwrap();
        let text = "# 0001 — Reviewed policy\n\n**Status:** accepted\n\n## Decision\nKeep this reviewed rule.\n";
        let entry = policy.inspect("docs/adr/0001-reviewed.md", text).unwrap();
        assert_eq!(entry.kind, "decision");
        assert_eq!(entry.id, "0001-reviewed");
        assert!(policy.inspect("docs/adr/0001-reviewed.md", &text.replace("accepted", "proposed")).is_err());
        assert!(policy.inspect("docs/adr/0001-reviewed.md", &text.replace("**Status:** accepted\n", "")).is_err());
        assert!(policy.inspect("outside/0001-reviewed.md", text).is_err());
        let custom = "---\nid: custom\nkind: policy\ntitle: Custom\n---\nA reviewed policy.\n";
        assert_eq!(policy.inspect("knowledge/policies/custom.md", custom).unwrap().kind, "policy");
        let confirmed =
            policy.inspect("docs/adr/0001-reviewed.md", &text.replace("accepted\n", "accepted\n**Checked:** 2026-10-20\n")).unwrap();
        assert_eq!(confirmed.checked.as_deref(), Some("2026-10-20"));
        assert!(!confirmed.extra.contains_key("checked"), "a field of its own, not extra");
    }
    #[test]
    fn malformed_configuration_and_private_entries_are_rejected() {
        assert!(KnowledgePolicy::from_config("../outside", None).is_err());
        assert!(KnowledgePolicy::from_config(".ai", Some("[store]\ndir='../outside'\n")).is_err());
        let policy = KnowledgePolicy::from_config(".ai", None).unwrap();
        let text = "---\nid: one\nkind: learning\ntitle: One\nvisibility: private\n---\nPrivate note.\n";
        assert!(policy.inspect(".ai/learnings/one.md", text).is_err());
        assert!(policy.inspect(".ai/learnings/../one.md", text).is_err());
    }
}
