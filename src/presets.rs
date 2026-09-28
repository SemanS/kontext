//! Adapter presets: plain TOML snippets (bundled ones plus `~/.config/kontext/presets/*.toml`).
//! Nothing in the core refers to them — a preset is just configuration someone wrote once.
use crate::app::App;
use crate::config;
use crate::util;
use anyhow::{Result, anyhow, bail};
use std::path::PathBuf;

pub const BUNDLED: &[(&str, &str)] = &[
    ("openviking", include_str!("../presets/openviking.toml")),
    ("codegraph", include_str!("../presets/codegraph.toml")),
    ("serena", include_str!("../presets/serena.toml")),
    ("agent-lcm", include_str!("../presets/agent-lcm.toml")),
    ("sessions", include_str!("../presets/sessions.toml")),
    ("llm-claude", include_str!("../presets/llm-claude.toml")),
    ("llm-codex", include_str!("../presets/llm-codex.toml")),
    ("llm-ollama", include_str!("../presets/llm-ollama.toml")),
    ("slack-webhook", include_str!("../presets/slack-webhook.toml")),
];

fn user_presets() -> Vec<(String, String)> {
    let dir = config::config_home().join("presets");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("toml")
                && let (Some(stem), Ok(text)) = (p.file_stem().map(|s| s.to_string_lossy().to_string()), std::fs::read_to_string(&p))
            {
                out.push((stem, text));
            }
        }
    }
    out
}

pub fn all() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = BUNDLED.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    for (n, t) in user_presets() {
        v.retain(|(x, _)| x != &n);
        v.push((n, t));
    }
    v
}

pub fn get(name: &str) -> Option<String> {
    all().into_iter().find(|(n, _)| n == name).map(|(_, t)| t)
}

pub fn describe(text: &str) -> String {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("description = ").map(|d| d.trim_matches('"').to_string()))
        .or_else(|| text.lines().next().map(|l| l.trim_start_matches('#').trim().to_string()))
        .unwrap_or_default()
}

fn adapter_names(text: &str) -> Vec<String> {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("adapters").and_then(|a| a.as_table()).map(|a| a.keys().cloned().collect()))
        .unwrap_or_default()
}

/// Remove every table of `adapters.<name>` (and the comments right above them) from TOML text,
/// leaving the rest of the file — including other presets' comments — untouched.
fn remove_adapter_tables(text: &str, name: &str) -> String {
    let own = |header: &str| {
        let path = header.trim().trim_start_matches('[').trim_end_matches(']').trim().replace('"', "");
        path == format!("adapters.{name}") || path.starts_with(&format!("adapters.{name}."))
    };
    let mut out: Vec<&str> = Vec::new();
    let mut trivia: Vec<&str> = Vec::new();
    let mut keep = true;
    for line in text.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            trivia.push(line);
            continue;
        }
        if t.starts_with('[') {
            keep = !own(t);
            if keep {
                out.append(&mut trivia);
            } else {
                trivia.clear();
            }
            if keep {
                out.push(line);
            }
            continue;
        }
        if keep {
            out.append(&mut trivia);
            out.push(line);
        } else {
            trivia.clear();
        }
    }
    if keep {
        out.append(&mut trivia);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

pub enum Scope {
    User,
    Repo,
    Local,
}

/// Set `vars` of every adapter in a preset (keeps the preset's leading comment block).
fn with_vars(text: &str, vars: &[(String, String)]) -> Result<String> {
    if vars.is_empty() {
        return Ok(text.to_string());
    }
    let mut t: toml::Table = text.parse()?;
    if let Some(adapters) = t.get_mut("adapters").and_then(|a| a.as_table_mut()) {
        for (_, a) in adapters.iter_mut() {
            if let Some(a) = a.as_table_mut() {
                let v = a.entry("vars").or_insert_with(|| toml::Value::Table(Default::default()));
                if let Some(v) = v.as_table_mut() {
                    for (k, val) in vars {
                        v.insert(k.clone(), toml::Value::String(val.clone()));
                    }
                }
            }
        }
    }
    let header: String = text.lines().take_while(|l| l.starts_with('#')).map(|l| format!("{l}\n")).collect();
    Ok(format!("{header}{}", toml::to_string_pretty(&t)?))
}

pub fn add(app: &App, name: &str, scope: Scope, force: bool, vars: &[(String, String)]) -> Result<(PathBuf, Vec<String>)> {
    let text = get(name).ok_or_else(|| anyhow!("no preset '{name}' (see `kontext presets`)"))?;
    let text = with_vars(&text, vars)?;
    let path = match scope {
        Scope::User => config::user_config_path(),
        Scope::Local => config::local_config_path(&app.repo),
        Scope::Repo => app.loaded.shared_path.clone().unwrap_or_else(|| app.repo.root.join(&app.cfg().store.dir).join("kontext.toml")),
    };
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let names = adapter_names(&text);
    let existing = adapter_names(&current);
    let clash: Vec<&String> = names.iter().filter(|n| existing.contains(n)).collect();
    let mut base = current.clone();
    if !clash.is_empty() {
        if !force {
            bail!(
                "{} already defines adapter(s) {} — edit it there, or pass --force to replace",
                path.display(),
                clash.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
        for n in &clash {
            base = remove_adapter_tables(&base, n);
        }
    }
    let sep = if base.is_empty() || base.ends_with("\n\n") {
        ""
    } else if base.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let next = format!("{base}{sep}# preset `{name}` (added by kontext {})\n{}", util::today(), text.trim_start());
    next.parse::<toml::Table>().map_err(|e| anyhow!("the result would not be valid TOML: {e}"))?;
    util::write_atomic(&path, &next)?;
    Ok((path, names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_one_adapter() {
        let text = "# mine\n[adapters.a]\ndriver = \"http\"\n\n# preset b\n[adapters.b]\ndriver = \"mcp\"\n[adapters.b.ops.code]\ntool = \"x\"\n\n[[adapters.b.on]]\nevent = \"sync\"\n\n# keep me\n[adapters.c]\ndriver = \"command\"\n";
        let out = remove_adapter_tables(text, "b");
        assert!(out.contains("# mine\n[adapters.a]") && out.contains("# keep me\n[adapters.c]"), "{out}");
        assert!(!out.contains("adapters.b") && !out.contains("preset b"), "{out}");
        assert!(out.parse::<toml::Table>().is_ok());
    }
}
