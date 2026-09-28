# Code intelligence

kontext's own symbol extraction is deliberately light (regexes that feed module summaries). For precise answers — where is `RunContext` defined, who calls it — plug in a code-intelligence server through a `code` op. `ctx_why <symbol>` uses it to locate the code before looking for decisions and history.

## CodeGraph

A local code knowledge graph with an MCP server.

```sh
codegraph init . && codegraph index .     # once per repository
kontext adapters add codegraph
kontext adapters test codegraph --query RunContext
```

The preset starts `codegraph serve --mcp`, is active only where `.codegraph/` exists, maps `code` to `codegraph_search` and extracts `path:line` locations from its Markdown answer (`split = "### "`, `uri = "re:…"`).

Re-publish CodeGraph's tools through kontext when an agent should use a single MCP server:

```toml
[adapters.codegraph]
expose = ["codegraph_explore", "codegraph_node", "codegraph_callers"]
```

## Serena

A language-server-backed toolkit with MCP support for many languages.

```sh
kontext adapters add serena
kontext adapters inspect serena      # the tools and their schemas
```

The preset starts `serena start-mcp-server --context ide --project <repo>` and maps `code` to `find_symbol`; the symbol argument (`name_path` / `name_path_pattern`) is bound from the tool's schema. Without a `serena` binary, use the `uvx` command shown in the preset's comments.

## Your own

Any tool that finds symbols works — an MCP server, an HTTP service or a CLI. A zero-dependency example with `git grep`:

```toml
[adapters.gitgrep]
driver = "command"
description = "Symbol lookup with git grep (no extra tools)"

[adapters.gitgrep.ops.code]
command = ["git", "grep", "-n", "-I", "-w", "-E", "(fn|struct|class|interface|type|def|func)[[:space:]]+{{query}}"]
format = "lines"
map = { title = "re:^[^:]+:\\d+:\\s*(.*)$", uri = "re:^([^:]+:\\d+)", kind = "=symbol" }
```

```text
$ kontext why Artifact
## Code (gitgrep)
- type Artifact = { apps/ui/src/main.tsx:198
- pub struct Artifact { libs/core/src/lib.rs:75
```

## When to use which

| Need | Tool |
| --- | --- |
| why a path is the way it is | `ctx_why path` (git + knowledge; no code adapter needed) |
| where a symbol lives, then why | `ctx_why Symbol` (code adapter + knowledge) |
| deep navigation, references, refactoring | the code server's own tools (`expose`) |
