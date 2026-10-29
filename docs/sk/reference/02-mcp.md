# MCP nástroje, prompty a zdroje

Server: `kontext mcp`, JSON-RPC 2.0 cez stdio, správy oddelené novým riadkom. Schopnosti: `tools`, `prompts`, `resources` (od 0.10.0 bez `logging`). Zlyhania nástrojov sa vracajú ako výsledky s `isError: true`; neznámy nástroj, chýbajúce meno nástroja alebo zlý cursor `resources/list` je chyba protokolu `-32602`, panic vnútri jednej požiadavky `-32603`.

## Protokol {#protocol}

Server odpovedá v oboch érach MCP:

- **Bezstavová (2026-07-28).** `server/discover` vráti schopnosti, inštrukcie a informácie o serveri; každá požiadavka môže namiesto handshaku `initialize` niesť `_meta["io.modelcontextprotocol/protocolVersion"]`. Výsledky nesú `resultType: "complete"` a `_meta["io.modelcontextprotocol/serverInfo"]`; výsledky zoznamov a čítania nesú `ttlMs` a `cacheScope: "private"`. Neznáma verzia je `-32022` s `data: {supported, requested}`.
- **Starý handshake.** `initialize` s 2024-11-05, 2025-03-26, 2025-06-18 alebo 2025-11-25 zopakuje verziu klienta; neznáma padne späť na 2025-11-25.

Zoznamy sa vracajú v deterministickom poradí. `resources/list` stránkuje cez cursor. `tools.listChanged` môže byť true: je true, len keď adaptéry vystavujú nástroje cez `expose`. `notifications/cancelled` zastaví požiadavku a zabije bežiaci verify príkaz.

**Inštrukcie.** Prvých 512 znakov inštrukcií servera stojí samostatne (čo kontext drží, `ctx_brief` ako prvé, kedy hľadať); celok ostáva pod 4 096 znakmi. `ctx_brief` nesie `_meta["anthropic/alwaysLoad"]: true`, takže ho načítajú aj hostitelia, ktorí odkladajú nástroje za tool search; `ctx_read` a `ctx_api` nesú `maxResultSizeChars`.

## Nástroje

[`ctx_api`](./05-application-api.md) sprístupňuje verzovanú aplikačnú službu cez `operation`, `input`
a voliteľné `dir`. Vracia `structuredContent` a `outputSchema` aj úplnú textovú JSON kópiu.

Nástrojov je šestnásť: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads`, `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_verify`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init`, `ctx_init_submit` a `ctx_api`.

**Štruktúrované výsledky.** `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox` a `ctx_drift` deklarujú `outputSchema` a vedľa textu, ktorý ostáva rovnaký, vracajú `structuredContent`. Výsledky a riadiace záznamy prídu aj ako bloky `resource_link` na `kontext://entry/{id}`. Ostatné nástroje vracajú text.

**Aliasy parametrov.** Parametre dostávajú aliasy, nikdy sa nepremenúvajú: `id` a `uri` (aj `path`) pri `ctx_read`; `id` a `ids` pri `ctx_inbox`, `ctx_challenge` a `ctx_verify`; `focus` a `paths` pri `ctx_brief`; `kind` a `kinds` pri `ctx_search`, `ctx_drift` a `ctx_log`, kde `kinds` môže byť aj reťazec oddelený čiarkami; `limit` a `max`; `budget_tokens` všade (pri `ctx_threads` nastaví `budget_chars` na štyri znaky na token); a `reason`, `why` a `because`, ktoré znamenajú to isté pri `ctx_challenge`, `ctx_confirm`, `ctx_verify` a `ctx_init_submit`.

