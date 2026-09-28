//! Wire agent clients to the kontext MCP server (and, optionally, auto-inject the brief).
use crate::app::App;
use crate::util;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;

pub const TARGETS: &[&str] = &["claude", "claude-hooks", "codex", "cursor", "opencode", "agents-md"];

const AGENTS_START: &str = "<!-- kontext:start -->";
const AGENTS_END: &str = "<!-- kontext:end -->";

pub fn agents_block() -> String {
    format!(
        "{AGENTS_START}\n## Team context (kontext)\n\n\
- Start a task with `ctx_brief` (MCP server `kontext`): decisions, conventions, pitfalls and the module map, within a small token budget. Pass `focus` with the paths you will touch.\n\
- Need more? `ctx_search` (decisions, docs, commit history, connected memory systems) and `ctx_read <uri>` for detail. `ctx_why <path[:line]>` explains why code is the way it is.\n\
- Decided something durable, or hit a non-obvious gotcha? `ctx_capture` it (kind: decision | convention | learning | incident; paths you touched). Personal notes: `visibility: private`.\n\
- Before committing: `ctx_prepare_commit` — it lists relevant inbox candidates, validates knowledge files and promotes the ones that belong to the change so they ship in the same commit and get reviewed with it.\n\
- Shared knowledge lives in `.ai/` and changes only through commits and review — never edit it to win an argument; propose a superseding decision instead.\n{AGENTS_END}\n"
    )
}

fn merge_json_file(path: &Path, patch: impl FnOnce(&mut Value)) -> Result<bool> {
    let mut v: Value = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => {
            serde_json::from_str(&crate::init::scan::strip_jsonc(&t)).with_context(|| format!("{} is not valid JSON", path.display()))?
        }
        _ => json!({}),
    };
    let before = v.clone();
    patch(&mut v);
    if v == before {
        return Ok(false);
    }
    util::write_atomic(path, &(serde_json::to_string_pretty(&v)? + "\n"))?;
    Ok(true)
}

fn obj<'a>(v: &'a mut Value, key: &str) -> &'a mut serde_json::Map<String, Value> {
    if !v.get(key).is_some_and(|x| x.is_object()) {
        v[key] = json!({});
    }
    v[key].as_object_mut().unwrap()
}

