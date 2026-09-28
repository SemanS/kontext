# Konfigurácia adaptérov

Adaptéry žijú pod `[adapters.<name>]` v ktorejkoľvek [vrstve konfigurácie](../reference/03-configuration.md#layers). Osobné adaptéry patria do `~/.config/kontext/config.toml`; adaptéry deklarované v zdieľanej konfigurácii repozitára sa spustia až po [`kontext trust`](../concepts/08-security.md#trust-for-repository-declared-adapters).

## Polia adaptéra

| Pole | Driver | Význam |
| --- | --- | --- |
| `driver` | všetky | `mcp`, `http` alebo `command` |
| `enabled` | všetky | `false` ho vypne |
| `description` | všetky | zobrazí sa v `kontext adapters list` a v popisoch nástrojov |
| `when` | všetky | podmienky aktivácie (pozri nižšie) |
| `vars` | všetky | premenné pre šablóny (`{{vars.x}}`), samy sú šablónami |
| `timeout_ms` | všetky | časový limit jedného volania (predvolene 10000) |
| `weight` | všetky | váha výsledkov vyhľadávania pri zlučovaní (predvolene 0,9; lokálne výsledky majú 1,0) |
| `command` | mcp, command | vektor argumentov: MCP server, ktorý sa má spustiť, alebo predvolený príkaz |
| `env`, `cwd` | mcp, command | prostredie a pracovný adresár (predvolene koreň repozitára) |
| `url` | mcp, http | MCP endpoint cez streamable HTTP alebo základná HTTP URL |
| `base_url` | http | základná URL pre `path` jednotlivých operácií |
| `headers` | mcp (HTTP), http | hlavičky požiadaviek |
| `expose` | mcp | `"none"` (predvolene), `"all"` alebo zoznam názvov nástrojov na publikovanie |
| `prefix` | mcp | prefix pre ďalej publikované nástroje (predvolene `<adapter>_`, vynechá sa, ak ním nástroj už začína) |
| `ops.<op>` | všetky | operácie schopností |
| `on` | všetky | odbery udalostí |
| `tools` | všetky | nástroje deklarované v konfigurácii |

### `when`

```toml
when = { command = "codegraph", file = ".codegraph" }        # both must hold
when = { repo = "github.com/acme/(shop|billing)" }           # regex on the repository id
when = { env = "NOTES_TOKEN" }                                # variable must be set
when = { exists = "{{env.HOME}}/.config/notes/{{repo.slug}}.json" }   # templated; `*` allowed in the last segment
```

Adaptér, ktorého podmienky neplatia, sa vypíše ako neaktívny aj s dôvodom a nikdy sa nevolá.

## Operácie

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

### Naväzovanie argumentov MCP

Keď MCP operácia nemá `args`, kontext prečíta vstupnú schému nástroja a vyplní parametre podľa názvu:

| Rola | Skúšané názvy parametrov (v poradí) | Hodnota |
| --- | --- | --- |
| query (search, code, brief) | `query`, `q`, `search`, `search_query`, `text`, `pattern`, `substring_pattern`, `keyword(s)`, `term`, `question`, `topic`, `name_path_pattern`, `name_path`, `symbol`, `symbol_name`, `name`, `input`, `prompt` | dopyt |
| target (history) | `target`, `path`, `file`, `file_path`, `filepath`, `relative_path`, `commit`, `query`, `q`, `text` | cesta, `path:line`, SHA alebo téma |
| uri (read) | `uri`, `url`, `path`, `id`, `resource`, `name` | URI |
| content (store) | `content`, `text`, `memory`, `body`, `markdown`, `message`, `data`, `note` | záznam ako Markdown |
| title, tags (store) | `title`, `subject`, `name`, `key` · `tags`, `labels`, `categories` | |
| limit | `limit`, `max_results`, `top_k`, `k`, `n`, `count`, `max`, `num_results`, `size`, `maxFiles` | |
| project | `projectPath`, `project_path`, `project`, `repo`, `repository`, `cwd`, `root`, `workspace`, `directory` | koreň repozitára |
| prompt (llm) | `prompt`, `input`, `message`, `text`, `query` | prompt |

Povinný reťazcový parameter, ktorému nič nezodpovedalo, dostane hlavnú hodnotu (dopyt, cieľ, URI, obsah alebo prompt). Hodnoty sa prevedú na JSON typ parametra. `kontext adapters inspect <name>` vypíše nástroje a ich schémy.

### Mapovanie výsledkov

1. **Payload.** Z výsledkov MCP sa stane ich `structuredContent`, alebo textový obsah naparsovaný ako JSON, ak to ide, alebo obyčajný text. Výstup HTTP a príkazov sa parsuje podľa `format` (`auto` znesie riadky logu pred JSON).
2. **Položky.** `items` vyberie zoznam malým jazykom ciest ([referencia](../reference/04-templates.md#paths)); `split` rozreže textovú odpoveď na riadkoch, ktoré začínajú prefixom (z každého kusa je `{title, text, raw}`); inak kontext vezme pole na najvyššej úrovni alebo prvé pole pod `results`, `items`, `hits`, `data`, `matches`, `entries`, `memories` či `documents`, alebo celý payload považuje za jednu položku. `where` potom ponechá len položky, ktorých polia sa rovnajú zadaným hodnotám.
3. **Polia.** `map` vyberie každé pole výsledku z položky: cesta, alternatívy `a|b`, `re:<regex>` (prvá zachytávacia skupina, aplikovaná na text položky), `tpl:<template>` (vykreslená voči položke, napr. `tpl:notes://{{id}}`) alebo `=literal`. Nenamapované polia sa vrátia k bežným názvom (`title`/`name`/…, `snippet`/`abstract`/`summary`/`content`/…, `uri`/`url`/`path`/…, `score`/`relevance`/…).

## Udalosti

```toml
[[adapters.<name>.on]]
event = "sync"                      # capture | promote | sync
op = "store"                        # any op of this adapter
visibility = "private"              # optional filter (capture events)
kinds = ["decision", "incident"]    # optional filter
```

Payload udalosti je dostupný v šablónach operácie: `id`, `kind`, `title`, `status`, `date`, `summary`, `tags`, `paths`, `body`, `markdown` (celý súbor), `path`, `visibility`, `source.repo`, `source.branch`.

## Nástroje {#tools}

Deklaruj nový MCP nástroj, ktorý sa opiera o jedno z volaní adaptéra:

```toml
[[adapters.notes.tools]]
name = "notes_recent"
description = "The ten most recent team notes"
params = { tag = "string", limit = "integer" }       # `!` marks required: { q = "string!" }
op = { method = "GET", path = "/recent", query = { tag = "{{tag}}", n = "{{limit}}" }, text = "items" }
# or op = "search" to reuse an existing op
```

Argumenty sú dostupné ako `{{args.x}}` aj priamo ako `{{x}}`. Výsledok sa agentovi vráti ako text. `params` môže byť aj úplná JSON schéma.

Ďalšie publikovanie vlastných nástrojov MCP servera:

```toml
[adapters.codegraph]
driver = "mcp"
command = ["codegraph", "serve", "--mcp"]
expose = ["codegraph_explore", "codegraph_node"]   # or "all"
```

## Šablóny

Všetky reťazcové polia sú šablóny: `{{repo.id}}`, `{{repo.slug}}`, `{{repo.name}}`, `{{repo.dir}}`, `{{repo.root}}`, `{{repo.branch}}`, `{{project.name}}`, `{{vars.x}}`, `{{env.X}}`, `{{now.date}}` a vstupy operácie. Filtre ako `{{query|urlencode}}` a `{{env.URL|default:http://localhost:1933}}` opisuje stránka [Šablóny a mapovanie výsledkov](../reference/04-templates.md).
