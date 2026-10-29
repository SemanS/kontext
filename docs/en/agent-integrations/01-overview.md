# Agent integrations

kontext is an MCP server: `kontext mcp` speaks JSON-RPC 2.0 over stdio. It answers both the stateless 2026-07-28 revision (`server/discover`, no `initialize`) and the legacy handshake for protocol versions 2024-11-05 to 2025-11-25. Every MCP client gets the same sixteen tools, prompts and resources.

## What agents get

**Tools** (see the [MCP reference](../reference/02-mcp.md) for parameters):

| Tool | When an agent should call it |
| --- | --- |
| `ctx_brief` | at the start of every task, with `focus` = the paths or topic it will work on |
| `ctx_search` | to find decisions, docs, history or memory about something |
| `ctx_read` | to open a hit at L0/L1/L2 |
| `ctx_why` | before changing code whose reason is unclear (`path`, `path:line`, commit, symbol) |
| `ctx_log` | to see the decision timeline |
| `ctx_drift` | to see which entries a change left behind |
| `ctx_threads` | to read past agent sessions (Claude Code, Codex, Superset) |
| `ctx_capture` | when something durable was decided or learned |
| `ctx_challenge` | when an entry looks wrong: a challenge for a person to resolve |
| `ctx_confirm` | after checking that an entry still holds (recorded as an `agent` stamp) |
| `ctx_verify` | to run an entry's pinned `verify` command; an unpinned one is returned for the agent to run itself |
| `ctx_inbox` | to list, show, promote or drop captured candidates |
| `ctx_prepare_commit` | before every commit |
| `ctx_init`, `ctx_init_submit` | to bootstrap and deepen the repository's knowledge |
| `ctx_api` | to call an [application API](../reference/05-application-api.md) operation |

Read tools (`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads`) are `readOnlyHint: true`. `ctx_verify` is `destructiveHint: true` and `openWorldHint: true`; `ctx_api` is `openWorldHint: true`. Tools re-published from MCP adapters (`expose`) keep their upstream annotations and appear next to these, as do tools declared in adapter config.

**Structured results**: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox` and `ctx_drift` return `structuredContent` beside the text, and hits carry `resource_link` blocks to `kontext://entry/{id}`.

**Prompts**: `kontext-init`, `kontext-commit`, `kontext-reflect` (slash commands in Claude Code).

**Resources**: `kontext://brief` and `kontext://entry/<id>` for clients that browse resources.

**Instructions**: on connect the server tells the agent how to use the tools above. The first 512 characters stand alone (what kontext holds, call `ctx_brief` first, when to search) and the whole stays under 4 096 characters, the limits Codex and Claude Code read. `ctx_brief` carries `alwaysLoad`, so hosts that defer tools behind search still load it.

**Hooks**: `kontext hook --host claude|codex|cursor` delivers the brief at session start and the governing entries before an edit, opt-in with `kontext connect <host> --hooks`. See [Hooks](../guides/09-hooks.md).

## Behaviour of the server

- Requests are handled concurrently. A panic in one request answers `-32603`; the server keeps running.
- The repository is discovered from the working directory (or `-C <dir>`, or `KONTEXT_DIR`). The server also starts outside a repository, says so, and still serves calls with `dir`.
- Configuration edits (a new `.ai/kontext.toml`, a changed adapter) are picked up without a restart; a config that does not parse comes back as a warning in the next result.
- An index refreshed by another process (a CLI capture, `kontext status`, a second session) is reloaded before the next answer. The branch is read per request.
- `notifications/cancelled` stops the request and kills a running verify command.
- Adapter MCP servers are started on first use and kept for the lifetime of the session.

## Clients

| Client | Guide |
| --- | --- |
| Claude Code | [Claude Code](./02-claude-code.md) |
| Codex | [Codex](./03-codex.md) |
| Cursor, OpenCode, Superset, other MCP clients | [Other clients](./04-other-clients.md) |

## Without MCP

Everything is also a CLI command (`kontext brief`, `kontext search`, …), and `kontext call <tool> '<json>'` runs any tool locally, which is handy for harnesses without MCP support and for scripts.
