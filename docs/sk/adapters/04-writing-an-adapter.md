# Vlastný adaptér

Tento návod pripojí tri druhy systémov. Pracuj vo svojej osobnej konfigurácii (`~/.config/kontext/config.toml`) a každý krok over cez `kontext adapters list` a `kontext adapters test`.

## MCP server

Povedzme, že tvoj tím prevádzkuje dokumentačný server, ktorý hovorí MCP.

**1. Deklaruj ho a pozri sa, čo ponúka.**

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

**2. Namapuj schopnosti na nástroje.** Argumenty sa naviažu zo schém (`query` ← dopyt, `max_results` ← limit); `args` nastav len pre pevné hodnoty:

```toml
[adapters.teamdocs.ops.search]
tool = "search_docs"
args = { query = "{{query}}", max_results = "{{limit}}", space = "engineering" }
```

**3. Namapuj výsledky.** Spusti `kontext search <something> --source teamdocs --json`. Ak výsledky nevyzerajú dobre, pozri sa na surový payload (publikuj nástroj ďalej cez `expose` a zavolaj ho, alebo použi vlastné CLI servera) a pridaj `items` / `map`. Každému výsledku daj URI so schémou, aby `ctx_read` vedel, ktorého adaptéra sa má opýtať:

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

`tpl:` vykreslí šablónu voči položke; `after:` ponechá to, čo nasleduje za prefixom. `ctx_read teamdocs://page-7` sa teraz dostane až k `get_page`.

**4. Voliteľne publikuj nástroje ďalej**, aby ich agenti mohli volať priamo cez kontext:

```toml
expose = ["get_page"]
```

## HTTP API

Služba s poznámkami a JSON API:

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

- Hodnota v tele, ktorá je presne jeden `{{…}}`, si zachová svoj JSON typ (`tags` zostane poľom); prázdne hodnoty zmiznú.
- HTTP stav ≥ 400 je chyba (zobrazí sa telo odpovede).
- Operáciu store otestuj end-to-end cez `kontext sync --all` v skúšobnom klone a potom `kontext outbox`.

## Príkaz

Adaptérom môže byť čokoľvek s CLI. Vyhľadávanie v lokálnom priečinku so znalosťami cez ripgrep:

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

`kontext search tuesdays --source wiki` vráti zodpovedajúce riadky a `ctx_read wiki://…` vráti súbor.

Pravidlá pre operácie s príkazmi:

- vektor argumentov sa odovzdá tak, ako je – žiadny shell, žiadny globbing; pred hodnoty od používateľa daj `--`, ak to program podporuje,
- `stdin = "{{prompt}}"` pošle dlhý vstup bez narazenia na limity argumentov,
- `output_file = true` dá programu dočasný súbor (`{{output_file}}`) na odpoveď – pre nástroje, ktoré logujú na stdout,
- nenulové návratové kódy sú chyby; časový limit proces ukončí.

## LLM pre autopilota

Funguje akýkoľvek model, ktorý vie odpovedať na prompt:

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

Odpoveď musí obsahovať jeden JSON objekt; text okolo neho sa toleruje.

## Kontrolný zoznam

- [ ] `when` ho drží neaktívny tam, kde nemôže fungovať
- [ ] `kontext adapters test <name>` prejde
- [ ] výsledky vyhľadávania majú zmysluplný titulok, úryvok a URI
- [ ] tajné údaje idú z `{{env.X}}`, nikdy nie z konfiguračného súboru
- [ ] pre každý prefix URI, ktorý tvoje výsledky používajú, existuje operácia `read` (alebo výsledky nemajú URI)
