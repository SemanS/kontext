# Configuration

Configuration is TOML. Every key has a default; most repositories need only what `kontext init` writes.

## Layers

Later layers win. Tables are merged key by key; arrays are replaced.

| # | File | Scope | Adapters |
| --- | --- | --- | --- |
| 1 | built-in defaults | – | – |
| 2 | `~/.config/kontext/config.toml` | you, every repository | trusted |
| 3 | `~/.config/kontext/repos/<slug>.toml` | you, one repository (slug from `kontext status`) | trusted |
| 4 | `<repo>/.kontext.toml` or `<repo>/<store.dir>/kontext.toml` (default `.ai/kontext.toml`) | the team (committed) | only after `kontext trust` |
| 5 | `<git-common-dir>/kontext/config.toml` | this clone | trusted |

The user directory is `$KONTEXT_CONFIG_DIR`, else `$XDG_CONFIG_HOME/kontext`, else `~/.config/kontext`. `kontext status` lists the layers that were loaded. Unknown keys and a key that a later layer shadows are reported as warnings by `kontext status` and `kontext check`.

A few keys take only your own layers (2, 3 and 5), so a cloned repository cannot set them: `[verify] pass_env`. Two lists are joined across layers instead of replaced: `secrets.redact` and `secrets.allow`. Shared `[vars]` never override the `vars` of an adapter declared in your own layers.

## `[project]`

| Key | Default | |
| --- | --- | --- |
| `name` | set by `kontext init` from the root manifest; otherwise the remote's repository name | shown in the brief; `{{project.name}}` in templates. In the shared config it must be a slug and is covered by the trust pin |

## `[store]` {#store}

| Key | Default | |
| --- | --- | --- |
| `dir` | `.ai` | the store directory |
| `style` | `frontmatter` | `frontmatter` or `fields` for new entries |
| `index_file` | `true` | maintain `<dir>/README.md` |
| `max_body_lines` | `80` | warn above this many non-empty body lines (0 = off) |

### `[store.kinds.<kind>]` {#store-kinds}

| Key | Default | |
| --- | --- | --- |
| `dir` | `decisions`, `conventions`, `learnings`, `incidents`, `architecture` | relative to `store.dir` |
| `path` | | relative to the repository root; wins over `dir` (e.g. `docs/adr`) |
| `style` | `store.style` | |
| `numbering` | `date` for decisions, `none` otherwise | `date`, `sequential`, `none` |
| `trailer` | `Decision`, `Convention`, `Learning`, `Incident` | commit trailer key; none for architecture |
| `review_after` | none | default `review_by` for new entries of this kind, counted from their date: `90d`, `12w`, `6m` (30-day months) or `1y`. Off unless set |

Custom kinds work too: add a table with a `dir`.

```toml
# temporary workarounds should be looked at again within a quarter
[store.kinds.incident]
review_after = "90d"
```

## `[sources]`

Documents indexed besides the store.

| Key | Default |
| --- | --- |
| `include` | `["/*.md", "**/README.md", "**/AGENTS.md", "**/CLAUDE.md", "docs/**/*.md", "doc/**/*.md", "adr/**/*.md"]` |
| `exclude` | `["**/node_modules/**", "**/CHANGELOG.md", "**/vendor/**", "**/fixtures/**", "**/testdata/**", "**/.github/ISSUE_TEMPLATE/**"]` |
| `max_bytes` | `300000` |

Globs follow git conventions: a pattern without `/` matches at any depth, a leading `/` anchors it, `**` crosses directories, `{a,b}` alternatives.

## `[index]`

| Key | Default | |
| --- | --- | --- |
| `commits` | `true` | index commit history |
| `max_commits` | `5000` | how far back |

## `[brief]`

| Key | Default |
| --- | --- |
| `budget_tokens` | `1400` |
| `max_decisions` | `12` |
| `max_modules` | `14` |

## `[freshness]`

