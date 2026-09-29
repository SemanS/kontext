# Architektúra

```text
                 Claude Code · Codex · Cursor · OpenCode · any MCP client
                                         │  MCP (stdio, JSON-RPC)
                                ┌────────┴────────┐
                                │     kontext     │  tools · prompts · resources
                                │  (one binary)   │  CLI · git hooks
                                └──┬─────┬─────┬──┘
             adapters (config) ────┘     │     └──── git hooks
   ┌──────────────┬──────────────┐       │       pre-commit        validate · secret-scan · index
   │ mcp driver   │ http driver  │       │       prepare-commit-msg  Decision: trailers
   │ command drv  │              │       │       post-commit/merge/rewrite  sync events
   └──────┬───────┴──────┬───────┘       ▼
          │              │         TEAM TRUTH (in the repository)   LOCAL, DERIVED, PRIVATE
   CodeGraph, Serena,  OpenViking,  .ai/decisions/*.md   ◄─ PR ──  .git/kontext/inbox/      candidates
   Agent LCM, sessions Ollama, …    .ai/conventions/, learnings/   .git/kontext/worktrees/… search index
   claude -p, codex    Slack        .ai/architecture/…             .git/kontext/outbox.jsonl adapter deliveries
```

## Komponenty

| Komponent | Zdroják | Zodpovednosť |
| --- | --- | --- |
| CLI | `src/main.rs` | všetky príkazy; `kontext mcp` spustí server |
| MCP server | `src/mcp.rs`, `src/tools.rs` | JSON-RPC cez stdio, súbežné požiadavky, nástroje, prompty a zdroje |
| Operácie | `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| Úložisko | `src/store.rs` | záznamy v repozitári: parsovanie, validácia, prideľovanie ID, generovanie indexu |
| Inbox | `src/inbox.rs` | lokálni kandidáti v spoločnom git adresári |
| Index | `src/index.rs` | Tantivy BM25 nad záznamami, dokumentmi a commitmi; inkrementálna obnova |
| Adaptéry | `src/adapters/` | register, drivery (MCP klient, HTTP, command), mapovanie výsledkov, udalosti, federované nástroje |
| Udalosti | `src/events.rs` | outbox s opakovaním; doručovanie na pozadí |
| Hooky | `src/hooks.rs` | inštalácia/odinštalovanie; obsluha pre-commit, prepare-commit-msg a post-* |
| Init | `src/init/` | sken, ťaženie histórie, generovanie, protokol úloh prehĺbenia |
| Konfigurácia | `src/config.rs` | vrstvené TOML, dôvera v adaptéry deklarované v repozitári |

## Kde žije stav

| Čo | Kde | Commitované | Zdieľané medzi worktree |
| --- | --- | --- | --- |
| Záznamy znalostí, index, zdieľaná konfigurácia | `.ai/` (alebo tvoj adresár ADR) | áno | cez git |
| Osobné adaptéry | `~/.config/kontext/config.toml` | nie | – |
| Osobné nastavenia pre jeden repozitár | `~/.config/kontext/repos/<slug>.toml` | nie | – |
| Konfigurácia pre tento klon | `<git-common-dir>/kontext/config.toml` | nie | áno |
| Inbox (kandidáti) | `<git-common-dir>/kontext/inbox/` | nie | áno |
| Outbox (doručenia adaptérom) | `<git-common-dir>/kontext/outbox.jsonl` | nie | áno |
| Postup zavádzania | `<git-common-dir>/kontext/init-done.json` + samotné záznamy | čiastočne (záznamy) | áno |
| Vyhľadávací index, cache initu, snapshot synchronizácie | `<git-common-dir>/kontext/worktrees/<worktree>/` | nie | nie (pre každý worktree zvlášť) |
| Logy adaptérov (stderr MCP serverov) | `<git-common-dir>/kontext/logs/` | nie | áno |

`<git-common-dir>` je `$(git rev-parse --git-common-dir)`, zvyčajne `.git`. Všetko v `.git/kontext/` je odvodené alebo lokálne a dá sa zmazať.

## Identita repozitára

kontext odvodí stabilnú identitu z remote `origin`: `git@github.com:Acme/Shop.git` aj `https://github.com/acme/shop` sa zmenia na `github.com/acme/shop` (slug `github.com-acme-shop`). Každý worktree a klon rovnakého remote dostane rovnakú identitu, podľa ktorej sa riadia osobné nastavenia pre repozitár a premenné adaptérov. Bez remote je identita `local/<dir>-<hash>`.

## Tok požiadavky: `ctx_search`

1. Lokálny index sa obnoví, ak sa niečo zmenilo (záznamy, dokumenty, nové commity), najviac raz za sekundu v jednom procese.
2. Tantivy spustí dopyt (BM25 nad titulkom, telom a poliami s rozloženými identifikátormi).
3. Paralelne dostane dopyt každý adaptér s operáciou `search` (limit 12 s; zlyhávajúci adaptér si dá na 30 s pauzu).
4. Skóre sa znormalizujú pre každý zdroj, zvážia, zlúčia a zbavia duplicít podľa URI a takmer rovnakého textu.
5. Výsledok sa vypíše ako kompaktné riadky v rámci tokenového rozpočtu, každý s URI pre `ctx_read`.

## Tok požiadavky: commit

1. `pre-commit` zvaliduje stagnuté súbory znalostí, preskenuje ich na tajné údaje (pri chybách zablokuje), znova vygeneruje `.ai/README.md` zo stagnutého stromu, stagne ho a pripomenie kandidátov z inboxu, ktorí sa týkajú stagnutých ciest.
2. `prepare-commit-msg` pridá pre stagnuté záznamy trailery `Decision: <id>` (a `Convention:`, `Learning:`, `Incident:`).
3. `post-commit` porovná znalosti v `HEAD` s posledným snapshotom synchronizácie, zaradí udalosti `sync` a spustí odpojený `kontext outbox flush`.

## Rozhodnutia o návrhu

Projekt zapisuje svoje vlastné rozhodnutia kontextom. Pozri [`.ai/decisions/`](https://github.com/SemanS/kontext/tree/main/.ai/decisions).
