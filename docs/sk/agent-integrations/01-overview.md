# Integrácie agentov

kontext je MCP server: `kontext mcp` komunikuje cez JSON-RPC 2.0 na stdio. Odpovedá na bezstavovú revíziu 2026-07-28 (`server/discover`, bez `initialize`) aj na starý handshake pre verzie protokolu 2024-11-05 až 2025-11-25. Každý MCP klient dostane rovnakých šestnásť nástrojov, prompty a zdroje.

## Čo agenti dostanú

**Nástroje** (parametre nájdeš v [referencii MCP](../reference/02-mcp.md)):

| Nástroj | Kedy ho má agent zavolať |
| --- | --- |
| `ctx_brief` | na začiatku každej úlohy, s `focus` = cesty alebo téma, na ktorých bude pracovať |
| `ctx_search` | na vyhľadanie rozhodnutí, dokumentácie, histórie alebo pamäte o niečom |
| `ctx_read` | na otvorenie výsledku na úrovni L0/L1/L2 |
| `ctx_why` | pred zmenou kódu, ktorého dôvod nie je jasný (`path`, `path:line`, commit, symbol) |
| `ctx_log` | na zobrazenie časovej osi rozhodnutí |
| `ctx_drift` | na zistenie, ktoré záznamy zmena nechala za sebou |
| `ctx_threads` | na čítanie minulých session agentov (Claude Code, Codex, Superset) |
| `ctx_capture` | keď sa rozhodlo alebo zistilo niečo trvalé |
| `ctx_challenge` | keď záznam vyzerá zle: námietka, ktorú vyrieši človek |
| `ctx_confirm` | po overení, že záznam stále platí (zapíše sa ako `agent` pečiatka) |
| `ctx_verify` | na spustenie pinnutého príkazu `verify` záznamu; nepinnutý sa vráti, aby ho agent spustil sám |
| `ctx_inbox` | na vypísanie, zobrazenie, povýšenie alebo zahodenie zachytených kandidátov |
| `ctx_prepare_commit` | pred každým commitom |
| `ctx_init`, `ctx_init_submit` | na zavedenie a prehĺbenie znalostí repozitára |
| `ctx_api` | na volanie operácie [aplikačného API](../reference/05-application-api.md) |

Čítacie nástroje (`ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads`) majú `readOnlyHint: true`. `ctx_verify` má `destructiveHint: true` a `openWorldHint: true`; `ctx_api` má `openWorldHint: true`. Nástroje znovu publikované z MCP adaptérov (`expose`) si ponechajú anotácie zo zdroja a zobrazia sa vedľa nich, rovnako ako nástroje deklarované v konfigurácii adaptérov.

**Štruktúrované výsledky**: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox` a `ctx_drift` vracajú vedľa textu `structuredContent` a výsledky nesú bloky `resource_link` na `kontext://entry/{id}`.

**Prompty**: `kontext-init`, `kontext-commit`, `kontext-reflect` (v Claude Code ako slash príkazy).

**Zdroje**: `kontext://brief` a `kontext://entry/<id>` pre klientov, ktorí prehliadajú zdroje.

**Inštrukcie**: pri pripojení server agentovi vysvetlí, ako používať nástroje uvedené vyššie. Prvých 512 znakov stojí samostatne (čo kontext drží, `ctx_brief` ako prvé, kedy hľadať) a celok ostáva pod 4 096 znakmi, čo sú limity, ktoré čítajú Codex a Claude Code. `ctx_brief` nesie `alwaysLoad`, takže ho načítajú aj hostitelia, ktorí odkladajú nástroje za vyhľadávanie.

**Hooky**: `kontext hook --host claude|codex|cursor` doručí brief na začiatku session a riadiace záznamy pred úpravou, opt-in cez `kontext connect <host> --hooks`. Pozri [Hooky](../guides/09-hooks.md).

## Správanie servera

- Požiadavky sa spracúvajú súbežne. Panic v jednej požiadavke odpovie `-32603`; server beží ďalej.
- Repozitár sa zistí z pracovného adresára (alebo z `-C <dir>`, prípadne z `KONTEXT_DIR`). Server sa spustí aj mimo repozitára, povie to a volania s `dir` obslúži.
- Zmeny konfigurácie (nový `.ai/kontext.toml`, zmenený adaptér) sa prejavia bez reštartu; konfigurácia, ktorá sa nedá parsovať, sa vráti ako varovanie v ďalšom výsledku.
- Index obnovený iným procesom (capture z CLI, `kontext status`, druhá session) sa pred ďalšou odpoveďou znova načíta. Vetva sa číta pri každej požiadavke.
- `notifications/cancelled` zastaví požiadavku a zabije bežiaci verify príkaz.
- MCP servery adaptérov sa spustia pri prvom použití a bežia až do konca session.

## Klienti

| Klient | Návod |
| --- | --- |
| Claude Code | [Claude Code](./02-claude-code.md) |
| Codex | [Codex](./03-codex.md) |
| Cursor, OpenCode, Superset, ďalší MCP klienti | [Ďalší klienti](./04-other-clients.md) |

## Bez MCP

Všetko existuje aj ako CLI príkaz (`kontext brief`, `kontext search`, …) a `kontext call <tool> '<json>'` spustí ľubovoľný nástroj lokálne, čo je šikovné pre prostredia agentov bez podpory MCP a pre skripty.
