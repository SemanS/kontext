# Zoznam zmien

Tu sú uvedené všetky podstatné zmeny. Projekt sa riadi [sémantickým verzovaním](https://semver.org/); do verzie 1.0 môžu minor verzie meniť konfiguráciu alebo formát záznamov, vždy s poznámkou k migrácii.

## 0.1.2 – 2026-09-29

Z nasadenia kontextu do toho monorepa s pripojeným Claude Code a Codexom.

**Zavádzanie**
- Volania LLM z autopilota sa už nedostanú k vlastnému MCP serveru kontextu a prompt žiada len JSON odpoveď: keď bol kontext pripojený ku Codexu, model každú úlohu zapísal sám cez `ctx_init_submit` a druhýkrát cez odpoveď, takže poznatky a rozhodnutia vznikali dvakrát (`…-2`). Preset `llm-codex` odovzdáva `-c mcp_servers.kontext.enabled=false`, `llm-claude` `--strict-mcp-config`. Presety žijú v tvojom configu: obnov ich cez `kontext adapters add llm-codex --force` (a `llm-claude`).
- Kým beží `kontext init --deepen`, `ctx_init_submit` z iného procesu úlohy tohto behu odmietne.
- Opakovane odovzdaná úloha nahradí vlastné skoršie poznatky a rozhodnutia namiesto pridávania kópií a záznamy, ktoré napísali ľudia, nechá na pokoji.
- Vetva bez tímových znalostí sa dozvie, keď ich má iná vetva (bootstrap čakajúci na review): brief odporučí zlúčenie a `ctx_init` odmietne založiť druhé, konfliktné `.ai/`.
- Záznamy vyťažené initom nedostanú trailery commitu – pochádzajú zo starších commitov uvedených v ich `commits` a bootstrap commit už nenesie desiatky riadkov `Decision:` / `Learning:`.

**Agenti**
- Keď je kontext zaregistrovaný pre všetky repozitáre (user scope), tam, kde ho nikto nezaviedol, len číta: v repozitári bez úložiska znalostí to povedia MCP inštrukcie, `ctx_capture` odmietne tímové záznamy (súkromné poznámky fungujú ďalej), povýšenia sa odmietnu a `ctx_init` zavádza len s `bootstrap=true` (odovzdá ho prompt `kontext-init`).
- `kontext connect codex` zapisuje do `$CODEX_HOME/config.toml`, keď je `CODEX_HOME` nastavené (ako to robia obaly prepínajúce účty), v prípade potreby vytvorí adresár a zálohu spomenie, len keď ju naozaj vytvoril.

## 0.1.1 – 2026-09-28

Opravy z testu v praxi na monorepe s 74 modulmi (moon/Cargo), na ktorom paralelne pracujú agenti Claude Code a Codex.

**Agenti**
- Codex už pri `approval_policy = "never"` neodmieta nástroje kontextu: každý nástroj nesie MCP anotácie (`readOnlyHint`, `destructiveHint`, `title`) a `kontext connect codex --write` pridá `default_tools_approval_mode = "approve"` (aj do existujúceho záznamu).
- Paralelné worktree: `ctx_prepare_commit`, pripomienka v pre-commit hooku a brief ponúkajú len kandidátov z inboxu zachytených v aktuálnom worktree; kandidáti iného živého worktree sa vypíšu zvlášť („not for this commit“), kandidátov zmazaného worktree si môže prevziať ktokoľvek.
- Brief vypíše každý `AGENTS.md` / `CLAUDE.md` medzi koreňom a zameranou cestou a zameranému modulu bez dokumentu modulu dá fakty zo skenu.
- Prázdne hodnoty v zoznamových argumentoch (`--paths a,`, agentove `[""]`) sa zahodia, namiesto toho aby sa stali cestami, tagmi alebo nahradeniami záznamu.

**Git**
- `git commit --amend -m …` zachová trailery `Decision:` / `Convention:` … (amend sa meria od rodiča `HEAD`); dostanú ich aj commity z `git merge --squash`.
- Hooky čítajú stagnuté znalosti jedným `git cat-file --batch` a cesty záznamov porovnávajú bez opakovanej kompilácie globov: commit 44 súborov znalostí sa skrátil zo 4,2 s na 0,4 s.
- Vyhľadávací index zabudne commity, ktoré z histórie vypadli (amend, rebase).

**Zavádzanie**
- Hľadanie rozhodnutí rozpozná voľby formulované ako pravidlá („X, not Y“, „never“, „no longer“, „is gone“), viac slov zdôvodnenia a dlhé vysvetľujúce telá aj zakladajúci commit; keď prah prejde málo commitov, doplnia sa ďalšie najlepšie až do štvrtiny histórie. Malé zhluky sa pridajú k nadradenému modulu, veľké sa rozdelia na chronologické časti po najviac 8 commitoch; `init.max_decision_tasks` je predvolene 20. Z vyťažených správ sa odstránia trailery s autorstvom.
- Popisy modulov sa v núdzi vezmú z dokumentačného komentára balíka (docstring v `__init__.py`, `//!` dokumentácia crate, `doc.go`); úvody README, ktoré končia uvádzacou vetou (`…:`), si zachovajú prvé vety.
- Prehľad vymenuje moduly bez dokumentu a spočíta súbory s pravidlami pre agentov, ktoré nevypisuje.

**Vyhľadávanie**
- Federované vyhľadávanie spája zdroje váženým recipročným poradím, takže stlačené skóre podobnosti adaptéra už nevytláča lokálne znalosti; kópie lokálnych záznamov z adaptéra (synchronizované `…/<id>.md`) splynú s lokálnym výsledkom.

**Bezpečnosť**
- Tajné údaje zapísané vo vetách („the secret is …“, „rotate the token …“) sa zachytia, keď hodnota vyzerá náhodne; cesty a zástupné hodnoty zostanú nedotknuté.

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
