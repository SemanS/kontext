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

The user directory is `$KONTEXT_CONFIG_DIR`, else `$XDG_CONFIG_HOME/kontext`, else `~/.config/kontext`. `kontext status` lists the layers that were loaded.

## `[project]`

| Key | Default | |
| --- | --- | --- |
| `name` | set by `kontext init` from the root manifest; otherwise the remote's repository name | shown in the brief; `{{project.name}}` in templates |
| `description` | | |

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
| `threshold_commits` | `20` | flag an active decision once this many commits touched its `paths` since its date; `0` turns it off (see [Freshness](../concepts/04-context-layers.md#freshness)) |

## `[search]` {#search}

| Key | Default | |
| --- | --- | --- |
| `adapters` | `always` | when `ctx_search` and `kontext search` ask the adapters with a `search` op: `always`, or `fallback` — only when the local sources found fewer than `min_local_hits` hits. Naming an adapter in `sources`, or passing `adapters: true` (`--adapters`), always asks |
| `min_local_hits` | `3` | the local hits that are enough in `fallback` mode |

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

## `[secrets]`

| Key | Default | |
| --- | --- | --- |
| `scan` | `store` | `store` (knowledge files), `staged` (every staged text file) or `off` |
| `allow` | `[]` | regexes of lines to ignore |
| `redact` | `[]` | regexes of values masked in agent threads and in what is distilled from them (a client's name, an integration id); keep them in a private layer (`.git/kontext/config.toml`, `~/.config/kontext/repos/<slug>.toml`) |

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
| `jobs` | `2` | autopilot parallelism (also `--jobs`) |
| `llm` | | default autopilot adapter |

## `[vars]`

Global template variables, available to every adapter as `{{vars.<name>}}` (adapter `vars` override them).

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