| Key | Default | |
| --- | --- | --- |
| `threshold_commits` | `20` | flag an active decision, convention or learning once this many commits touched its `paths` since its date, its `checked` confirmation or the last commit that changed it, whichever is latest; `0` turns it off (see [Freshness](../concepts/04-context-layers.md#freshness)) |

## `[search]` {#search}

| Key | Default | |
| --- | --- | --- |
| `adapters` | `always` | when `ctx_search` and `kontext search` ask the adapters with a `search` op: `always`, or `fallback` — only when the local sources found fewer than `min_local_hits` hits. Naming an adapter in `sources`, or passing `adapters: true` (`--adapters`), always asks |
| `min_local_hits` | `3` | the local hits that are enough in `fallback` mode |
| `weights` | entries above doc chunks and commits | a table of multipliers per source (`entry`, `doc`, `commit`, or an adapter name) applied to the score; the defaults are set on the graded eval set (`tests/eval/qrels.tsv`), change them only with a measurement |

`fallback` saves an adapter's round trip when the local answer is already there (on the evaluation
set, a hub with one second of latency added about a second to every search in `always` mode and nothing
in `fallback` mode). It also means a semantic memory is not asked when BM25 found anything, which is
why `always` stays the default.

## `[capture]`

| Key | Default | |
| --- | --- | --- |
| `default_visibility` | `team` | `team` or `private` |
| `redact` | `true` | redact secrets in captures |

## `[hooks]`

| Key | Default | |
| --- | --- | --- |
| `validate` | `true` | validate staged entries in pre-commit |
| `trailers` | `true` | add trailers in prepare-commit-msg |
| `remind_inbox` | `true` | mention inbox candidates touching staged paths |
| `enabled` | `true` | agent host hooks (`kontext hook --host`) answer; `false` (or `KONTEXT_HOOKS=off`) makes them print nothing. They run only where `kontext connect <host> --hooks` installed them |
| `session_budget_tokens` | built in (`kontext status` shows it) | one budget for everything kontext injects into a session: `ctx_brief` and the host hooks draw from it; not a second budget |
| `stop_note` | `false` | a Stop hook that adds a one-line capture reminder as `additionalContext`; it never blocks |

The first three keys govern the git hooks; the last three the agent host hooks (see [Hooks](../guides/09-hooks.md)).

## `[secrets]`

| Key | Default | |
| --- | --- | --- |
| `scan` | `store` | `store` (knowledge files), `staged` (every staged text file) or `off` |
| `allow` | `[]` | regexes of lines to ignore; joined across layers |
| `redact` | `[]` | regexes of values masked in agent threads and in what is distilled from them (a client's name, an integration id); keep them in a private layer (`.git/kontext/config.toml`, `~/.config/kontext/repos/<slug>.toml`). Your patterns are joined with the repository's, never replaced by them. A pattern that does not compile is an error in `status` and `check`, and distill, threads and autopilot refuse to send anything to a model until it is fixed |

## `[init]`

| Key | Default | |
| --- | --- | --- |
| `max_module_docs` | `40` | module documents to write |
| `min_module_files` | `3` | code files for an *inferred* module |
| `exclude` | `[]` | extra globs to skip |
| `history_max_commits` | `3000` | commits to mine |
| `max_decision_tasks` | `20` | `dec:` tasks |
| `task_max_files` | `12` | key files per module task |
| `task_inline_chars` | `24000` | attached source per autopilot task |
| `jobs` | `2` | autopilot parallelism; `--jobs` overrides it |
| `llm` | | default autopilot adapter |
| `decision_words` | `[]` | extra words that mark a decision-shaped commit, in your team's language (for example `rozhodli`, `namiesto`, `prechádzame`); added to the language-neutral signals |
| `bot_authors` | `[]` | extra author names or emails left out of history scoring, besides the built-in bots (dependabot, renovate, `*[bot]`) |

## `[vars]`

Global template variables, available to every adapter as `{{vars.<name>}}` (adapter `vars` override them).

Shared `[vars]` are covered by the trust pin and never override the `vars` of an adapter declared in your own layers.

## `[verify]` {#verify}

| Key | Default | |
| --- | --- | --- |
| `pass_env` | `[]` | names of extra environment variables a verify command receives, besides `PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR` and `TERM`. Read only from your own layers; the shared config cannot set it |

A verify command runs only after `kontext trust --verify`; see [Security](../concepts/08-security.md#verify).

## `[adapters.<name>]`

See [Configuring adapters](../adapters/02-configuration.md).

## Example: a repository with existing ADRs

```toml
[project]
name = "shop"

[store]
dir = ".ai"

[store.kinds.decision]
path = "docs/adr"
style = "fields"
numbering = "sequential"
trailer = "Decision"

[sources]
include = ["/*.md", "**/README.md", "docs/**/*.md", "handbook/**/*.md"]

[init]
exclude = ["legacy/**"]
```
