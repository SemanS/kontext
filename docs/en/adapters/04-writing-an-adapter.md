# Writing an adapter

This walkthrough connects three kinds of systems. Work in your personal config (`~/.config/kontext/config.toml`) and check each step with `kontext adapters list` and `kontext adapters test`.

## An MCP server

Say your team runs a documentation server that speaks MCP.

**1. Declare it and see what it offers.**

```toml
[adapters.teamdocs]
driver = "mcp"
command = ["teamdocs-mcp", "--stdio"]
when = { command = "teamdocs-mcp" }
```

```sh
kontext adapters inspect teamdocs
```

```text
search_docs
    (query:string, max_results:number, space:string)
    Full-text search across the team's documentation spaces
get_page
    (page_id:string)
    Read one page
```

**2. Map capabilities to tools.** Arguments are bound from the schemas (`query` ← the query, `max_results` ← the limit); only set `args` for fixed values:

```toml
[adapters.teamdocs.ops.search]
tool = "search_docs"
args = { query = "{{query}}", max_results = "{{limit}}", space = "engineering" }
```

**3. Map results.** Run `kontext search <something> --source teamdocs --json`. If the hits look wrong, look at the raw payload (re-publish the tool with `expose` and call it, or use the server's own CLI) and add `items` / `map`. Give every hit a URI with a scheme, so `ctx_read` knows which adapter to ask:

```toml
[adapters.teamdocs.ops.search]
tool = "search_docs"
args = { query = "{{query}}", max_results = "{{limit}}", space = "engineering" }
items = "results[*]"
map = { title = "title", snippet = "excerpt", uri = "tpl:teamdocs://{{id}}" }

[adapters.teamdocs.ops.read]
tool = "get_page"
args = { page_id = "{{uri|after:teamdocs://}}" }
owns = ["teamdocs://"]
```

`tpl:` renders a template against the item; `after:` keeps what follows the prefix. `ctx_read teamdocs://page-7` now reaches `get_page`.

**4. Optionally re-publish tools** so agents can call them directly through kontext:

```toml
expose = ["get_page"]
```

## An HTTP API

A notes service with a JSON API:

```toml
[adapters.notes]
driver = "http"
base_url = "{{env.NOTES_URL|default:http://localhost:8080}}"
headers = { Authorization = "Bearer {{env.NOTES_TOKEN}}" }
when = { env = "NOTES_TOKEN" }

[adapters.notes.ops.search]
method = "GET"
path = "/v1/search"
query = { q = "{{query}}", limit = "{{limit}}" }
items = "data[*]"
map = { title = "title", snippet = "highlight|body", uri = "url", date = "updated_at" }

[adapters.notes.ops.store]
method = "PUT"
path = "/v1/notes/{{repo.slug}}/{{id}}"
body = { title = "{{title}}", content = "{{markdown}}", tags = "{{tags}}", repo = "{{source.repo}}" }

[[adapters.notes.on]]
event = "sync"
op = "store"
```

- A body value that is exactly one `{{…}}` keeps its JSON type (`tags` stays an array); values that are empty disappear.
- HTTP status ≥ 400 is an error (the response body is shown).
- Test the store op end to end with `kontext sync --all` in a scratch clone, then `kontext outbox`.

## A command

Anything with a CLI can be an adapter. A search over a local knowledge folder with ripgrep:

```toml
[adapters.wiki]
driver = "command"
when = { command = "rg", exists = "{{env.HOME}}/wiki" }

[adapters.wiki.ops.search]
command = ["rg", "--json", "--max-count", "3", "--ignore-case", "--", "{{query}}", "{{env.HOME}}/wiki"]
format = "jsonl"                       # one JSON value per line → a list of items
where = { type = "match" }             # ripgrep also prints begin/end/summary records
map = { title = "data.path.text", snippet = "data.lines.text", uri = "tpl:wiki://{{data.path.text}}", kind = "=note" }

[adapters.wiki.ops.read]
command = ["cat", "--", "{{uri|after:wiki://}}"]
format = "text"
owns = ["wiki://"]
```

`kontext search tuesdays --source wiki` returns the matching lines, and `ctx_read wiki://…` returns the file.

Rules for command ops:

- the argument vector is passed as is (no shell, no globbing); put `--` before user-supplied values when the program supports it,
- `stdin = "{{prompt}}"` feeds long input without hitting argument limits,
- `output_file = true` gives the program a temporary file (`{{output_file}}`) for its answer, for tools that log to stdout,
- non-zero exit codes are errors; the timeout kills the process.

## An LLM for autopilot

Any model works if it can answer a prompt:

```toml
[adapters.my-llm]
driver = "command"
[adapters.my-llm.ops.llm]
command = ["my-llm", "--model", "large", "--quiet"]
stdin = "{{prompt}}"
format = "text"
```

```sh
kontext init --deepen --llm my-llm --max 3
```

The answer must contain one JSON object; prose around it is tolerated.

## Checklist

- [ ] `when` keeps it inactive where it cannot work
- [ ] `kontext adapters test <name>` passes
- [ ] search hits have a meaningful title, a snippet and a URI
- [ ] secrets come from `{{env.X}}`, never from the config file
- [ ] a `read` op exists for every URI prefix your hits use (or hits have no URI)
