# Agent integrations

kontext is an MCP server: `kontext mcp` speaks JSON-RPC 2.0 over stdio (protocol versions 2024-11-05 to 2025-11-25). Every MCP client gets the same tools, prompts and resources.

## What agents get

**Tools** — see the [MCP reference](../reference/02-mcp.md) for parameters:

| Tool | When an agent should call it |
| --- | --- |
| `ctx_brief` | at the start of every task, with `focus` = the paths or topic it will work on |
| `ctx_search` | to find decisions, docs, history or memory about something |
| `ctx_read` | to open a hit at L0/L1/L2 |
| `ctx_why` | before changing code whose reason is unclear (`path`, `path:line`, commit, symbol) |
| `ctx_log` | to see the decision timeline |
| `ctx_capture` | when something durable was decided or learned |
| `ctx_inbox` | to list, show, promote or drop captured candidates |
| `ctx_prepare_commit` | before every commit |
| `ctx_init`, `ctx_init_submit` | to bootstrap and deepen the repository's knowledge |

Tools re-published from MCP adapters (`expose`) and tools declared in adapter config appear next to these.

**Prompts** — `kontext-init`, `kontext-commit`, `kontext-reflect` (slash commands in Claude Code).

**Resources** — `kontext://brief` and `kontext://entry/<id>` for clients that browse resources.

**Instructions** — on connect the server tells the agent, in five sentences, how to use the tools above.

## Behaviour of the server

- Requests are handled concurrently.
- The repository is discovered from the working directory (or `-C <dir>`, or `KONTEXT_DIR`). The server also starts outside a repository and answers with a clear error until it can open one.
- Configuration edits (a new `.ai/kontext.toml`, a changed adapter) are picked up without a restart.
- Adapter MCP servers are started on first use and kept for the lifetime of the session.

## Clients

| Client | Guide |
| --- | --- |
| Claude Code | [Claude Code](./02-claude-code.md) |
| Codex | [Codex](./03-codex.md) |
| Cursor, OpenCode, Superset, other MCP clients | [Other clients](./04-other-clients.md) |

## Without MCP

Everything is also a CLI command (`kontext brief`, `kontext search`, …), and `kontext call <tool> '<json>'` runs any tool locally — handy for harnesses without MCP support and for scripts.
