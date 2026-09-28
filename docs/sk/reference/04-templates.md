# Šablóny a mapovanie výsledkov

## Šablóny

Každý reťazec v definícii adaptéra je šablóna. Výrazy sa píšu ako `{{path | filter | filter:arg}}`.

### Kontext

| Premenná | Príklad |
| --- | --- |
| `repo.id` | `github.com/acme/shop` |
| `repo.slug` | `github.com-acme-shop` |
| `repo.name` | `shop` (z remote) |
| `repo.dir` | názov adresára hlavného worktree |
| `repo.root` | absolútna cesta aktuálneho worktree |
| `repo.remote`, `repo.branch`, `repo.worktree` | |
| `project.name` | `[project] name`, inak názov z koreňového manifestu, inak `repo.name` |
| `vars.<name>` | globálne `[vars]` zlúčené s `vars` adaptéra |
| `env.<NAME>` | premenná prostredia |
| `now.date`, `now.iso` | `2026-09-28`, časová pečiatka RFC 3339 |
| `adapter.name` | |

Vstupy operácií:

| Op | Premenné |
| --- | --- |
| `search`, `code` | `query`, `limit` |
| `history` | `target`, `query` (rovnaká hodnota), `limit` |
| `read` | `uri`, `level` |
| `brief` | `query`, `focus` |
| `llm` | `prompt` |
| doručenia udalostí (`store`, …) | `id`, `kind`, `title`, `status`, `date`, `summary`, `tags`, `paths`, `body`, `markdown`, `path`, `visibility`, `source.repo`, `source.branch` |
| deklarované nástroje | `args.<name>` a každý argument podľa názvu |
| operácie s príkazmi s `output_file = true` | `output_file` |

### Filtre

| Filter | Výsledok |
| --- | --- |
| `urlencode` | percentuálne zakódované |
| `json` | JSON text hodnoty |
| `lower`, `upper`, `trim`, `slug` | |
| `first_line` | |
| `join:<sep>` | spojí zoznam (`join:, ` · `join: `) |
| `truncate:<n>` | najviac n znakov |
| `after:<sep>`, `before:<sep>` | časť za / pred prvým oddeľovačom (celá hodnota, keď chýba) |
| `default:<literal>` | použije sa, keď hodnota chýba alebo je prázdna (`{{env.URL|default:http://localhost:1933}}`) |
| `int` | reťazec naparsovaný ako celé číslo |

Ako hodnoty fungujú aj literály v úvodzovkách: `{{'text'|upper}}`.

### Typy

- JSON hodnota, ktorá je **presne jeden** výraz, si zachová typ: `limit = "{{limit}}"` pošle číslo, `tags = "{{tags}}"` pošle pole.
- Všetko ostatné sa vykreslí ako reťazec.
- Členy objektov, ktoré sa vykreslia na `null`, sa vynechajú, takže voliteľné argumenty zmiznú namiesto toho, aby sa poslali prázdne.

## Cesty {#paths}

`items`, `text`, kľúče `where` a hodnoty `map` používajú malý jazyk ciest nad JSON:

| Syntax | Význam |
| --- | --- |
| `a.b.c` | kľúče objektov |
| `a[0]` | index v poli |
| `a[*]` | každý prvok poľa |
| `*` | každá hodnota objektu (alebo prvok poľa) |
| `$` alebo prázdne | koreň |
| `a|b` | prvá alternatíva, ktorá existuje (v `map`) |

Príklady: `result.memories[*]`, `result.*[*]` (každý zoznam pod `result`), `data[*].attributes`, `results[0].title`.

## Mapovanie výsledkov

Pri operáciách `search`, `history` a `code` sa z každej položky stane výsledok s `title`, `snippet`, `uri`, `score`, `kind` a `date`.

| Hodnota `map` | Význam |
| --- | --- |
| `name` / `a.b` / `a|b` | cesta do položky |
| `re:<regex>` | prvá zachytávacia skupina (alebo celá zhoda) regexu na texte položky |
| `tpl:<template>` | šablóna vykreslená voči položke (`tpl:notes://{{id}}`) |
| `=<literal>` | konštanta |

Náhradné kľúče, keď pole nie je namapované:

| Pole | Skúšané kľúče |
| --- | --- |
| `title` | `title`, `name`, `subject`, `heading`, `uri`, `path`, `id` |
| `snippet` | `snippet`, `abstract`, `summary`, `content`, `text`, `description`, `body`, `overview` |
| `uri` | `uri`, `url`, `path`, `file`, `id` |
| `score` | `score`, `relevance`, `similarity`, `rank` (inak podľa poradia) |
| `kind` | `kind`, `type`, `context_type`, `category` |
| `date` | `date`, `created_at`, `updated_at`, `time`, `timestamp` |

Položky bez titulku aj úryvku sa zahodia. URI, ktoré sa majú dať čítať cez `ctx_read`, potrebujú schému (`notes://…`) a operáciu `read`, ktorá daný prefix `owns`.

## Formáty payloadu

| `format` | Parsovanie |
| --- | --- |
| `auto` (predvolene) | JSON, keď sa dá naparsovať (riadky logu pred JSON sa preskočia), inak text |
| `json` | JSON alebo null |
| `jsonl` | jedna JSON hodnota na riadok → zoznam |
| `lines` | neprázdne riadky → zoznam reťazcov |
| `text` | surový text |

Výsledky MCP nástrojov sa najprv rozbalia: `structuredContent`, ak existuje, inak textový obsah (naparsovaný ako JSON, keď to ide).
