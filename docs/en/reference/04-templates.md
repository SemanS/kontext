# Templates and result mapping

## Templates

Every string in an adapter definition is a template. Expressions are written `{{path | filter | filter:arg}}`.

### Context

| Variable | Example |
| --- | --- |
| `repo.id` | `github.com/acme/shop` |
| `repo.slug` | `github.com-acme-shop` |
| `repo.name` | `shop` (from the remote) |
| `repo.dir` | directory name of the main worktree |
| `repo.root` | absolute path of the current worktree |
| `repo.remote`, `repo.branch`, `repo.worktree` | |
| `project.name` | `[project] name`, else the root manifest's name, else `repo.name` |
| `vars.<name>` | global `[vars]` merged with the adapter's `vars` |
| `env.<NAME>` | environment variable |
| `now.date`, `now.iso` | `2026-09-28`, RFC 3339 timestamp |
| `adapter.name` | |

Op inputs:

| Op | Variables |
| --- | --- |
| `search`, `code` | `query`, `limit` |
| `history` | `target`, `query` (same value), `limit` |
| `read` | `uri`, `level` |
| `brief` | `query`, `focus` |
| `llm` | `prompt` |
| event deliveries (`store`, …) | `id`, `kind`, `title`, `status`, `date`, `summary`, `tags`, `paths`, `body`, `markdown`, `path`, `visibility`, `source.repo`, `source.branch` |
| declared tools | `args.<name>` and each argument by name |
| command ops with `output_file = true` | `output_file` |

### Filters

| Filter | Result |
| --- | --- |
| `urlencode` | percent-encoded |
| `json` | JSON text of the value |
| `lower`, `upper`, `trim`, `slug` | |
| `first_line` | |
| `join:<sep>` | joins a list (`join:, ` · `join: `) |
| `truncate:<n>` | at most n characters |
| `after:<sep>`, `before:<sep>` | the part after / before the first separator (the whole value when absent) |
| `default:<literal>` | used when the value is missing or empty (`{{env.URL\|default:http://localhost:1933}}`) |
| `int` | a string parsed as an integer |

Quoted literals work as values: `{{'text'|upper}}`.

### Types

- A JSON value that is **exactly one** expression keeps its type: `limit = "{{limit}}"` sends a number, `tags = "{{tags}}"` sends an array.
- Anything else is rendered as a string.
- Object members that render to `null` are dropped, so optional arguments disappear instead of being sent empty.

## Paths {#paths}

`items`, `text`, `where` keys and `map` values use a small path language over JSON:

| Syntax | Meaning |
| --- | --- |
| `a.b.c` | object keys |
| `a[0]` | array index |
| `a[*]` | every element of an array |
| `*` | every value of an object (or element of an array) |
| `$` or empty | the root |
| `a\|b` | the first alternative that exists (in `map`) |

Examples: `result.memories[*]`, `result.*[*]` (every list under `result`), `data[*].attributes`, `results[0].title`.

## Mapping hits

For `search`, `history` and `code` ops each item becomes a hit with `title`, `snippet`, `uri`, `score`, `kind` and `date`.

| `map` value | Meaning |
| --- | --- |
| `name` / `a.b` / `a\|b` | a path into the item |
| `re:<regex>` | first capture group (or the whole match) of the regex on the item's text |
| `tpl:<template>` | a template rendered against the item (`tpl:notes://{{id}}`) |
| `=<literal>` | a constant |

Fallbacks when a field is not mapped:

| Field | Tried keys |
| --- | --- |
| `title` | `title`, `name`, `subject`, `heading`, `uri`, `path`, `id` |
| `snippet` | `snippet`, `abstract`, `summary`, `content`, `text`, `description`, `body`, `overview` |
| `uri` | `uri`, `url`, `path`, `file`, `id` |
| `score` | `score`, `relevance`, `similarity`, `rank` (else by position) |
| `kind` | `kind`, `type`, `context_type`, `category` |
| `date` | `date`, `created_at`, `updated_at`, `time`, `timestamp` |

Items without a title and a snippet are dropped. URIs that should be readable through `ctx_read` need a scheme (`notes://…`) and a `read` op that `owns` the prefix.

## Payload formats

| `format` | Parsing |
| --- | --- |
| `auto` (default) | JSON when it parses (log lines before the JSON are skipped), else text |
| `json` | JSON or null |
| `jsonl` | one JSON value per line → a list |
| `lines` | non-empty lines → a list of strings |
| `text` | the raw text |

MCP tool results are unwrapped first: `structuredContent` if present, else the text content (parsed as JSON when possible).