Každý nástroj nesie MCP anotácie, podľa ktorých klienti rozhodujú, čo treba schváliť: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift` a `ctx_threads` majú `readOnlyHint: true`; `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init` a `ctx_init_submit` zapisujú len do lokálneho inboxu, pracovného stromu a git indexu a majú `destructiveHint: false`. Tieto nástroje si zachovávajú `openWorldHint: false`. `ctx_verify` spúšťa pinnutý shellový príkaz a má `destructiveHint: true`, `openWorldHint: true`. Zmiešané `ctx_api` má konzervatívne `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false`, `openWorldHint: true`: nakonfigurované LLM a event adaptéry môžu pristupovať k externým službám.

### Iný repozitár: `dir` {#dir}

`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_challenge`, `ctx_confirm`, `ctx_verify`, `ctx_drift`, `ctx_inbox` a `ctx_prepare_commit` prijímajú `dir`: cestu vnútri iného worktree, submodulu alebo repozitára, absolútnu alebo relatívnu k tomuto koreňu. Pri `ctx_verify`, `ctx_inbox` a `ctx_prepare_commit`, ktoré spúšťajú príkazy alebo menia lokálny stav, musí `dir` ležať v tomto repozitári, v jeho worktree alebo submodule; čokoľvek iné sa odmietne. Čítacie nástroje môžu siahnuť aj do susedného repozitára. Bez neho rozhodujú cesty, ktoré nástroj dostane, ktorý repozitár odpovie. Pozri [Viac repozitárov](../concepts/09-several-repositories.md).

### `ctx_brief`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `focus` | string[] | | cesty alebo slová k téme, ktoré sa uprednostnia |
| `budget_tokens` | integer | `brief.budget_tokens` (1400) | približná veľkosť |
| `adapters` | boolean | true | zahrnie sekcie `brief` z adaptérov |

S focusom ide prvá sekcia *Governs &lt;focus&gt; (N)*, ktorá nikdy ticho nevypadne: jeden riadok na riadiaci záznam (plné id, názov, značky, commity odvtedy), zoradené podľa špecifickosti cesty a potom od najstaršieho; čo sa nezmestí, vymenuje riadok `also governing:`. Ostatné sekcie pri orezaní končia `… N more (K govern your focus)`. Položka focusu s `/`, glob znakom alebo známou príponou je cesta, aj kým súbor neexistuje. Odkazy v hranatých zátvorkách sú plné id (`[2026-09-28-adapters-are-configuration]`); do `supersedes` a `[[odkazov]]` ich kopíruj tak, ako sú. Riadok má značku, keď záznam nie je prijatá a schválená znalosť: stav, keď nie je `accepted` (`(proposed)`), `local, unapproved`, keď je jeho súbor v pracovnom strome nový, zmenený alebo stagnutý, `review due`, `challenged locally` a `N sources`, keď má evidenciu, napr. `Move search to OpenSearch (proposed; 2 sources)`. Prijatý a commitnutý záznam značku nemá. `## Changed since <rev>` uvádza, čo sa zmenilo od posledného briefu v tomto worktree (najviac päť riadkov; bez neho *Changed in the last 14 days*). Brief uvádza aj *Local notes* (inbox tohto klonu) a bez úložiska aj *Related docs and history*. Pozri [Vrstvy kontextu](../concepts/04-context-layers.md).

### `ctx_search`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `query` | string | povinné | slová, identifikátory, cesty |
| `kinds` | string[] alebo string | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit`; `kind` je alias |
| `sources` | string[] | všetky | `local` a/alebo názvy adaptérov |
| `limit` | integer | 10 | najviac výsledkov (do 50) |
| `adapters` | boolean | `[search] adapters` | `true` sa pýta vyhľadávacích adaptérov, aj keď lokálne zásahy stačia, `false` nikdy |
| `budget_tokens` | integer | 1200 | približná veľkosť odpovede |

Každý výsledok: `N. title — snippet [kind · status · date · source] <uri>`. Stav `accepted` sa vynecháva; značky ako `local, unapproved` a `review due` nasledujú za stavom. Zásah, ktorý iba začína ako ten nad ním (rovnaké prvé slová, iné tvrdenie), nezmizne: nasleduje ako `↳ similar, differs: … <uri>` s časťou, ktorou sa líši. Vypadnú iba preukázané kópie: rovnaké URI, kópia lokálneho záznamu z adaptéra a zásah, ktorého názov a text majú rovnaký digest ako predošlý; nahradené dvojča nikdy neostane nad aktívnym nástupcom. Lokálne zdroje sa pred spojením vážia (záznamy 1.0, kúsky dokumentácie 0.7, commity 0.6; vstavané, pozri [`[search]`](./03-configuration.md#search)); nahradený alebo zastaraný zásah dostane nižšiu váhu a jeho stav znie `superseded · → superseded by [<id>] <Title>`, nikdy sa neskryje.

Lokálny zdroj zahŕňa aj poznámky v inboxe tohto klonu (`inbox:<id>`, označené *local note*) a submoduly s vlastnými tímovými znalosťami (označené ich cestou).

### `ctx_read`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `uri` | string | povinné | `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, URI adaptéra alebo obyčajná cesta |
| `level` | 0 \| 1 \| 2 | 1 | jeden riadok, prehľad, celé |
| `offset`, `part` | integer | | stránkovanie dlhého textu: text dlhší ako 32 000 znakov príde po častiach a odpoveď končí tým, ako čítať ďalej (`part: 2` alebo `offset: N`) |

