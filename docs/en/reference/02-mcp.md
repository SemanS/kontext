# MCP tools, prompts and resources

Server: `kontext mcp` — JSON-RPC 2.0 over stdio, newline-delimited. Protocol versions 2024-11-05, 2025-03-26, 2025-06-18 and 2025-11-25 (the client's version is echoed when supported). Capabilities: `tools`, `prompts`, `resources`, `logging`. Tool results are text; errors are returned as `isError: true` results.

## Tools

### `ctx_brief`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `focus` | string[] | | paths or topic words to prioritise |
| `budget_tokens` | integer | `brief.budget_tokens` (1400) | approximate size |
| `adapters` | boolean | true | include adapter `brief` sections |

### `ctx_search`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `query` | string | required | words, identifiers, paths |
| `kinds` | string[] | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` |
| `sources` | string[] | all | `local` and/or adapter names |
| `limit` | integer | 10 | maximum hits (up to 50) |
| `budget_tokens` | integer | 1200 | approximate size of the answer |

Each hit: `N. title — snippet [kind · status · date · source] <uri>`.

### `ctx_read`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `uri` | string | required | `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, an adapter URI, or a plain path |
| `level` | 0 \| 1 \| 2 | 1 | one line, overview, full |

### `ctx_why`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `target` | string | required | path, `path:line`, `path:start-end`, commit sha, or symbol |
| `limit` | integer | 12 | items per section |

### `ctx_log`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `kind` | string | `decision` | kind to list |
| `all` | boolean | false | include superseded / rejected |
| `limit` | integer | 40 | rows |

### `ctx_capture`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `kind` | string | required | `decision`, `convention`, `learning`, `incident` (aliases accepted) |
| `title` | string | required | short statement |
| `body` | string | required | Markdown: context, decision or lesson, consequences |
| `summary` | string | | one line |
| `paths` | string[] | | paths or globs it governs |
| `tags` | string[] | | |
| `visibility` | `team` \| `private` | `capture.default_visibility` | |
| `supersedes` | string[] | | ids of entries it replaces |
| `status` | string | `accepted` for decisions | |
| `promote` | boolean | false | write into the store now instead of the inbox |

### `ctx_inbox`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `action` | `list` \| `show` \| `promote` \| `drop` | `list` | |
| `ids` | string[] | | for show / promote / drop |

### `ctx_prepare_commit`

| Parameter | Type | |
| --- | --- | --- |
| `promote` | string[] | inbox ids to write into the store and stage |
| `drop` | string[] | inbox ids to discard |

Returns the report described in [Capture and review](../concepts/03-capture-and-review.md#before-committing-ctx-prepare-commit).

### `ctx_init`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `count` | integer | 1 | how many tasks to return (up to 5) |

When the repository has no `.ai/` yet, `ctx_init` first runs scan, history and render (no hooks) and then returns the first task.

### `ctx_init_submit`

| Parameter | Type | |
| --- | --- | --- |
| `task_id` | string (required) | `purpose`, `mod:<module>`, `dec:<area>`, `refresh:<module>` |
| `summary` | string | one line, at most ~200 characters |
| `overview` | string | Markdown, 5–15 lines |
| `decisions` | object[] | `{title, decision, summary?, context?, consequences?, paths?, commits?, date?, status?, tags?}` — for `dec:` tasks |
| `learnings` | object[] | `{title, body, paths?, tags?}` |
| `skip` | boolean | skip the task |
| `reason` | string | why it is skipped |

`paths`, `commits` and `tags` also accept comma-separated strings.

### Adapter tools

Tools of MCP adapters with `expose` are listed with their original schemas (names prefixed with `<adapter>_` unless they already start with it); tools declared in adapter config are listed with the schema from `params`. Their descriptions start with `[adapter]`.

## Prompts

| Name | Arguments | Purpose |
| --- | --- | --- |
| `kontext-init` | `tasks` (optional) | bootstrap and deepen the repository's knowledge |
| `kontext-commit` | | prepare the current change for commit |
| `kontext-reflect` | | capture at most three durable items from the session |

## Resources

| URI | Content |
| --- | --- |
| `kontext://brief` | the brief (without adapter sections) |
| `kontext://entry/<id>` | an entry's Markdown (template `kontext://entry/{id}`) |

## Server-initiated requests

The MCP *client* inside kontext (used by `mcp` adapters) answers `ping` and `roots/list` (with the repository root) from the servers it starts; other server requests are declined.
