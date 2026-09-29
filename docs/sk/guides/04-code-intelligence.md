# Inteligencia kódu

Vlastná extrakcia symbolov, ktorú robí kontext, je zámerne jednoduchá (regulárne výrazy, ktoré napĺňajú zhrnutia modulov). Pre presné odpovede (kde je definovaný `RunContext`, kto ho volá) pripoj server pre inteligenciu kódu cez operáciu `code`. `ctx_why <symbol>` ho použije na nájdenie kódu predtým, než začne hľadať rozhodnutia a históriu.

## CodeGraph

Lokálny znalostný graf kódu s MCP serverom.

```sh
codegraph init . && codegraph index .     # once per repository
kontext adapters add codegraph
kontext adapters test codegraph --query RunContext
```

Preset spúšťa `codegraph serve --mcp`, je aktívny iba tam, kde existuje `.codegraph/`, mapuje `code` na `codegraph_search` a z jeho odpovede vo formáte Markdown extrahuje pozície `path:line` (`split = "### "`, `uri = "re:…"`).

Ak má agent používať jediný MCP server, znovu publikuj nástroje CodeGraphu cez kontext:

```toml
[adapters.codegraph]
expose = ["codegraph_explore", "codegraph_node", "codegraph_callers"]
```

## Serena

Sada nástrojov postavená na language serveroch, s podporou MCP pre mnoho jazykov.

```sh
kontext adapters add serena
kontext adapters inspect serena      # the tools and their schemas
```

Preset spúšťa `serena start-mcp-server --context ide --project <repo>` a mapuje `code` na `find_symbol`; argument so symbolom (`name_path` / `name_path_pattern`) sa naviaže podľa schémy nástroja. Ak nemáš binárku `serena`, použi príkaz `uvx` uvedený v komentároch presetu.

## Vlastný nástroj

Poslúži akýkoľvek nástroj, ktorý vie nájsť symboly: MCP server, HTTP služba alebo CLI. Príklad bez závislostí s `git grep`:

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

## Kedy použiť čo

| Potreba | Nástroj |
| --- | --- |
| prečo je cesta taká, aká je | `ctx_why path` (git + znalosti; adaptér pre kód netreba) |
| kde sa symbol nachádza, a potom prečo | `ctx_why Symbol` (adaptér pre kód + znalosti) |
| hĺbková navigácia, referencie, refaktoring | vlastné nástroje servera pre kód (`expose`) |
