# CLI

```text
kontext [-C <dir>] <command> [options]
```

`-C, --dir <dir>` runs as if kontext was started in `<dir>` (works with every command). `kontext <command> --help` prints the options below.

**Exit status.** 0 on success, 1 on errors, 2 when `drift --fail` or `ci --strict` lists an entry, 64 for a usage error (an unknown option or a missing argument).

## Bootstrap

Applications use [`kontext api <operation> [JSON|-]`](./05-application-api.md) for versioned JSON
results and errors, including knowledge queries, extraction, idempotent staging and reviewed export.

### `kontext init`

Runs scan → history → render → wire → deepen. Safe to re-run.

| Option | Default | Meaning |
| --- | --- | --- |
| `--no-hooks` | | do not install git hooks (remembered for the clone; only this explicit flag records the opt-out); `kontext hooks install` adds them later |
| `--connect <targets>` | | also wire clients, comma-separated (`claude,claude-hooks,cursor,opencode,agents-md`) |
| `--deepen` | | run pending deepen tasks with an LLM adapter |
| `--llm <adapter>` | `init.llm` | adapter with an `llm` op |
| `--jobs <n>` | `init.jobs` (2) | parallel LLM calls |
| `--max <n>` | 50 | tasks per run |

| Subcommand | Meaning |
| --- | --- |
| `kontext init status` | phase status and deepen progress |
| `kontext init next [-n <count>] [--inline]` | print the next task(s); `--inline` attaches file excerpts (for pasting into a chat model) |
| `kontext init submit <file\|->` | record a task result (JSON: `task_id`, `summary`, `overview`, `decisions`, `learnings`) |
| `kontext init skip <task> [--reason <text>]` | skip a task (`mod:<module>` included); it is not offered again |

## Everyday

### `kontext brief`

| Option | Default | Meaning |
| --- | --- | --- |
| `-f, --focus <path\|word>` | | prioritise these paths or topics (repeatable) |
| `--budget <tokens>` | `brief.budget_tokens` | approximate size |
| `--format <fmt>` | `text` | `text`, `json` (the structure of `ctx_brief`: governing section, state, counts, currency, what was trimmed), `claude-hook` (SessionStart `additionalContext`) |
| `--no-adapters` | | skip adapter `brief` sections |

### `kontext search <query…>`

| Option | Default | Meaning |
| --- | --- | --- |
| `-k, --kind <kind>` | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` (repeatable) |
| `-s, --source <name>` | all | `local` and/or adapter names (repeatable) |
| `-n, --limit <n>` | 10 | maximum hits |
| `--json` | | hits as JSON |
| `--adapters` | | ask the search adapters even when local hits suffice (see [`[search]`](./03-configuration.md#search)) |

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
| `--supersedes <id,…>` | entries it replaces (full ids; an ambiguous one is refused) |
| `--extends <id,…>` | entries it builds on |
| `--status <status>` | `proposed` or `accepted` (decisions default to accepted) |
| `--private` | keep it personal (never promoted) |
| `--promote` | write into the store now instead of the inbox |
| `--review-by <YYYY-MM-DD>` | when to look at it again (else the kind's `review_after`, if set) |
| `--verify <command>` | the command that proved it: one line, at most 500 characters, run by [`kontext verify`](../concepts/03-capture-and-review.md#verify) |

### `kontext challenge <id> <why…> [--private]`

Say that an entry no longer holds, and why. The challenge waits in the local inbox; `prepare-commit` offers to retire the entry or to record a replacement that supersedes it.

### `kontext confirm <id…> [--because <text>] [--kind read] | --repair | --migrate-legacy`

Confirm that entries still hold: sets [`checked`](../concepts/02-knowledge-store.md#checked) to the current UTC time (`YYYY-MM-DDTHH:MM:SSZ`) in each entry's own file, changes nothing else, appends a line per entry to the [verification ledger](../concepts/03-capture-and-review.md#verifications) and stages both. The line is `read` when stdin and stdout are a terminal, `agent` otherwise, and `bulk` when more than five ids are given. `--kind read` forces `read` without a terminal and prints a warning; a terminal is not proof that a person read the entry. `--because` keeps a one-line reason in the ledger. Refuses an unknown or inactive entry, an entry that is not committed yet and a file with unstaged edits. Exit status 1 when any id fails.

`--repair` rewrites the `checked` of every active entry whose file is behind its newest ledger confirmation (a merge took the older side) and stages it; exit status 1 when one could not be rewritten.

`--migrate-legacy` appends a `bulk` line for every active entry whose only evidence is a `checked:` date (kind `cache`, `verified: false`), so freshness and drift have a ledger line to count from. It does not make those entries verified.

### `kontext verify <id…> [--because <text>] [--json]`

Run each entry's [`verify`](../concepts/03-capture-and-review.md#verify) command with `sh -c` from the repository root, but only when its text is pinned with `kontext trust --verify`; an unpinned command is refused with `untrusted_verify`, the command, its hash and the line to pin it. The command gets an environment allowlist (see [`[verify]`](./03-configuration.md#verify)) and is stopped after `KONTEXT_VERIFY_TIMEOUT_SECS` (default 120) together with its process group. A `verify` line with pass or fail, the commit and a dirty-tree mark goes to the ledger and is staged. Prints the outcome, and the redacted end of the output when it failed; `--json` prints the `drafts.verify` results. Refuses an entry without a command and an inactive entry. Exit status 1 when a command failed or an id was refused.

### `kontext drift`

The entries a range of commits left behind: active entries whose `paths` cover changed files while their own file was not edited or confirmed in the range. See [Drift](../concepts/03-capture-and-review.md#drift).

| Option | Default | Meaning |
| --- | --- | --- |
| `--base <rev>` | merge base of `HEAD` with `origin/HEAD` (`origin/main`, `origin/master`), else `HEAD` | start of the range |
| `--head <rev>` | the working tree | end of the range; without it uncommitted and untracked changes count too |
| `-k, --kind <kind>` | decision, convention, learning, incident | kinds to check (repeatable) |
| `--json` | | the `entries.drift` result as JSON |
| `--fail` | | exit with status 2 when an entry is listed: the gate for a team's check script or CI, which also covers commits made without hooks (jj). Without a baseline drift says why and exits non-zero |

### `kontext inbox [list | show <id> | drop <id…> | verify <id> <command|--clear>]`

Local candidates. `list` is the default. `verify` sets the command that proved a draft (validated like `--verify` of `capture`); `--clear` removes it. Promotion keeps it.

### `kontext promote <id…> [--no-stage]`

Write candidates into the store and `git add` them.

### `kontext prepare-commit [--promote <id>]… [--drop <id>]… [--retire <id>]…`

The report agents get from `ctx_prepare_commit`. With nothing staged it reads the working tree's changes against `HEAD` (untracked files included) and says so, which is what to run before `jj commit`. `--retire` retires an entry challenged in this worktree: status `deprecated`, the challenge as the reason in a `## Retired` note, staged.

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

