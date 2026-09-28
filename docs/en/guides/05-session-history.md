# Session history

Raw agent sessions are too long and too personal for git, but they answer "what happened when we changed this?". kontext reads them through adapters and never copies them into the repository.

| Op | Used for |
| --- | --- |
| `history` | `ctx_why <path|commit|topic>` — sessions that touched the target |
| `search` | `ctx_search` — full-text or semantic hits from transcripts |
| `brief` | a short section in `ctx_brief` (a primer of recent work) |

## sessions

[sessions](https://github.com/nicknisi/sessions) indexes transcripts of Claude Code, Codex, OpenCode and Pi, and answers `why_did_this_change` by correlating files, commits and time.

```sh
kontext adapters add sessions
kontext adapters test sessions
```

Mapped tools: `history` → `why_did_this_change`, `search` → `search_sessions`, `brief` → `get_context_primer`.

## Agent LCM

[Agent LCM](https://github.com/Team-Volt/agent-lcm) is a lossless, cross-harness session archive (Codex, Cursor, VS Code, GitHub Copilot, Kiro, Claude Code, OpenCode) exposed over MCP.

```sh
kontext adapters add agent-lcm
kontext adapters test agent-lcm
```

Mapped tools: `search` and `history` → `lcm_grep`, `brief` → `lcm_pack_context`.

Both presets bind arguments from the tools' schemas; if a tool changes, `kontext adapters inspect <name>` shows its current parameters.

## From history to knowledge

Session hits are evidence, not truth. When a session reveals a decision worth keeping, capture it — with the paths it governs — and promote it into the relevant commit. The `kontext-reflect` prompt does exactly that at the end of a session.