pub fn connect(app: &App, target: &str, write: bool) -> Result<String> {
    let root = &app.repo.root;
    match target {
        "claude" => {
            let path = root.join(".mcp.json");
            if !write {
                return Ok("Add to .mcp.json (project scope, shared with the team):\n{\n  \"mcpServers\": { \"kontext\": { \"command\": \"kontext\", \"args\": [\"mcp\"] } }\n}\nor for yourself only: claude mcp add --scope user kontext -- kontext mcp".into());
            }
            let changed = merge_json_file(&path, |v| {
                obj(v, "mcpServers").insert("kontext".into(), json!({"command": "kontext", "args": ["mcp"]}));
            })?;
            Ok(format!(
                "claude: {} .mcp.json (server `kontext`; Claude Code asks each teammate to approve it once)",
                if changed { "wrote" } else { "already configured in" }
            ))
        }
        "claude-hooks" => {
            let path = root.join(".claude").join("settings.json");
            let cmd = "command -v kontext >/dev/null 2>&1 && kontext brief --format claude-hook 2>/dev/null || true";
            if !write {
                return Ok(format!("Add a SessionStart hook to .claude/settings.json running:\n  {cmd}"));
            }
            let changed = merge_json_file(&path, |v| {
                let hooks = obj(v, "hooks");
                let list = hooks.entry("SessionStart").or_insert_with(|| json!([]));
                let exists = list.as_array().is_some_and(|a| a.iter().any(|h| h.to_string().contains("kontext brief")));
                if !exists && let Some(a) = list.as_array_mut() {
                    a.push(json!({"hooks": [{"type": "command", "command": cmd, "timeout": 15}]}));
                }
            })?;
            Ok(format!(
                "claude-hooks: {} .claude/settings.json (SessionStart injects `kontext brief`)",
                if changed { "updated" } else { "already configured in" }
            ))
        }
        "cursor" => {
            let path = root.join(".cursor").join("mcp.json");
            if !write {
                return Ok(
                    "Add to .cursor/mcp.json: { \"mcpServers\": { \"kontext\": { \"command\": \"kontext\", \"args\": [\"mcp\"] } } }"
                        .into(),
                );
            }
            let changed = merge_json_file(&path, |v| {
                obj(v, "mcpServers").insert("kontext".into(), json!({"command": "kontext", "args": ["mcp"]}));
            })?;
            Ok(format!("cursor: {} .cursor/mcp.json", if changed { "wrote" } else { "already configured in" }))
        }
        "opencode" => {
            let path = root.join("opencode.json");
            if !write {
                return Ok("Add to opencode.json: { \"mcp\": { \"kontext\": { \"type\": \"local\", \"command\": [\"kontext\", \"mcp\"], \"enabled\": true } } }".into());
            }
            let changed = merge_json_file(&path, |v| {
                if v.get("$schema").is_none() {
                    v["$schema"] = json!("https://opencode.ai/config.json");
                }
                obj(v, "mcp").insert("kontext".into(), json!({"type": "local", "command": ["kontext", "mcp"], "enabled": true}));
            })?;
            Ok(format!("opencode: {} opencode.json", if changed { "wrote" } else { "already configured in" }))
        }
        "codex" => {
            let snippet = "[mcp_servers.kontext]\ncommand = \"kontext\"\nargs = [\"mcp\"]\n";
            let path = util::home_dir().join(".codex").join("config.toml");
            if !write {
                return Ok(format!(
                    "Add to ~/.codex/config.toml (user level; run `kontext connect codex --write` to append it):\n{snippet}"
                ));
            }
            let current = std::fs::read_to_string(&path).unwrap_or_default();
            if current.contains("[mcp_servers.kontext]") {
                return Ok("codex: ~/.codex/config.toml already has [mcp_servers.kontext]".into());
            }
            if path.exists() {
                std::fs::copy(&path, path.with_extension(format!("toml.bak-kontext-{}", chrono::Local::now().format("%Y%m%d%H%M%S"))))?;
            }
            let sep = if current.is_empty() || current.ends_with("\n\n") {
                ""
            } else if current.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            util::write_atomic(&path, &format!("{current}{sep}{snippet}"))?;
            Ok("codex: appended [mcp_servers.kontext] to ~/.codex/config.toml (backup kept next to it)".into())
        }
        "agents-md" => {
            let candidates = ["AGENTS.md", "CLAUDE.md", "Claude.md"];
            let file = candidates.iter().find(|f| root.join(f).is_file()).copied().unwrap_or("AGENTS.md");
            let path = root.join(file);
            if !write {
                return Ok(format!("Append to {file}:\n\n{}", agents_block()));
            }
            let current = std::fs::read_to_string(&path).unwrap_or_default();
            let next = match (current.find(AGENTS_START), current.find(AGENTS_END)) {
                (Some(s), Some(e)) if e > s => {
                    format!("{}{}{}", &current[..s], agents_block(), current[e + AGENTS_END.len()..].trim_start_matches('\n'))
                }
                _ => {
                    let sep = if current.is_empty() || current.ends_with("\n\n") {
                        ""
                    } else if current.ends_with('\n') {
                        "\n"
                    } else {
                        "\n\n"
                    };
                    format!("{current}{sep}{}", agents_block())
                }
            };
            if next == current {
                return Ok(format!("agents-md: {file} already has the kontext block"));
            }
            util::write_atomic(&path, &next)?;
            Ok(format!("agents-md: wrote the kontext block into {file}"))
        }
        other => bail!("unknown target '{other}' (use one of: {})", TARGETS.join(", ")),
    }
}
