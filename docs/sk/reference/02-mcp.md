# MCP nástroje, prompty a zdroje

Server: `kontext mcp`, JSON-RPC 2.0 cez stdio, správy oddelené novým riadkom. Verzie protokolu 2024-11-05, 2025-03-26, 2025-06-18 a 2025-11-25 (verzia klienta sa zopakuje, ak je podporovaná). Schopnosti: `tools`, `prompts`, `resources`, `logging`. Výsledky nástrojov sú text; chyby sa vracajú ako výsledky s `isError: true`.

## Nástroje

Každý nástroj nesie MCP anotácie, podľa ktorých klienti rozhodujú, čo treba schváliť: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log` a `ctx_threads` majú `readOnlyHint: true`; `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init` a `ctx_init_submit` zapisujú len do lokálneho inboxu, pracovného stromu a git indexu a majú `destructiveHint: false`. Všetky majú `openWorldHint: false`.

### Iný repozitár: `dir` {#dir}

`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_inbox` a `ctx_prepare_commit` prijímajú `dir`: cestu vnútri iného worktree, submodulu alebo susedného repozitára (absolútnu, alebo relatívnu ku koreňu tohto repozitára). Nástroj beží tam a jeho odpoveď začína repozitárom, z ktorého prišla.

Aj bez `dir` kontext ide za cestami, ktoré dostane:

- Na cesty vo `focus`, cieľ `ctx_why` a `paths` v `ctx_capture` vnútri submodulu odpovedá submodul a zachytenie ide do jeho inboxu, ak má tímové znalosti. Cesta, ktorá existuje len vnútri jedného submodulu, relatívne k nemu (`apps/runner` pre `extractor/apps/runner`, tak ako cesty pomenúva vlastné `AGENTS.md` submodulu), sa počíta za cestu submodulu. Na absolútne cesty do iného repozitára alebo worktree odpovedá ten.
- Submodul s vlastnými tímovými znalosťami dostane v briefe vlastnú sekciu a jeho výsledky vyhľadávania nesú jeho cestu: `kx:extractor/<id>`, `git:extractor/<sha>`, `file:extractor/<path>`. `ctx_read` ich otvorí tak, ako sú.

### `ctx_brief`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `focus` | string[] | | cesty alebo slová k téme, ktoré sa uprednostnia |
| `budget_tokens` | integer | `brief.budget_tokens` (1400) | približná veľkosť |
| `adapters` | boolean | true | zahrnie sekcie `brief` z adaptérov |

Odkazy v hranatých zátvorkách sú najkratšia jednoznačná predpona id záznamu (`[2026-09-28-adapters]`), ktorú `ctx_read` rozpozná. Okrem tímových znalostí brief uvádza *Local notes* (inbox tohto klonu) a v repozitári bez úložiska aj *Related docs and history* k focusu. Pozri [Vrstvy kontextu](../concepts/04-context-layers.md).

### `ctx_search`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `query` | string | povinné | slová, identifikátory, cesty |
| `kinds` | string[] | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` |
| `sources` | string[] | všetky | `local` a/alebo názvy adaptérov |
| `limit` | integer | 10 | najviac výsledkov (do 50) |
| `budget_tokens` | integer | 1200 | približná veľkosť odpovede |

Každý výsledok: `N. title — snippet [kind · status · date · source] <uri>`.

Lokálny zdroj zahŕňa aj poznámky v inboxe tohto klonu (`inbox:<id>`, označené *local note*) a submoduly s vlastnými tímovými znalosťami (označené ich cestou).

### `ctx_read`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `uri` | string | povinné | `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, URI adaptéra alebo obyčajná cesta |
| `level` | 0 \| 1 \| 2 | 1 | jeden riadok, prehľad, celé |

### `ctx_why`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `target` | string | povinné | cesta, `path:line`, `path:start-end`, SHA commitu alebo symbol |
| `limit` | integer | 12 | položiek na sekciu |

### `ctx_log`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `kind` | string | `decision` | druh, ktorý sa vypíše |
| `all` | boolean | false | zahrnie nahradené / zamietnuté |
| `limit` | integer | 40 | riadky |

### `ctx_capture`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `kind` | string | povinné | `decision`, `convention`, `learning`, `incident` (akceptujú sa aliasy) |
| `title` | string | povinné | krátke tvrdenie |
| `body` | string | povinné | Markdown: kontext, rozhodnutie alebo poznatok, dôsledky |
| `summary` | string | | jeden riadok |
| `paths` | string[] | | cesty alebo globy, ktorých sa týka |
| `tags` | string[] | | |
| `visibility` | `team` \| `private` | `capture.default_visibility` | |
| `supersedes` | string[] | | ID záznamov, ktoré nahrádza |
| `status` | string | `accepted` pri rozhodnutiach | |
| `promote` | boolean | false | zapíše ho hneď do úložiska namiesto inboxu |
| `commits` | string[] | | krátke sha commitov, z ktorých pochádza |
| `source` | string | | kde sa našiel, napr. označenie vlákna z `ctx_threads` (zostáva len v inboxe) |

