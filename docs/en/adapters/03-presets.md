# Presets

A preset is a TOML snippet that defines one adapter. kontext ships a few and you can add your own; none of them is special to the code.

```sh
kontext presets                         # list bundled and user presets
kontext presets openviking              # print one
kontext adapters add openviking --var user=alice          # append it to ~/.config/kontext/config.toml
kontext adapters add codegraph --scope local              # this clone only
kontext adapters add openviking --var user=alice --force  # replace an existing definition
kontext adapters test                   # health + a sample call of each op
```

`--scope user` (default) writes to your personal config, `repo` to the shared `.ai/kontext.toml` (teammates must `kontext trust` it), `local` to `.git/kontext/config.toml`.

Your own presets go into `~/.config/kontext/presets/<name>.toml` and appear in `kontext presets`.

## Bundled presets

| Preset | Driver | Ops | Events | Notes |
| --- | --- | --- | --- | --- |
| `openviking` | http | search, read, store, remember, health | `sync` → store (team knowledge into the peer's `resources/`), private `capture` → remember (`memories/`) | [guide](../guides/03-openviking.md) |
| `codegraph` | mcp | code | – | active where `.codegraph/` exists; [guide](../guides/04-code-intelligence.md) |
| `serena` | mcp | code | – | LSP-backed symbol search; [guide](../guides/04-code-intelligence.md) |
| `agent-lcm` | mcp | search, history, brief | – | cross-harness session archive; [guide](../guides/05-session-history.md) |
| `sessions` | mcp | history, search, brief | – | `why_did_this_change` for `ctx_why`; [guide](../guides/05-session-history.md) |
| `llm-claude` | command | llm | – | `claude -p` for autopilot deepening |
| `llm-codex` | command | llm | – | `codex exec` (read-only sandbox, ephemeral) |
| `llm-ollama` | http | llm | – | a local model via `/api/generate` |
| `slack-webhook` | http | store | `sync` (decisions) | announces merged decisions in a channel |

Presets whose external program is missing are inactive (`when.command`), so adding one on a machine without the tool is harmless.

The OpenViking and CodeGraph presets were verified against their current versions; Serena, Agent LCM and sessions follow their documented MCP tools and rely on schema-based argument binding. If a tool changes, `kontext adapters inspect <name>` shows what to adjust.

## Variables

Presets expose knobs through `vars`:

| Preset | Variable | Default |
| --- | --- | --- |
| `openviking` | `user`, `account`, `peer` | `$OPENVIKING_USER` or `default`, `default`, the project name |
| `llm-claude` | `model` | `sonnet` |
| `llm-ollama` | `model` | `qwen2.5-coder:14b` |

Set them when adding (`--var model=haiku`) or override them per repository in `~/.config/kontext/repos/<slug>.toml`:

```toml
[adapters.openviking.vars]
peer = "billing-service"
```

The slug is shown by `kontext status` (`github.com-acme-shop`).

## Contributing a preset

A good preset is small, uses `when` to stay inactive where its tool is missing, prefers schema-based binding over hard-coded `args`, and documents its variables in comments. Add it to `presets/`, register it in `src/presets.rs`, and describe it here.
