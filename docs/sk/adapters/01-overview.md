# Adaptéry

Adaptér pripája kontext k vonkajšiemu systému: úložisku pamäte, archívu sessions, serveru s inteligenciou kódu, LLM či chatovému webhooku. Adaptéry sú **len konfigurácia**: kód kontextu nikdy nemenuje žiadny produkt.

## Tri myšlienky

**Schopnosti (operácie).** O čo môže kontext adaptér požiadať:

| Op | Používa ju | Vracia |
| --- | --- | --- |
| `search` | `ctx_search` (federovane) | výsledky |
| `read` | `ctx_read` pre URI, ktoré adaptér `owns` | text |
| `history` | `ctx_why` (cesta, commit, téma) | výsledky |
| `code` | `ctx_why <symbol>` | výsledky (umiestnenia) |
| `brief` | sekcia `ctx_brief` | text |
| `store` | doručovanie udalostí | potvrdenie |
| `llm` | `kontext init --deepen` | text |
| `health` | `kontext adapters test` | text |

Môžeš definovať aj ďalšie názvy operácií (napríklad druhú operáciu store pre súkromné poznámky) a nasmerovať na ne udalosti.

**Udalosti.** Čo sa v kontexte deje a o čom môže chcieť adaptér vedieť:

| Udalosť | Kedy | Payload |
| --- | --- | --- |
| `capture` | kandidát vstúpi do inboxu | záznam a jeho `visibility` (`team` / `private`) |
| `promote` | kandidát sa zapíše do úložiska | záznam |
| `sync` | znalosti v `HEAD` sa zmenili (po commite, merge, rebase) | záznam tak, ako bol commitnutý |

Odbery mapujú udalosť na operáciu, voliteľne s filtrom podľa viditeľnosti a druhov.

**Drivery.** Ako sa operácia vykoná:

| Driver | Komunikuje s | Polia operácie |
| --- | --- | --- |
| `mcp` | MCP serverom cez stdio (`command`) alebo streamable HTTP (`url`) | `tool`, `args` (voliteľné: naviažu sa zo schémy nástroja) |
| `http` | HTTP API | `method`, `path`/`url`, `query`, `headers`, `body` |
| `command` | lokálnym programom | `command`, `stdin`, `cwd`, `env`, `output_file` |

## Úplný príklad

```toml
[adapters.notes]
driver = "http"
base_url = "http://localhost:8080"
headers = { Authorization = "Bearer {{env.NOTES_TOKEN}}" }

[adapters.notes.ops.search]
method = "GET"
path = "/search"
query = { q = "{{query}}", n = "{{limit}}" }
items = "results[*]"
map = { title = "name", snippet = "excerpt", uri = "url" }

[adapters.notes.ops.store]
method = "PUT"
path = "/notes/{{repo.slug}}/{{id}}"
body = { title = "{{title}}", markdown = "{{markdown}}", tags = "{{tags}}" }

[[adapters.notes.on]]
event = "sync"
op = "store"
kinds = ["decision", "convention"]
```

`ctx_search` teraz zahŕňa aj výsledky zo služby s poznámkami a každé rozhodnutie či konvencia, ktorá sa dostane do `HEAD`, sa do nej zrkadlí.

## Životný cyklus a zlyhania

- Adaptéry sa vytvárajú lenivo pri prvom použití. MCP servery sa spustia raz na proces `kontext mcp` a znova sa používajú; koreň repozitára sa dozvedia cez `roots/list`.
- Podmienky `when` (príkaz v `PATH`, súbor v repozitári, regex na ID repozitára, premenná prostredia, šablónovaná cesta, ktorá musí existovať) adaptér potichu vypnú tam, kde sa nehodí.
- Paralelné volania bežia s časovými limitmi (12 s pre search/code/history, 8 s pre brief).
- Po chybe si adaptér dá 30 sekúnd pauzu namiesto toho, aby spomaľoval každé volanie.
- Doručenia udalostí sa z outboxu skúšajú znova.

## Federovanie MCP nástrojov

MCP adaptér môže nástroje svojho servera publikovať ďalej cez kontext (`expose = "all"` alebo zoznam názvov nástrojov), takže agent potrebuje len jeden MCP server. Nástroje sa dajú aj **deklarovať** v konfigurácii a oprieť o ľubovoľný driver: nový MCP nástroj bez kódu. Pozri [Konfigurácia adaptérov](./02-configuration.md#tools).

## Ďalej

- [Konfigurácia adaptérov](./02-configuration.md): každé pole
- [Presety](./03-presets.md): pripravené adaptéry
- [Vlastný adaptér](./04-writing-an-adapter.md): návod krok za krokom
