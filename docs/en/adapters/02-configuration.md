# Configuring adapters

Adapters live under `[adapters.<name>]` in any [configuration layer](../reference/03-configuration.md#layers). Personal adapters belong in `~/.config/kontext/config.toml`; adapters declared in a repository's shared config run only after [`kontext trust`](../concepts/08-security.md#trust-for-repository-declared-adapters).

## Adapter fields

| Field | Driver | Meaning |
| --- | --- | --- |
| `driver` | all | `mcp`, `http` or `command` |
| `enabled` | all | `false` switches it off |
| `description` | all | shown by `kontext adapters list` and in tool descriptions |
| `when` | all | activation conditions (see below) |
| `vars` | all | variables for templates (`{{vars.x}}`), themselves templated |
| `timeout_ms` | all | per-call timeout (default 10000) |
| `weight` | all | search-result weight when merging (default 0.9; local results are 1.0) |
| `command` | mcp, command | argument vector: the MCP server to start, or the default command |
| `env`, `cwd` | mcp, command | environment and working directory (default: repository root) |
| `url` | mcp, http | streamable-HTTP MCP endpoint, or the HTTP base URL |
| `base_url` | http | base URL for op `path`s |
| `headers` | mcp (HTTP), http | request headers |
| `expose` | mcp | `"none"` (default), `"all"`, or a list of tool names to re-publish |
| `prefix` | mcp | prefix for re-published tool names (default `<adapter>_`, skipped when the tool already starts with it) |
| `ops.<op>` | all | capability operations |
| `on` | all | event subscriptions |
| `tools` | all | tools declared in config |

### `when`

```toml
when = { command = "codegraph", file = ".codegraph" }        # both must hold
when = { repo = "github.com/acme/(shop|billing)" }           # regex on the repository id
when = { env = "NOTES_TOKEN" }                                # variable must be set
when = { exists = "{{env.HOME}}/.config/notes/{{repo.slug}}.json" }   # templated; `*` allowed in the last segment
```

An adapter whose conditions fail is listed as inactive with the reason, and never called.

## Ops

```toml
[adapters.<name>.ops.<op>]
# mcp
tool = "search_notes"
args = { query = "{{query}}", limit = "{{limit}}" }   # optional
# http
method = "POST"                      # default: POST with a body, GET without
path = "/api/search"                 # appended to base_url; or `url` for an absolute URL
query = { q = "{{query}}" }          # URL-encoded; empty values are dropped
headers = { X-Trace = "kontext" }
body = { query = "{{query}}" }       # JSON; a string body is sent as-is
# command
command = ["rg", "--json", "{{query}}"]
stdin = "{{prompt}}"
output_file = true                   # the program writes its answer to {{output_file}}
cwd = "{{repo.root}}"
env = { NO_COLOR = "1" }
# all drivers
format = "auto"                      # auto | json | jsonl | lines | text
timeout_ms = 20000
# result mapping (search, history, code)
items = "result.*[*]"
split = "### "
where = { type = "match" }           # keep only items whose field equals the value
map = { title = "name", snippet = "summary", uri = "url", score = "score", kind = "type", date = "created_at" }
# text answers (read, brief, llm, store receipts)
text = "result"
# read
owns = ["notes://"]
```

### MCP argument binding

When an MCP op has no `args`, kontext reads the tool's input schema and fills parameters by name:

| Role | Parameter names tried (in order) | Value |
| --- | --- | --- |
| query (search, code, brief) | `query`, `q`, `search`, `search_query`, `text`, `pattern`, `substring_pattern`, `keyword(s)`, `term`, `question`, `topic`, `name_path_pattern`, `name_path`, `symbol`, `symbol_name`, `name`, `input`, `prompt` | the query |
| target (history) | `target`, `path`, `file`, `file_path`, `filepath`, `relative_path`, `commit`, `query`, `q`, `text` | path, `path:line`, sha or topic |
| uri (read) | `uri`, `url`, `path`, `id`, `resource`, `name` | the URI |
| content (store) | `content`, `text`, `memory`, `body`, `markdown`, `message`, `data`, `note` | the entry as Markdown |
| title, tags (store) | `title`, `subject`, `name`, `key` · `tags`, `labels`, `categories` | |
| limit | `limit`, `max_results`, `top_k`, `k`, `n`, `count`, `max`, `num_results`, `size`, `maxFiles` | |
| project | `projectPath`, `project_path`, `project`, `repo`, `repository`, `cwd`, `root`, `workspace`, `directory` | repository root |
| prompt (llm) | `prompt`, `input`, `message`, `text`, `query` | the prompt |

A required string parameter that matched nothing receives the primary value (the query, target, URI, content or prompt). Values are converted to the parameter's JSON type. `kontext adapters inspect <name>` prints the tools and their schemas.

### Result mapping

1. **Payload.** MCP results become their `structuredContent`, or the text content parsed as JSON when possible, or plain text. HTTP and command output is parsed per `format` (`auto` tolerates log lines before the JSON).
2. **Items.** `items` selects the list with a small path language ([reference](../reference/04-templates.md#paths)); `split` cuts a text answer at lines starting with the prefix (each piece becomes `{title, text, raw}`); otherwise kontext takes a top-level array or the first array under `results`, `items`, `hits`, `data`, `matches`, `entries`, `memories` or `documents`, or treats the whole payload as one item. `where` then keeps only the items whose fields equal the given values.
3. **Fields.** `map` picks each hit field from the item: a path, alternatives `a|b`, `re:<regex>` (first capture group, applied to the item's text), `tpl:<template>` (rendered against the item, e.g. `tpl:notes://{{id}}`), or `=literal`. Unmapped fields fall back to common names (`title`/`name`/…, `snippet`/`abstract`/`summary`/`content`/…, `uri`/`url`/`path`/…, `score`/`relevance`/…).

## Events

```toml
[[adapters.<name>.on]]
event = "sync"                      # capture | promote | sync
op = "store"                        # any op of this adapter
visibility = "private"              # optional filter (capture events)
kinds = ["decision", "incident"]    # optional filter
```

The event payload is available to the op's templates: `id`, `kind`, `title`, `status`, `date`, `summary`, `tags`, `paths`, `body`, `markdown` (the whole file), `path`, `visibility`, `source.repo`, `source.branch`.

## Tools

Declare a new MCP tool backed by one of the adapter's calls:

```toml
[[adapters.notes.tools]]
name = "notes_recent"
description = "The ten most recent team notes"
params = { tag = "string", limit = "integer" }       # `!` marks required: { q = "string!" }
op = { method = "GET", path = "/recent", query = { tag = "{{tag}}", n = "{{limit}}" }, text = "items" }
# or op = "search" to reuse an existing op
```

Arguments are available as `{{args.x}}` and directly as `{{x}}`. The result is returned to the agent as text. `params` may also be a full JSON schema.

Re-publishing an MCP server's own tools:

```toml
[adapters.codegraph]
driver = "mcp"
command = ["codegraph", "serve", "--mcp"]
expose = ["codegraph_explore", "codegraph_node"]   # or "all"
```

## Templates

All string fields are templates: `{{repo.id}}`, `{{repo.slug}}`, `{{repo.name}}`, `{{repo.dir}}`, `{{repo.root}}`, `{{repo.branch}}`, `{{project.name}}`, `{{vars.x}}`, `{{env.X}}`, `{{now.date}}`, and the op's inputs. Filters such as `{{query|urlencode}}` and `{{env.URL|default:http://localhost:1933}}` are described in [Templates and result mapping](../reference/04-templates.md).