Validate all entries (or exactly what is staged) and scan them for secrets. An unparsable entry, an invalid `secrets.redact` pattern and an ambiguous `supersedes` are errors; a one-sided supersede, unparsable or future ledger lines and unknown config keys are warnings. Exit status 1 on errors.

### `kontext repair [--dry-run]`

Write the missing side of one-sided supersedes (`status` and `superseded_by` of the replaced entry, as single lines) and stage it. `--dry-run` prints the edits only.

### `kontext ci [--base <rev>] [--format text|markdown] [--strict]`

For a CI job: `kontext check` over every entry plus `kontext drift` over the committed range `base..HEAD` (`--base` defaults as for `drift`). `--format markdown` suits a GitHub job summary or a pull request comment. Exit status 1 on check errors; with `--strict` also 2 when drift lists an entry, otherwise drift is a warning (status 0). See [Validate knowledge in CI](../guides/07-ci.md).

## Git hooks

| Command | Meaning |
| --- | --- |
| `kontext hooks install [--hooks-dir <dir>]` | install into `core.hooksPath`, `.git/hooks`, or `<dir>` |
| `kontext hooks uninstall [--hooks-dir <dir>]` | remove kontext's blocks, restore chained originals |
| `kontext hooks status` | which hooks carry the block |
| `kontext hook <name> [args…]` | entry point called by the hooks (`pre-commit`, `prepare-commit-msg`, `post-commit`, `post-merge`, `post-rewrite`) |
| `kontext hook --host claude\|codex\|cursor` | entry point for agent host hooks: reads the host's hook JSON on stdin and prints the answer in its shape; fail-open (exit 0, no output, on any error). See [Hooks](../guides/09-hooks.md) |

## Adapters

| Command | Meaning |
| --- | --- |
| `kontext adapters [list]` | active and inactive adapters, with the reason |
| `kontext adapters test [<name>] [--query <q>]` | health check and a sample call of each op |
| `kontext adapters inspect <name>` | an MCP adapter's tools and input schemas |
| `kontext adapters add <preset> [--scope user\|repo\|local] [--var k=v]… [--force]` | append a preset to a config layer |
| `kontext presets [<name>]` | list presets, or print one |
| `kontext trust` | trust the adapters, `[vars]` and `project.name` declared in the shared config (after showing them); terminal only; a legacy FNV pin is replaced, never upgraded |
| `kontext trust --verify [<id>\|--all]` | pin the `verify` command of an entry, or of every entry, after printing each command; terminal only |
| `kontext outbox [list \| flush [--quiet]]` | queued deliveries; deliver now |
| `kontext outbox retry` | move dead-lettered events (`outbox.dead.jsonl`) back to the queue |
| `kontext sync [--all]` | queue `sync` events for knowledge at `HEAD` (`--all`: every entry once) |

## Clients and servers

| Command | Meaning |
| --- | --- |
| `kontext mcp` | run the MCP server on stdio |
| `kontext connect <target> [--write] [--hooks]` | `claude`, `claude-hooks`, `skills`, `codex`, `cursor`, `opencode`, `agents-md`; `--hooks` adds the host hooks for `claude`, `codex` and `cursor` |
| `kontext call <tool> ['<json>']` | call an agent tool locally, e.g. `kontext call ctx_why '{"target":"src/api"}'` |

## Maintenance

| Command | Meaning |
| --- | --- |
| `kontext status` | identity, config layers, store, index, entries that may need a refresh, entries past their `review_by` date, inbox, init progress, adapters, hooks, outbox and its dead letters, invalid redaction patterns |
| `kontext reindex` | rebuild the local search index |

## Environment variables

| Variable | Meaning |
| --- | --- |
| `KONTEXT_CONFIG_DIR` | user config directory (default `$XDG_CONFIG_HOME/kontext` or `~/.config/kontext`) |
| `KONTEXT_DIR` | repository for `kontext mcp` when not started inside it |
| `KONTEXT_SKIP=1` | hooks do nothing (one-off bypass) |
| `KONTEXT_HOOKS=off` | agent host hooks (`kontext hook --host`) print nothing |
| `KONTEXT_VERIFY_TIMEOUT_SECS` | timeout of a verify command (default 120) |
| `KONTEXT_TRUST_REPO_ADAPTERS=1` | trust repository-declared adapters without `kontext trust` (CI, sandboxes) |
