# MCP tools, prompts and resources

Server: `kontext mcp`, JSON-RPC 2.0 over stdio, newline-delimited. Capabilities: `tools`, `prompts`, `resources`. Tool failures are returned as `isError: true` results; an unknown tool is the protocol error `-32602`, a panic inside one request `-32603`.

## Protocol {#protocol}

The server answers both eras of MCP:

- **Stateless (2026-07-28).** `server/discover` returns the capabilities, instructions and server info; every request may carry `_meta` with the protocol version instead of an `initialize` handshake. Results carry `resultType`, and list results carry `ttlMs` and `cacheScope`. An unknown version is `-32022`.
- **Legacy handshake.** `initialize` with 2024-11-05, 2025-03-26, 2025-06-18 or 2025-11-25 echoes the client's version; an unknown one falls back to 2025-11-25.

Lists are returned in a deterministic order. `resources/list` pages with a cursor. `tools.listChanged` is true only when adapters `expose` tools. `notifications/cancelled` stops the request and kills a running verify command.

**Instructions.** The first 512 characters of the server instructions stand alone (what kontext holds, call `ctx_brief` first, when to search); the whole stays under 4 096 characters. `ctx_brief` carries `_meta["anthropic/alwaysLoad"]: true`, so hosts that defer tools behind tool search still load it; `ctx_read` and `ctx_api` carry `maxResultSizeChars`.

## Tools

[`ctx_api`](./05-application-api.md) exposes the versioned application service with `operation`,
`input` and optional `dir`. It returns `structuredContent` and `outputSchema` as well as a complete
JSON text copy.

There are sixteen tools: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads`, `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_verify`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init`, `ctx_init_submit` and `ctx_api`.

**Structured results.** `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox` and `ctx_drift` declare an `outputSchema` and return `structuredContent` beside the text, which stays as it was. Hits and governing entries also come as `resource_link` blocks to `kontext://entry/{id}`. The other tools return text.

**Parameter aliases.** Parameters gain aliases, never renames: `id` and `uri` are interchangeable where a tool names one entry, `focus` and `paths`, `limit` and `max`, `budget_tokens` everywhere (`budget_chars` stays on `ctx_threads`), and `reason`, `why` and `because`.

Every tool carries MCP annotations, which clients use to decide what needs approval: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift` and `ctx_threads` are `readOnlyHint: true`; `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init` and `ctx_init_submit` write only the local inbox, the working tree and the git index and are `destructiveHint: false`. These tools retain `openWorldHint: false`. `ctx_verify` runs a pinned shell command and is `destructiveHint: true`, `openWorldHint: true`. The mixed `ctx_api` surface is conservatively `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false`, `openWorldHint: true`: configured LLM and event adapters may access external services.

### Another repository: `dir` {#dir}

`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_verify`, `ctx_drift`, `ctx_inbox` and `ctx_prepare_commit` take `dir`: a path inside another worktree, submodule or repository, absolute or relative to this root. It must lie in this repository, one of its worktrees or submodules, or a repository below the same root; anything else is refused. Without it, the paths a tool is given decide which repository answers. See [Several repositories](../concepts/09-several-repositories.md).

### `ctx_brief`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `focus` | string[] | | paths or topic words to prioritise |
| `budget_tokens` | integer | `brief.budget_tokens` (1400) | approximate size |
| `adapters` | boolean | true | include adapter `brief` sections |

With a focus, a *Governs &lt;focus&gt; (N)* section comes first and is never dropped silently: one line per governing entry (full id, title, marks, commits since), ordered by path specificity, then oldest first; what does not fit is named on an `also governing:` line. Other sections end with `… N more (K govern your focus)` when trimmed. A focus item with `/`, a glob character or a known extension is a path even before the file exists. References in brackets are full ids (`[2026-09-28-adapters-are-configuration]`); copy them as they are into `supersedes` and `[[links]]`. A line is marked when the entry is anything short of accepted, approved knowledge: its status when it is not `accepted` (`(proposed)`), `local, unapproved` when its file is new, edited or staged in the working tree, `review due`, `challenged locally`, and `N sources` when it carries evidence, e.g. `Move search to OpenSearch (proposed; 2 sources)`. An accepted, committed entry has no mark. `## Changed since <rev>` lists what changed since this worktree's last brief (at most five lines; *Changed in the last 14 days* without one). The brief also lists *Local notes* (this clone's inbox) and, without a store, *Related docs and history*. See [Context layers](../concepts/04-context-layers.md).