`id` je alias pre `uri`. Obyčajný argument uprednostní existujúcu cestu a odpoveď povie, na čo sa vyriešil. Úrovne 0 a 1 ukážu `superseded_by` s odkazom a značky (local, challenged, review due). `file:` odmietne `.git/`, `.git-credentials` a `.ssh`.

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
| `body` | string | povinné | Markdown: kontext, rozhodnutie alebo poznatok, dôsledky; pri rozhodnutí voliteľne `## Alternatives` |
| `summary` | string | | jeden riadok |
| `paths` | string[] | | cesty alebo globy, ktorých sa týka |
| `tags` | string[] | | |
| `visibility` | `team` \| `private` | `capture.default_visibility` | |
| `supersedes` | string[] | | plné id záznamov, ktoré nahrádza; nejednoznačné id sa odmietne |
| `extends` | string[] | | plné id záznamov, na ktorých stavia; zachová sa cez inbox aj povýšenie |
| `status` | string | `accepted` pri rozhodnutiach | |
| `promote` | boolean | false | zapíše ho hneď do úložiska namiesto inboxu |
| `commits` | string[] | | krátke sha commitov, z ktorých pochádza |
| `source` | string | | kde sa našiel, napr. označenie vlákna z `ctx_threads` (zostáva len v inboxe) |
| `review_by` | string | `review_after` druhu | `YYYY-MM-DD`: kedy ho znova pozrieť (pravidlo, ktoré platí počas migrácie) |
| `evidence` | object[] | | `{ref, quote?}`: `pr:<id>`, `issue:<id>`, `task:<id>`, `code:<repo>@<rev>:<cesta>#L<a>-<b>`, `url:<url>`; pozri [Evidencia](./05-application-api.md#maintenance) |
| `verify` | string | | príkaz, ktorý ho dokázal (poučenie alebo rozhodnutie overené testom nesie ten test): jeden riadok, najviac 500 znakov, spúšťa ho `ctx_verify` |

Odpoveď uvedie súvisiace záznamy ako poznámky (`related, check: <názov> <kx:id> — <dôvod>`), so štítkom od modelu, napr. `possible conflict, check`, keď sa ho [`entries.related`](./05-application-api.md#related) na tento text už pýtalo. Či záznam niečo nahrádza, rozhodneš ty; poznámka to nikdy netvrdí.

### `ctx_challenge`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `id` | string | povinné | záznam: `kx:<id>`, id alebo jeho jednoznačná predpona |
| `why` | string | povinné | prečo už neplatí (aliasy `reason`, `because`) |
| `visibility` | `team` \| `private` | `team` | |

Zaznamená, že záznam už neplatí, ako lokálneho kandidáta v inboxe druhu `challenge` (voči záznamu a commitu, v ktorom bol prečítaný). Kým neprejde review, nič sa nemení: brief označí záznam `challenged locally` a `ctx_prepare_commit` ponúkne jeho vyradenie alebo zaznamenanie náhrady, ktorá ho nahrádza. Námietka ostáva v stave *resolving*, kým nový blob záznamu nepristane v `HEAD`.

### `ctx_confirm`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `id` | string | | záznam: `kx:<id>`, id alebo jeho jednoznačná predpona |
| `ids` | string[] | | viac záznamov naraz; viac ako päť sa zapíše ako `bulk` |
| `because` | string | | jeden riadok: voči čomu bol záznam overený |

Zaznamená, že záznam stále platí aj po zmene kódu, ktorý riadi: nastaví jeho čas [`checked`](../concepts/02-knowledge-store.md#checked) na teraz (UTC, na sekundu), nič iné v súbore nezmení, pridá do [denníka overení](../concepts/03-capture-and-review.md#verifications) riadok `agent` s `HEAD` a dôvodom a oboje stagne, takže potvrdenie prejde review so zmenou. Aktuálnosť aj drift počítajú od neho. Agent potvrdí len to, čo overil voči kódu; záznam, ktorý už neplatí, napadne alebo nahradí. Potvrdenie agentom sa zobrazuje ako `stamped by agent`, nie ako overenie, a MCP nikdy nezapíše riadok `read`; kde záznam uvádza pinnutý príkaz, `ctx_verify` dokáže viac.

### `ctx_verify`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `id` | string | povinné | záznam: `kx:<id>`, id alebo jeho jednoznačná predpona |
| `because` | string | | jeden riadok: prečo bol spustený |

Spustí príkaz [`verify`](../concepts/03-capture-and-review.md#verify) záznamu z koreňa repozitára, ale len keď je text príkazu pinnutý v tvojom `trust.toml` (`kontext trust --verify <id>`, z terminálu) a súbor záznamu ani jeho riadené cesty nemajú necommitnuté zmeny. Vtedy beží s povoleným zoznamom premenných prostredia a časovým limitom, do denníka pridá riadok `verify` s výsledkom a `HEAD`, stagne ho a odpovie výsledkom a zamaskovaným koncom výstupu. Inak nespustí nič: odpoveď začína `Not run <id> (untrusted_verify)` alebo `(dirty_tree)`, ukáže príkaz a poznámku, a agent môže príkaz spustiť vo vlastnom sandboxe a to, čo zistil, zapísať cez `ctx_confirm` (pečiatka agenta, nikdy riadok `verify`). Záznam bez príkazu alebo neaktívny záznam odmietne. `destructiveHint: true`, `openWorldHint: true`; pozri [Bezpečnosť](../concepts/08-security.md#verify).

### `ctx_drift`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `base` | string | merge base `HEAD` s predvolenou vetvou remote, inak `HEAD` | začiatok rozsahu, git revízia |
| `head` | string | pracovný strom | koniec rozsahu, git revízia; bez neho sa počítajú aj necommitnuté a nesledované zmeny |
| `kinds` | string[] | decision, convention, learning, incident | druhy na kontrolu |

Znalosti, ktoré zmena nechala za sebou: aktívne záznamy, ktorých `paths` pokrývajú súbory zmenené v rozsahu a ktorých vlastný súbor sa v ňom neupravil ani nepotvrdil, každý so zmenenými súbormi, commitmi a s tým, čo s ním robiť. Deterministické, bez modelu. Pozri [Drift](../concepts/03-capture-and-review.md#drift); `entries.drift` v `ctx_api` vráti to isté ako JSON.

### `ctx_threads`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `thread` | string | | `claude:<id>`, `codex:<id>`, `superset:<workspace>` (všetky jeho vlákna), súbor s prepisom pod známym koreňom prepisov alebo `last`; bez neho vypíše posledné vlákna. Iná cesta a `-` sa odmietnu |
| `part` | integer | 1 | časť dlhého prepisu |
| `budget_chars` | integer | 20000 | znakov na časť (najviac 80000) |

Kompaktné, zamaskované prepisy vlákien agentov tohto repozitára na tomto počítači. Notifikácia prostredia (`<task-notification>` v Claude Code o dokončenej úlohe na pozadí) sa zmení na jeden riadok a cesty do dočasného adresára relácie agenta sa skrátia na `<tmp>/`. Časť sa zmestí do jedného výsledku nástroja, dlhšie klienti orezávajú. Pozri [Znalosti z vlákien agentov](../guides/08-distill-threads.md).

### `ctx_inbox`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `action` | `list` \| `show` \| `promote` \| `drop` \| `verify` | `list` | |
| `ids` | string[] | | pre show / promote / drop; pri verify jedno ID. Presné id alebo predpona či prípona, ktorá sedí na jedného kandidáta |
| `verify` | string | | pri `action=verify`: príkaz, ktorý návrh dokázal (ako v `ctx_capture`); `""` ho odstráni. Povýšenie ho zachová |

### `ctx_prepare_commit`

| Parameter | Typ | |
| --- | --- | --- |
| `promote` | string[] | ID z inboxu, ktoré sa zapíšu do úložiska a stagnú |
| `drop` | string[] | ID z inboxu, ktoré sa zahodia; najprv sa overí každé ID a kandidát, ktorého zapísal iný worktree, sa odmietne |
| `retire` | string[] | záznamy s námietkou z tohto worktree (alebo námietky): stav `deprecated`, námietka ako dôvod v poznámke `## Retired`, stagnuté |

Vráti report opísaný v [Zachytávanie a review](../concepts/03-capture-and-review.md#before-committing-ctx-prepare-commit). Keď nie je nič stagnuté, prečíta zmeny pracovného stromu voči `HEAD` (ako pri jj) a povie to. Uvedie trailery commitu a či ich pridá hook prepare-commit-msg: v klone bez hookov kontextu (čerstvý klon, projekt v Superset) ich treba dopísať ručne, alebo spustiť `kontext hooks install`.

### `ctx_init`

| Parameter | Typ | Predvolene | |
| --- | --- | --- | --- |
| `count` | integer | 1 | koľko úloh vrátiť (do 5) |
| `bootstrap` | boolean | false | založí úložisko znalostí, keď ho repozitár nemá (len keď o to požiadal používateľ) |

Keď repozitár ešte nemá `.ai/` a `bootstrap` je true, `ctx_init` najprv spustí scan, history a render (bez hookov) a potom vráti prvú úlohu; bez neho vysvetlí, ako kontext zaviesť. Vetvu nikdy nezavádza, keď tímové znalosti už má iná vetva.

V repozitári bez úložiska znalostí prijme `ctx_capture` len `visibility: private` a `ctx_inbox` / `ctx_prepare_commit` nepovyšujú.

Zachytenie, ktorého cesty všetky patria inému repozitáru s tímovými znalosťami, ide do inboxu toho repozitára. Pozri [Viac repozitárov](../concepts/09-several-repositories.md#limits).

### `ctx_init_submit`

| Parameter | Typ | |
| --- | --- | --- |
| `task_id` | string (povinné) | `purpose`, `mod:<module>`, `dec:<module>-<sha7>`, `refresh:<module>` |
| `summary` | string | jeden riadok, najviac ~200 znakov |
| `overview` | string | Markdown, 5–15 riadkov |
| `decisions` | object[] | `{title, decision, summary?, context?, alternatives?, consequences?, paths?, commits?, date?, status?, tags?}` (pre úlohy `dec:`) |
| `learnings` | object[] | `{title, body, paths?, tags?}` |
| `skip` | boolean | preskočí úlohu |
| `reason` | string | prečo sa preskakuje |

`paths`, `commits` a `tags` prijímajú aj reťazce oddelené čiarkou.

### Nástroje adaptérov

Nástroje MCP adaptérov s `expose` sa vypisujú s ich pôvodnými schémami (názvy s prefixom `<adapter>_`, ak ním už nezačínajú); nástroje deklarované v konfigurácii adaptéra sa vypisujú so schémou z `params`. Ich popisy začínajú `[adapter]`. Vystavené nástroje si ponechajú názov a anotácie zo zdroja; chýbajúce sú `readOnlyHint: false`, `openWorldHint: true`.

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
