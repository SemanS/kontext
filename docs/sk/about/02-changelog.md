# Zoznam zmien

Tu sú uvedené všetky podstatné zmeny. Projekt sa riadi [sémantickým verzovaním](https://semver.org/); do verzie 1.0 môžu minor verzie meniť konfiguráciu alebo formát záznamov, vždy s poznámkou k migrácii.

## 0.1.0 – 2026-09-28

Prvé verejné vydanie.

**Úložisko znalostí**
- Markdown záznamy (decision, convention, learning, incident, architecture) s plochým front matter alebo klasické ADR s poliami `**Status:**`; číslovanie podľa dátumu, sekvenčné alebo žiadne; nahrádzanie; validácia; generovaný `.ai/README.md` zlučovaný cez union.

**Agenti**
- MCP server cez stdio: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init`, `ctx_init_submit`; prompty `kontext-init`, `kontext-commit`, `kontext-reflect`; zdroje `kontext://brief` a `kontext://entry/{id}`.
- `kontext connect` pre Claude Code (projektový `.mcp.json`, hook SessionStart), Codex, Cursor, OpenCode a `AGENTS.md`.

**Zavedenie**
- `kontext init`: scan (jazyky, manifesty vrátane Nx a moon, stack, moduly a oblasti, závislosti, ADR, pravidlá pre agentov), history (signály rozhodnutí, výmeny závislostí, churn, konvencie), render, wire a protokol prehĺbenia pre agentov alebo LLM adaptér, ktorý sa dá prerušiť a obnoviť.

**Vyhľadávanie**
- Tantivy BM25 nad záznamami, dokumentmi rozdelenými na sekcie a históriou commitov s rozkladaním identifikátorov; federované vyhľadávanie naprieč adaptérmi; `ctx_why` pre cesty, rozsahy riadkov, commity a symboly; časová os rozhodnutí.

**Adaptéry**
- Generické drivery `mcp` (stdio a streamable HTTP), `http` a `command`; operácie, udalosti (`capture`, `promote`, `sync`), naväzovanie argumentov MCP zo schémy, mapovanie výsledkov (`items`, `split`, `where`, `map` s cestami, `re:`, `tpl:`, literálmi), federované a deklarované nástroje, podmienky `when`, dôvera pre adaptéry deklarované v repozitári.
- Presety: OpenViking, CodeGraph, Serena, Agent LCM, sessions, LLM Claude/Codex/Ollama, Slack webhook.

**Git**
- Hooky (validácia a skenovanie tajných údajov v pre-commit, trailery v prepare-commit-msg, synchronizácia v post-commit/merge/rewrite) inštalované popri existujúcich hookoch, `core.hooksPath` a husky; outbox s opakovaním; lokálny stav, ktorý pozná worktree.