### `ctx_search`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `query` | string | required | words, identifiers, paths |
| `kinds` | string[] | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` |
| `sources` | string[] | all | `local` and/or adapter names |
| `limit` | integer | 10 | maximum hits (up to 50) |
| `adapters` | boolean | `[search] adapters` | `true` asks the search adapters even when local hits suffice, `false` never asks them |
| `budget_tokens` | integer | 1200 | approximate size of the answer |

Each hit: `N. title — snippet [kind · status · date · source] <uri>`. The status is left out when it is `accepted`; marks such as `local, unapproved` and `review due` follow it. A hit that only opens like the one above it (the same first words, a different claim) is not dropped: it follows as `↳ similar, differs: … <uri>` with the part that differs. Only proven copies are dropped: the same URI, an adapter's copy of a local entry and a hit whose whole normalised text equals an earlier one; a superseded twin is never kept over its active successor. Reviewed entries rank above doc chunks at equal score (`[search] weights`); a superseded hit is demoted and shown with `→ superseded by [id]`, never hidden.

The local source also covers notes in this clone's inbox (`inbox:<id>`, marked *local note*) and submodules that keep their own team knowledge (marked with their path).

### `ctx_read`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `uri` | string | required | `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, an adapter URI, or a plain path |
| `level` | 0 \| 1 \| 2 | 1 | one line, overview, full |
| `offset`, `part` | integer | | page through a long file |

