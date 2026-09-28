# História sessions

Surové sessions agentov sú na git príliš dlhé a príliš osobné, no odpovedajú na otázku „čo sa dialo, keď sme toto menili?“. kontext ich číta cez adaptéry a nikdy ich nekopíruje do repozitára.

| Op | Na čo |
| --- | --- |
| `history` | `ctx_why <path|commit|topic>` – sessions, ktoré sa dotkli cieľa |
| `search` | `ctx_search` – fulltextové alebo sémantické výsledky z prepisov |
| `brief` | krátka sekcia v `ctx_brief` (primer nedávnej práce) |

## sessions

[sessions](https://github.com/nicknisi/sessions) indexuje prepisy z Claude Code, Codexu, OpenCode a Pi a odpovedá na `why_did_this_change` tak, že porovnáva súbory, commity a čas.

```sh
kontext adapters add sessions
kontext adapters test sessions
```

Namapované nástroje: `history` → `why_did_this_change`, `search` → `search_sessions`, `brief` → `get_context_primer`.

## Agent LCM

[Agent LCM](https://github.com/Team-Volt/agent-lcm) je bezstratový archív sessions naprieč prostrediami agentov (Codex, Cursor, VS Code, GitHub Copilot, Kiro, Claude Code, OpenCode), sprístupnený cez MCP.

```sh
kontext adapters add agent-lcm
kontext adapters test agent-lcm
```

Namapované nástroje: `search` a `history` → `lcm_grep`, `brief` → `lcm_pack_context`.

Oba presety naväzujú argumenty zo schém nástrojov; ak sa nástroj zmení, `kontext adapters inspect <name>` ukáže jeho aktuálne parametre.

## Od histórie k znalostiam

Výsledky zo sessions sú dôkazy, nie pravda. Keď session odhalí rozhodnutie, ktoré stojí za to uchovať, zachyť ho – s cestami, ktorých sa týka – a povýš ho do príslušného commitu. Presne to robí na konci session prompt `kontext-reflect`.
