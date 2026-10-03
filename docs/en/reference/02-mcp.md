# MCP tools, prompts and resources

Server: `kontext mcp`, JSON-RPC 2.0 over stdio, newline-delimited. Protocol versions 2024-11-05, 2025-03-26, 2025-06-18 and 2025-11-25 (the client's version is echoed when supported). Capabilities: `tools`, `prompts`, `resources`, `logging`. Tool results are text; errors are returned as `isError: true` results.

## Tools

Every tool carries MCP annotations, which clients use to decide what needs approval: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log` and `ctx_threads` are `readOnlyHint: true`; `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init` and `ctx_init_submit` write only the local inbox, the working tree and the git index and are `destructiveHint: false`. All are `openWorldHint: false`.

### Another repository: `dir` {#dir}

`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_inbox` and `ctx_prepare_commit` take `dir`: a path inside another worktree, a submodule or a sibling repository (absolute, or relative to this repository's root). The tool runs there, and its answer starts with the repository it came from.

Without `dir`, kontext still follows the paths it is given:

- `focus` paths, a `ctx_why` target and `ctx_capture` `paths` inside a submodule are answered from the submodule, and captured into its inbox when it keeps team knowledge. A path that exists only inside one submodule, relative to it (`apps/runner` for `extractor/apps/runner`, as the submodule's own `AGENTS.md` names its paths), counts as the submodule's. Absolute paths into another repository or worktree are answered from there.
- A submodule with its own team knowledge gets a section of the brief, and its search hits carry its path: `kx:extractor/<id>`, `git:extractor/<sha>`, `file:extractor/<path>`. `ctx_read` opens them as they are.

### `ctx_brief`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `focus` | string[] | | paths or topic words to prioritise |
| `budget_tokens` | integer | `brief.budget_tokens` (1400) | approximate size |
| `adapters` | boolean | true | include adapter `brief` sections |

References in brackets are the shortest unique prefix of an entry's id (`[2026-09-28-adapters]`), which `ctx_read` resolves. Besides the team knowledge, the brief lists *Local notes* (this clone's inbox) and, in a repository without a store, *Related docs and history* for the focus. See [Context layers](../concepts/04-context-layers.md).

### `ctx_search`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `query` | string | required | words, identifiers, paths |
| `kinds` | string[] | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` |
| `sources` | string[] | all | `local` and/or adapter names |
| `limit` | integer | 10 | maximum hits (up to 50) |
| `budget_tokens` | integer | 1200 | approximate size of the answer |

Each hit: `N. title — snippet [kind · status · date · source] <uri>`.

The local source also covers notes in this clone's inbox (`inbox:<id>`, marked *local note*) and submodules that keep their own team knowledge (marked with their path).

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
| `commits` | string[] | | short shas of the commits it came from |
| `source` | string | | where it was found, e.g. a thread label from `ctx_threads` (kept in the inbox only) |

### `ctx_threads`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `thread` | string | | `claude:<id>`, `codex:<id>`, `superset:<workspace>` (all its threads), a transcript file or `last`; without it, the recent threads are listed |
| `part` | integer | 1 | part of a long transcript |
| `budget_chars` | integer | 20000 | characters per part (up to 80000) |

Compact, redacted transcripts of this repository's agent threads on this machine. A harness notification (Claude Code's `<task-notification>` for a finished background task) becomes one line, and paths in an agent's per-session temp dir are shortened to `<tmp>/`. A part fits one tool result: clients cut longer ones. See [Knowledge from agent threads](../guides/08-distill-threads.md).

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

Returns the report described in [Capture and review](../concepts/03-capture-and-review.md#before-committing-ctx-prepare-commit). It lists the commit trailers and says whether the prepare-commit-msg hook adds them: a clone without kontext's hooks (a fresh clone, a Superset project) needs them added by hand, or `kontext hooks install`.

### `ctx_init`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `count` | integer | 1 | how many tasks to return (up to 5) |
| `bootstrap` | boolean | false | start the knowledge store when the repository has none (only when the user asked for it) |

When the repository has no `.ai/` yet and `bootstrap` is true, `ctx_init` first runs scan, history and render (no hooks) and then returns the first task; without it, it explains how to set kontext up. It never bootstraps a branch when another branch already has the team knowledge.

In a repository without a knowledge store `ctx_capture` accepts only `visibility: private`, and `ctx_inbox` / `ctx_prepare_commit` do not promote.

A capture whose paths all lie in a submodule or another repository that keeps team knowledge goes to that repository's inbox, with the paths rewritten relative to it. Otherwise it stays here, and a note says why (mixed paths, a submodule without a store, paths that exist nowhere).

### `ctx_init_submit`

| Parameter | Type | |
| --- | --- | --- |
| `task_id` | string (required) | `purpose`, `mod:<module>`, `dec:<area>`, `refresh:<module>` |
| `summary` | string | one line, at most ~200 characters |
| `overview` | string | Markdown, 5–15 lines |
| `decisions` | object[] | `{title, decision, summary?, context?, consequences?, paths?, commits?, date?, status?, tags?}` (for `dec:` tasks) |
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
| `kontext-distill` | `thread` (optional) | distill durable knowledge from another thread or a Superset workspace |

## Resources

| URI | Content |
| --- | --- |
| `kontext://brief` | the brief (without adapter sections) |
| `kontext://entry/<id>` | an entry's Markdown (template `kontext://entry/{id}`) |

## Server-initiated requests

The MCP *client* inside kontext (used by `mcp` adapters) answers `ping` and `roots/list` (with the repository root) from the servers it starts; other server requests are declined.
