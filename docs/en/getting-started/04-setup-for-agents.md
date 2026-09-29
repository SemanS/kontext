# Set up your agents

kontext speaks the Model Context Protocol over stdio. Any client that can start a local MCP server with the command `kontext mcp` gets the full tool set. Start the server from inside a repository (clients usually do this for you: they launch MCP servers in the project directory).

## Wire a client

| Client | Command | What it writes |
| --- | --- | --- |
| Claude Code | `kontext connect claude --write` | `.mcp.json` in the repository (project scope; each teammate approves it once) |
| Claude Code, just for you | `claude mcp add --scope user kontext -- kontext mcp` | your user config |
| Claude Code, brief at session start | `kontext connect claude-hooks --write` | a `SessionStart` hook in `.claude/settings.json` |
| Codex | `kontext connect codex` / `--write` | prints / appends `[mcp_servers.kontext]` in `~/.codex/config.toml` (a backup is kept) |
| Cursor | `kontext connect cursor --write` | `.cursor/mcp.json` |
| OpenCode | `kontext connect opencode --write` | `opencode.json` |
| Anything reading `AGENTS.md` | `kontext connect agents-md --write` | a marked block in `AGENTS.md` (or `CLAUDE.md`) |

Without `--write`, `kontext connect` only prints what it would add. Existing JSON files are merged, not replaced (comments in JSON files are not preserved).

Agent-specific pages: [Claude Code](../agent-integrations/02-claude-code.md) · [Codex](../agent-integrations/03-codex.md) · [Cursor, OpenCode and others](../agent-integrations/04-other-clients.md).

## Tell agents how to use it

The MCP server sends short instructions to every client on connect. For harnesses that read `AGENTS.md`, `kontext connect agents-md --write` adds the same guidance to the file:

- start each task with `ctx_brief` (pass `focus` with the paths you will touch),
- use `ctx_search` / `ctx_read` for detail and `ctx_why` for "why is this like this?",
- `ctx_capture` durable decisions, conventions and pitfalls,
- run `ctx_prepare_commit` before committing.

## Slash commands (MCP prompts)

kontext publishes three prompts. Claude Code shows them as slash commands:

| Prompt | Claude Code | What it does |
| --- | --- | --- |
| `kontext-init` | `/mcp__kontext__kontext-init` | runs the bootstrap task loop (`ctx_init` → work → `ctx_init_submit`) |
| `kontext-commit` | `/mcp__kontext__kontext-commit` | prepares the current change: promote candidates, capture what is missing |
| `kontext-reflect` | `/mcp__kontext__kontext-reflect` | reviews the session and captures at most three durable items |
| `kontext-distill` | `/mcp__kontext__kontext-distill` | distills durable knowledge from another agent thread or a Superset workspace |

## Several agents, several worktrees

Orchestrators such as Superset run many agents in separate worktrees of one clone. kontext keeps the inbox, the outbox and bootstrap progress in the **git common directory**, so every worktree of a clone shares them; the search index is per worktree (it follows the checked-out branch). State of deleted worktrees is pruned automatically.

## Verify

```sh
kontext call ctx_brief '{"focus": ["src"]}'   # the tool, without a client
kontext status                                # adapters, hooks, inbox, bootstrap progress
```
