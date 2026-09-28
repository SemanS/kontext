# Adapters

An adapter connects kontext to an outside system — a memory store, a session archive, a code-intelligence server, an LLM, a chat webhook. Adapters are **configuration only**: kontext's code never names a product.

## Three ideas

**Capabilities (ops).** What kontext may ask of an adapter:

| Op | Used by | Returns |
| --- | --- | --- |
| `search` | `ctx_search` (federated) | hits |
| `read` | `ctx_read` for URIs the adapter `owns` | text |
| `history` | `ctx_why` (path, commit, topic) | hits |
| `code` | `ctx_why <symbol>` | hits (locations) |
| `brief` | a section of `ctx_brief` | text |
| `store` | event deliveries | a receipt |
| `llm` | `kontext init --deepen` | text |
| `health` | `kontext adapters test` | text |

You can define additional op names (for example a second store op for private notes) and point events at them.

**Events.** What happens in kontext that an adapter may want to know:

| Event | When | Payload |
| --- | --- | --- |
| `capture` | a candidate enters the inbox | the entry and its `visibility` (`team` / `private`) |
| `promote` | a candidate is written into the store | the entry |
| `sync` | knowledge at `HEAD` changed (after commit, merge, rebase) | the entry as committed |

Subscriptions map an event to an op, optionally filtered by visibility and kinds.

**Drivers.** How an op is executed:

| Driver | Talks to | Op fields |
| --- | --- | --- |
| `mcp` | an MCP server over stdio (`command`) or streamable HTTP (`url`) | `tool`, `args` (optional: bound from the tool's schema) |
| `http` | an HTTP API | `method`, `path`/`url`, `query`, `headers`, `body` |
| `command` | a local program | `command`, `stdin`, `cwd`, `env`, `output_file` |

## A complete example

```toml
[adapters.notes]
driver = "http"
base_url = "http://localhost:8080"
headers = { Authorization = "Bearer {{env.NOTES_TOKEN}}" }

[adapters.notes.ops.search]
method = "GET"
path = "/search"
query = { q = "{{query}}", n = "{{limit}}" }
items = "results[*]"
map = { title = "name", snippet = "excerpt", uri = "url" }

[adapters.notes.ops.store]
method = "PUT"
path = "/notes/{{repo.slug}}/{{id}}"
body = { title = "{{title}}", markdown = "{{markdown}}", tags = "{{tags}}" }

[[adapters.notes.on]]
event = "sync"
op = "store"
kinds = ["decision", "convention"]
```

`ctx_search` now includes hits from the notes service, and every decision or convention that reaches `HEAD` is mirrored into it.

## Lifecycle and failure handling

- Adapters are built lazily on first use. MCP servers are started once per `kontext mcp` process and reused; they are told the repository root through `roots/list`.
- `when` conditions (a command on `PATH`, a file in the repository, a repository-id regex, an environment variable, a templated path that must exist) switch an adapter off silently where it does not apply.
- Fan-out calls run in parallel with deadlines (12 s for search/code/history, 8 s for brief).
- After an error an adapter cools down for 30 seconds instead of slowing every call.
- Event deliveries are retried from the outbox.

## Federating MCP tools

An MCP adapter can re-publish its server's tools through kontext (`expose = "all"` or a list of tool names), so an agent needs only one MCP server. Tools can also be **declared** in config and backed by any driver — a new MCP tool without code. See [Configuring adapters](./02-configuration.md#tools).

## Next

- [Configuring adapters](./02-configuration.md) — every field
- [Presets](./03-presets.md) — ready-made adapters
- [Writing an adapter](./04-writing-an-adapter.md) — a walkthrough