`id` is an alias of `uri`. A plain argument prefers an existing path and the answer says what it resolved to. Levels 0 and 1 show `superseded_by` with a link and the marks (local, challenged, review due). `file:` refuses `.git/`, `.git-credentials` and `.ssh`.

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
| `body` | string | required | Markdown: context, decision or lesson, consequences; optionally `## Alternatives` for a decision |
| `summary` | string | | one line |
| `paths` | string[] | | paths or globs it governs |
| `tags` | string[] | | |
| `visibility` | `team` \| `private` | `capture.default_visibility` | |
| `supersedes` | string[] | | full ids of entries it replaces; an ambiguous id is refused |
| `extends` | string[] | | full ids of entries it builds on; kept through the inbox and promotion |
| `status` | string | `accepted` for decisions | |
| `promote` | boolean | false | write into the store now instead of the inbox |
| `commits` | string[] | | short shas of the commits it came from |
| `source` | string | | where it was found, e.g. a thread label from `ctx_threads` (kept in the inbox only) |
| `review_by` | string | `review_after` of the kind | `YYYY-MM-DD`: when to look at it again (a rule that holds during a migration) |
| `evidence` | object[] | | `{ref, quote?}`: `pr:<id>`, `issue:<id>`, `task:<id>`, `code:<repo>@<rev>:<path>#L<a>-<b>`, `url:<url>`; see [Evidence](./05-application-api.md#maintenance) |
| `verify` | string | | the command that proved it (a learning or decision you checked with a test carries that test): one line, at most 500 characters, run by `ctx_verify` |

The answer lists related entries as notes (`related, check: <title> <kx:id> — <reason>`), with a model's label such as `possible conflict, check` when [`entries.related`](./05-application-api.md#related) already asked one for this text. Decide whether it supersedes anything; the note never says so.

### `ctx_challenge`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `id` | string | required | the entry: `kx:<id>`, an id or its unique prefix |
| `why` | string | required | why it no longer holds (aliases `reason`, `because`) |
| `visibility` | `team` \| `private` | `team` | |

Records that an entry no longer holds, as a local inbox candidate of kind `challenge` (against the entry and the commit it was read at). Nothing changes until review: the brief marks the entry `challenged locally`, and `ctx_prepare_commit` offers to retire it or to record a replacement that supersedes it. The challenge stays *resolving* until the entry's new blob lands in `HEAD`.

### `ctx_confirm`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `id` | string | | the entry: `kx:<id>`, an id or its unique prefix |
| `ids` | string[] | | several entries at once; more than five are recorded as `bulk` |
| `because` | string | | one line: what the entry was checked against |

Records that an entry still holds after the code it governs changed: sets its [`checked`](../concepts/02-knowledge-store.md#checked) time to now (UTC, to the second), changes nothing else in the file, appends an `agent` line with `HEAD` and the reason to the [verification ledger](../concepts/03-capture-and-review.md#verifications), and stages both, so the confirmation is reviewed with the change. Freshness and drift count from it. An agent confirms only what it checked against the code; an entry that no longer holds is challenged or superseded instead. An agent's confirmation is shown as `stamped by agent`, not as a verification, and MCP never writes a `read` line; `ctx_verify` proves more where the entry names a pinned command.

### `ctx_verify`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `id` | string | required | the entry: `kx:<id>`, an id or its unique prefix |
| `because` | string | | one line: why it was run |

Runs the entry's [`verify`](../concepts/03-capture-and-review.md#verify) command from the repository root, but only when the command text is pinned in your `trust.toml` (`kontext trust --verify <id>`, from a terminal). Pinned, it runs with an environment allowlist and a timeout, appends a `verify` line with pass or fail, `HEAD` and a dirty-tree mark to the ledger, stages it, and answers with the outcome and the redacted end of the output. Unpinned, nothing runs: the answer is `untrusted_verify` with the command, its hash and the pin line, and the agent may run the command in its own sandbox and record the result with `ctx_confirm`. An entry without a command, or an inactive one, is refused. `destructiveHint: true`, `openWorldHint: true`; see [Security](../concepts/08-security.md#verify).

### `ctx_drift`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `base` | string | merge base of `HEAD` with the remote's default branch, else `HEAD` | start of the range, a git revision |
| `head` | string | the working tree | end of the range, a git revision; without it uncommitted and untracked changes count too |
| `kinds` | string[] | decision, convention, learning, incident | kinds to check |

The knowledge a change left behind: active entries whose `paths` cover files changed in the range and whose own file was not edited or confirmed in it, each with the changed files and commits and what to do about it. Deterministic, no model. See [Drift](../concepts/03-capture-and-review.md#drift); `entries.drift` in `ctx_api` returns the same as JSON.

### `ctx_threads`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `thread` | string | | `claude:<id>`, `codex:<id>`, `superset:<workspace>` (all its threads), a transcript file under a known transcript root, or `last`; without it, the recent threads are listed. Any other path and `-` are refused |
| `part` | integer | 1 | part of a long transcript |
| `budget_chars` | integer | 20000 | characters per part (up to 80000) |

Compact, redacted transcripts of this repository's agent threads on this machine. A harness notification (Claude Code's `<task-notification>` for a finished background task) becomes one line, and paths in an agent's per-session temp dir are shortened to `<tmp>/`. A part fits one tool result: clients cut longer ones. See [Knowledge from agent threads](../guides/08-distill-threads.md).

### `ctx_inbox`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `action` | `list` \| `show` \| `promote` \| `drop` \| `verify` | `list` | |
| `ids` | string[] | | for show / promote / drop; one id for verify. Exact ids or a unique prefix; `drop` removes only this clone's own candidates |
| `verify` | string | | for `action=verify`: the command that proved the draft (as in `ctx_capture`); `""` clears it. Promotion keeps it |

### `ctx_prepare_commit`

| Parameter | Type | |
| --- | --- | --- |
| `promote` | string[] | inbox ids to write into the store and stage |
| `drop` | string[] | inbox ids to discard |
| `retire` | string[] | entries challenged in this worktree (or the challenges): status `deprecated`, the challenge as the reason in a `## Retired` note, staged |

Returns the report described in [Capture and review](../concepts/03-capture-and-review.md#before-committing-ctx-prepare-commit). With nothing staged it reads the working tree's changes against `HEAD` (as under jj) and says so. It lists the commit trailers and says whether the prepare-commit-msg hook adds them: a clone without kontext's hooks (a fresh clone, a Superset project) needs them added by hand, or `kontext hooks install`.

### `ctx_init`

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `count` | integer | 1 | how many tasks to return (up to 5) |
| `bootstrap` | boolean | false | start the knowledge store when the repository has none (only when the user asked for it) |

When the repository has no `.ai/` yet and `bootstrap` is true, `ctx_init` first runs scan, history and render (no hooks) and then returns the first task; without it, it explains how to set kontext up. It never bootstraps a branch when another branch already has the team knowledge.

In a repository without a knowledge store `ctx_capture` accepts only `visibility: private`, and `ctx_inbox` / `ctx_prepare_commit` do not promote.

A capture whose paths all belong to another repository with team knowledge goes to that repository's inbox. See [Several repositories](../concepts/09-several-repositories.md#limits).

### `ctx_init_submit`

| Parameter | Type | |
| --- | --- | --- |
| `task_id` | string (required) | `purpose`, `mod:<module>`, `dec:<area>`, `refresh:<module>` |
| `summary` | string | one line, at most ~200 characters |
| `overview` | string | Markdown, 5–15 lines |
| `decisions` | object[] | `{title, decision, summary?, context?, alternatives?, consequences?, paths?, commits?, date?, status?, tags?}` (for `dec:` tasks) |
| `learnings` | object[] | `{title, body, paths?, tags?}` |
| `skip` | boolean | skip the task |
| `reason` | string | why it is skipped |

`paths`, `commits` and `tags` also accept comma-separated strings.

### Adapter tools

Tools of MCP adapters with `expose` are listed with their original schemas (names prefixed with `<adapter>_` unless they already start with it); tools declared in adapter config are listed with the schema from `params`. Their descriptions start with `[adapter]`. Exposed tools keep their upstream title and annotations; missing ones default to `readOnlyHint: false`, `openWorldHint: true`.

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
