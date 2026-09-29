# Integrácie agentov

kontext je MCP server: `kontext mcp` komunikuje cez JSON-RPC 2.0 na stdio (verzie protokolu 2024-11-05 až 2025-11-25). Každý MCP klient dostane rovnaké nástroje, prompty a zdroje.

## Čo agenti dostanú

**Nástroje** (parametre nájdeš v [referencii MCP](../reference/02-mcp.md)):

| Nástroj | Kedy ho má agent zavolať |
| --- | --- |
| `ctx_brief` | na začiatku každej úlohy, s `focus` = cesty alebo téma, na ktorých bude pracovať |
| `ctx_search` | na vyhľadanie rozhodnutí, dokumentácie, histórie alebo pamäte o niečom |
| `ctx_read` | na otvorenie výsledku na úrovni L0/L1/L2 |
| `ctx_why` | pred zmenou kódu, ktorého dôvod nie je jasný (`path`, `path:line`, commit, symbol) |
| `ctx_log` | na zobrazenie časovej osi rozhodnutí |
| `ctx_capture` | keď sa rozhodlo alebo zistilo niečo trvalé |
| `ctx_inbox` | na vypísanie, zobrazenie, povýšenie alebo zahodenie zachytených kandidátov |
| `ctx_prepare_commit` | pred každým commitom |
| `ctx_init`, `ctx_init_submit` | na zavedenie a prehĺbenie znalostí repozitára |

Vedľa nich sa zobrazia aj nástroje znovu publikované z MCP adaptérov (`expose`) a nástroje deklarované v konfigurácii adaptérov.

**Prompty**: `kontext-init`, `kontext-commit`, `kontext-reflect` (v Claude Code ako slash príkazy).

**Zdroje**: `kontext://brief` a `kontext://entry/<id>` pre klientov, ktorí prehliadajú zdroje.

**Inštrukcie**: pri pripojení server agentovi v piatich vetách vysvetlí, ako používať nástroje uvedené vyššie.

## Správanie servera

- Požiadavky sa spracúvajú súbežne.
- Repozitár sa zistí z pracovného adresára (alebo z `-C <dir>`, prípadne z `KONTEXT_DIR`). Server sa spustí aj mimo repozitára a kým nejaký nedokáže otvoriť, odpovedá jasnou chybou.
- Zmeny konfigurácie (nový `.ai/kontext.toml`, zmenený adaptér) sa prejavia bez reštartu.
- MCP servery adaptérov sa spustia pri prvom použití a bežia až do konca session.

## Klienti

| Klient | Návod |
| --- | --- |
| Claude Code | [Claude Code](./02-claude-code.md) |
| Codex | [Codex](./03-codex.md) |
| Cursor, OpenCode, Superset, ďalší MCP klienti | [Ďalší klienti](./04-other-clients.md) |

## Bez MCP

Všetko existuje aj ako CLI príkaz (`kontext brief`, `kontext search`, …) a `kontext call <tool> '<json>'` spustí ľubovoľný nástroj lokálne, čo je šikovné pre prostredia agentov bez podpory MCP a pre skripty.
