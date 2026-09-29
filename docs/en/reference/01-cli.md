# CLI

```text
kontext [-C <dir>] <command> [options]
```

`-C, --dir <dir>` runs as if kontext was started in `<dir>` (works with every command). `kontext <command> --help` prints the options below.

## Bootstrap

### `kontext init`

Runs scan → history → render → wire → deepen. Safe to re-run.

| Option | Default | Meaning |
| --- | --- | --- |
| `--no-hooks` | | do not install git hooks (remembered for the clone); `kontext hooks install` adds them later |
| `--connect <targets>` | | also wire clients, comma-separated (`claude,claude-hooks,cursor,opencode,agents-md`) |
| `--deepen` | | run pending deepen tasks with an LLM adapter |
| `--llm <adapter>` | `init.llm` | adapter with an `llm` op |
| `--jobs <n>` | 2 | parallel LLM calls |
| `--max <n>` | 50 | tasks per run |

| Subcommand | Meaning |
| --- | --- |
| `kontext init status` | phase status and deepen progress |
| `kontext init next [-n <count>] [--inline]` | print the next task(s); `--inline` attaches file excerpts (for pasting into a chat model) |
| `kontext init submit <file\|->` | record a task result (JSON: `task_id`, `summary`, `overview`, `decisions`, `learnings`) |
| `kontext init skip <task> [--reason <text>]` | skip a task |

## Everyday

### `kontext brief`

| Option | Default | Meaning |
| --- | --- | --- |
| `-f, --focus <path\|word>` | | prioritise these paths or topics (repeatable) |
| `--budget <tokens>` | `brief.budget_tokens` | approximate size |
| `--format <fmt>` | `text` | `text`, `json`, `claude-hook` (SessionStart `additionalContext`) |
| `--no-adapters` | | skip adapter `brief` sections |

### `kontext search <query…>`

| Option | Default | Meaning |
| --- | --- | --- |
| `-k, --kind <kind>` | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` (repeatable) |
| `-s, --source <name>` | all | `local` and/or adapter names (repeatable) |
| `-n, --limit <n>` | 10 | maximum hits |
| `--json` | | hits as JSON |

### `kontext read <uri>`

`-l, --level <0|1|2>` (default 2). URIs: `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, a plain repository path, or an adapter URI.

### `kontext why <target>`

`target` is a path, `path:line`, `path:start-end`, a commit sha, or a symbol/topic. `-n, --limit <n>` (default 12) items per section.

### `kontext log`

| Option | Default | Meaning |
| --- | --- | --- |
| `-k, --kind <kind>` | `decision` | kind to list |
| `--all` | | include superseded and rejected entries |
| `-n, --limit <n>` | 50 | rows |
| `--json` | | rows as JSON |

## Capturing and committing

### `kontext capture`

| Option | Meaning |
| --- | --- |
| `-k, --kind <kind>` | required: `decision`, `convention`, `learning` (`pitfall`) or `incident` |
| `-t, --title <text>` | short statement (required) |
| `-b, --body <text>` / `--file <path\|->` / stdin | the body |
| `--summary <text>` | one line (derived from the body when omitted) |
| `-p, --paths <a,b>` | paths or globs it governs |
| `--tags <a,b>` | tags |
| `--supersedes <id,…>` | entries it replaces |
| `--status <status>` | `proposed` or `accepted` (decisions default to accepted) |
| `--private` | keep it personal (never promoted) |
| `--promote` | write into the store now instead of the inbox |

### `kontext inbox [list | show <id> | drop <id…>]`

Local candidates. `list` is the default.

### `kontext promote <id…> [--no-stage]`

Write candidates into the store and `git add` them.

### `kontext prepare-commit [--promote <id>]… [--drop <id>]…`

The report agents get from `ctx_prepare_commit`.

### `kontext distill [<file>…] [--claude <id|last>]… [--codex <id|last>]… [--superset [<workspace>]]`

Distill knowledge from agent threads into the inbox; without a thread it lists this repository's threads on this machine. See [Knowledge from agent threads](../guides/08-distill-threads.md).

| Option | Default | Meaning |
| --- | --- | --- |
| `--llm <adapter>` | `init.llm` | adapter with an `llm` op |
| `--max <n>` | 5 | most entries per thread |
| `--max-parts <n>` | 12 | most transcript parts per thread |
| `--jobs <n>` | 3 | parallel model calls |
| `--list` | | list the threads instead |
| `--dry-run` | | print the findings, capture nothing |

### `kontext check [--staged]`

Validate all entries (or exactly what is staged) and scan them for secrets. Exit status 1 on errors.

## Git hooks

| Command | Meaning |
| --- | --- |
| `kontext hooks install [--dir <dir>]` | install into `core.hooksPath`, `.git/hooks`, or `<dir>` |
| `kontext hooks uninstall [--dir <dir>]` | remove kontext's blocks, restore chained originals |
| `kontext hooks status` | which hooks carry the block |
| `kontext hook <name> [args…]` | entry point called by the hooks (`pre-commit`, `prepare-commit-msg`, `post-commit`, `post-merge`, `post-rewrite`) |

## Adapters

| Command | Meaning |
| --- | --- |
| `kontext adapters [list]` | active and inactive adapters, with the reason |
| `kontext adapters test [<name>] [--query <q>]` | health check and a sample call of each op |
| `kontext adapters inspect <name>` | an MCP adapter's tools and input schemas |
| `kontext adapters add <preset> [--scope user\|repo\|local] [--var k=v]… [--force]` | append a preset to a config layer |
| `kontext presets [<name>]` | list presets, or print one |
| `kontext trust` | trust the adapters declared in the shared config (after showing them) |
| `kontext outbox [list \| flush [--quiet]]` | queued deliveries; deliver now |
| `kontext sync [--all]` | queue `sync` events for knowledge at `HEAD` (`--all`: every entry once) |

## Clients and servers

| Command | Meaning |
| --- | --- |
| `kontext mcp` | run the MCP server on stdio |
| `kontext connect <target> [--write]` | `claude`, `claude-hooks`, `codex`, `cursor`, `opencode`, `agents-md` |
| `kontext call <tool> ['<json>']` | call an agent tool locally, e.g. `kontext call ctx_why '{"target":"src/api"}'` |

## Maintenance

| Command | Meaning |
| --- | --- |
| `kontext status` | identity, config layers, store, index, inbox, init progress, adapters, hooks, outbox |
| `kontext reindex` | rebuild the local search index |

## Environment variables

| Variable | Meaning |
| --- | --- |
| `KONTEXT_CONFIG_DIR` | user config directory (default `$XDG_CONFIG_HOME/kontext` or `~/.config/kontext`) |
| `KONTEXT_DIR` | repository for `kontext mcp` when not started inside it |
| `KONTEXT_SKIP=1` | hooks do nothing (one-off bypass) |
| `KONTEXT_TRUST_REPO_ADAPTERS=1` | trust repository-declared adapters without `kontext trust` (CI, sandboxes) |