### `ctx_threads`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `thread` | string | | `claude:<id>`, `codex:<id>`, `superset:<workspace>` (všetky jeho vlákna), súbor s prepisom alebo `last`; bez neho vypíše posledné vlákna |
| `part` | integer | 1 | časť dlhého prepisu |
| `budget_chars` | integer | 20000 | znakov na časť (najviac 80000) |

Kompaktné, zamaskované prepisy vlákien agentov tohto repozitára na tomto počítači. Notifikácia prostredia (`<task-notification>` v Claude Code o dokončenej úlohe na pozadí) sa zmení na jeden riadok a cesty do dočasného adresára relácie agenta sa skrátia na `<tmp>/`. Časť sa zmestí do jedného výsledku nástroja, dlhšie klienti orezávajú. Pozri [Znalosti z vlákien agentov](../guides/08-distill-threads.md).

### `ctx_inbox`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `action` | `list` \| `show` \| `promote` \| `drop` | `list` | |
| `ids` | string[] | | pre show / promote / drop |

### `ctx_prepare_commit`

| Parameter | Typ | |
| --- | --- | --- |
| `promote` | string[] | ID z inboxu, ktoré sa zapíšu do úložiska a stagnú |
| `drop` | string[] | ID z inboxu, ktoré sa zahodia |

Vráti report opísaný v [Zachytávanie a review](../concepts/03-capture-and-review.md#before-committing-ctx-prepare-commit). Uvedie trailery commitu a či ich pridá hook prepare-commit-msg: v klone bez hookov kontextu (čerstvý klon, projekt v Superset) ich treba dopísať ručne, alebo spustiť `kontext hooks install`.

### `ctx_init`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `count` | integer | 1 | koľko úloh vrátiť (do 5) |
| `bootstrap` | boolean | false | založí úložisko znalostí, keď ho repozitár nemá (len keď o to požiadal používateľ) |

Keď repozitár ešte nemá `.ai/` a `bootstrap` je true, `ctx_init` najprv spustí scan, history a render (bez hookov) a potom vráti prvú úlohu; bez neho vysvetlí, ako kontext zaviesť. Vetvu nikdy nezavádza, keď tímové znalosti už má iná vetva.

V repozitári bez úložiska znalostí prijme `ctx_capture` len `visibility: private` a `ctx_inbox` / `ctx_prepare_commit` nepovyšujú.

Zachytenie, ktorého cesty všetky ležia v submodule alebo inom repozitári s tímovými znalosťami, ide do inboxu toho repozitára a cesty sa prepíšu relatívne k nemu. Inak zostane tu a poznámka povie prečo (zmiešané cesty, submodul bez úložiska, cesty, ktoré neexistujú nikde).

### `ctx_init_submit`

| Parameter | Typ | |
| --- | --- | --- |
| `task_id` | string (povinné) | `purpose`, `mod:<module>`, `dec:<area>`, `refresh:<module>` |
| `summary` | string | jeden riadok, najviac ~200 znakov |
| `overview` | string | Markdown, 5–15 riadkov |
| `decisions` | object[] | `{title, decision, summary?, context?, consequences?, paths?, commits?, date?, status?, tags?}` (pre úlohy `dec:`) |
| `learnings` | object[] | `{title, body, paths?, tags?}` |
| `skip` | boolean | preskočí úlohu |
| `reason` | string | prečo sa preskakuje |

`paths`, `commits` a `tags` prijímajú aj reťazce oddelené čiarkou.

### Nástroje adaptérov

Nástroje MCP adaptérov s `expose` sa vypisujú s ich pôvodnými schémami (názvy s prefixom `<adapter>_`, ak ním už nezačínajú); nástroje deklarované v konfigurácii adaptéra sa vypisujú so schémou z `params`. Ich popisy začínajú `[adapter]`.

## Prompty

| Názov | Argumenty | Účel |
| --- | --- | --- |
| `kontext-init` | `tasks` (voliteľné) | zavedie a prehĺbi znalosti repozitára |
| `kontext-commit` | | pripraví aktuálnu zmenu na commit |
| `kontext-reflect` | | zachytí zo session najviac tri trvalé veci |
| `kontext-distill` | `thread` (voliteľný) | vytiahne trvalé znalosti z iného vlákna alebo workspace v Supersete |

## Zdroje

| URI | Obsah |
| --- | --- |
| `kontext://brief` | brief (bez sekcií adaptérov) |
| `kontext://entry/<id>` | Markdown záznamu (šablóna `kontext://entry/{id}`) |

## Požiadavky iniciované serverom

MCP *klient* vnútri kontextu (používajú ho adaptéry `mcp`) odpovedá serverom, ktoré spúšťa, na `ping` a `roots/list` (s koreňom repozitára); ostatné požiadavky serverov odmietne.
