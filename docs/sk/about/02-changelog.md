# Zoznam zmien

Tu sú uvedené všetky podstatné zmeny. Projekt sa riadi [sémantickým verzovaním](https://semver.org/); do verzie 1.0 môžu minor verzie meniť konfiguráciu alebo formát záznamov, vždy s poznámkou k migrácii.

## 0.2.0 (2026-10-03)

**Migrácia:** `kontext hooks install --dir <dir>` je teraz `--hooks-dir <dir>` (rovnako `uninstall`). `--dir` pomenúva repozitár, rovnako ako `-C`.

**Znalosti v submoduloch a iných worktree.** Session, ktorá začala v repozitári aplikácie, ale pracovala v submodule, v inom worktree alebo v susednom projekte, dostala od kontextu „not set up“ a jej zachytenia skončili v inboxe, z ktorého sa nedali povýšiť. Teraz rozhodujú cesty, ktorý repozitár odpovie, a každý nástroj prijíma `dir`. Pozri [Viac repozitárov](../concepts/09-several-repositories.md).

**Súkromné poznámky sa dajú znova nájsť.** Súkromné zachytenie v repozitári bez úložiska sa raz zapísalo a už nikdy neprečítalo, lebo vyhľadávanie inbox nepokrývalo. `ctx_search` teraz nájde poznámky v inboxe a brief uvedie tie, ktoré zodpovedajú jeho focusu.

**Neaktuálne rozhodnutia sú označené.** Rozhodnutie sa zapíše raz, ale jeho kód sa ďalej mení. *Stav* v briefe a `kontext status` vypíšu rozhodnutia, ktorých cesty od ich prijatia zasiahlo `[freshness] threshold_commits` (predvolene 20) commitov. Pozri [Aktuálnosť](../concepts/04-context-layers.md#freshness).

**Kratšie odpovede.** Riadok rozhodnutia už neopakuje svoj titulok ako id: odkazy sú najkratšia jednoznačná predpona (`[2026-09-28-adapters]`), takže riadok je asi o 20 % kratší. `ctx_threads` zmení každú notifikáciu o úlohe na pozadí z Claude Code na jeden riadok. Vlákno, ktoré rozdeľovalo prácu, sa zmenšilo z 81 000 na 30 000 znakov a časť (predvolene 20 000 znakov) sa teraz zmestí do jedného výsledku nástroja.

**Čerstvé klony.** Klon od orchestrátora alebo z CI nemá git hooky. Brief to povie a `ctx_prepare_commit` už nesľubuje trailery, ktoré žiadny hook nepridá.

**Codex** sa riadi `AGENTS.md`, nie inštrukciami MCP servera. `kontext connect agents-md --write` patrí k jeho nastaveniu.

## 0.1.5 (2026-09-29)

**Autopilot**
- `llm-claude` beží na subscription prihláseného účtu (`ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` sa pre jeho volania zahodia), bez nástrojov, MCP serverov a s krátkym systémovým promptom (zhruba desatina tokenov na úlohu) a volania sa neukladajú ako sessions Claude Code. Nová premenná `effort` (predvolene `high`); časový limit 10 minút. Opus: `kontext adapters add llm-claude --force --var model=claude-opus-5-5`.

**Init**
- `kontext init --no-hooks` sa zapamätá pre klon: neskoršie `init` či `init --deepen` z akéhokoľvek worktree už nenainštaluje hooky tam, kde ich niekto odmietol; `kontext hooks install` ich pridá.

## 0.1.4 (2026-09-29)

**Citlivé vlákna**
- Prepisy vlákien (`kontext distill`, `ctx_threads`) a to, čo sa z nich vydestiluje, maskujú okrem tajných údajov aj e-mailové adresy, telefónne čísla, IBAN, čísla kariet a IP adresy; `secrets.redact` pridá tvoje vlastné vzory (meno klienta, id integrácie).
- Model má pokyn písať roly namiesto mien ľudí a vynechať kontakty, záznamy, prístupové údaje a hosty; jeho záznamy sa znova zamaskujú.
- Namerané na klientskom vlákne: 52 e-mailových adries, 2 telefónne čísla a 13 IP adries sa už nedostane k modelu a vydestilované záznamy nikoho nemenovali.

## 0.1.3 (2026-09-29)

**Znalosti z vlákien agentov**
- `kontext distill` číta vlákna Claude Code a Codexu (všetky účty Claude, `$CODEX_HOME`), workspaces v Supersete (podľa id, názvu worktree, cesty alebo vetvy; samotné `--superset` znamená aktuálny) alebo akýkoľvek text a trvalé rozhodnutia, konvencie, úskalia a incidenty, ktoré v nich nájde LLM adaptér, zachytí do inboxu spolu s cestami, commitmi, `origin: thread` a vláknom ako zdrojom. Prepisy sa skrátia a zamaskujú, dlhé vlákna sa rozdelia na časti a porovnajú sa s tým, čo už je zapísané. Pozri [Znalosti z vlákien agentov](../guides/08-distill-threads.md).
- `ctx_threads` (len na čítanie) dá pripojeným agentom tie isté prepisy po častiach; prompt `kontext-distill` prevedie agenta spracovaním vlákna cez `ctx_capture`, ktorý teraz prijíma `source` a `commits`.
- Vlákna workspace v Supersete, ktoré bežali v inom repozitári, sa preskočia a workspace iného repozitára sa odmietne.

## 0.1.2 (2026-09-29)

Z nasadenia kontextu do toho monorepa s pripojeným Claude Code a Codexom.

**Zavádzanie**
- Volania LLM z autopilota sa už nedostanú k vlastnému MCP serveru kontextu a prompt žiada len JSON odpoveď: keď bol kontext pripojený ku Codexu, model každú úlohu zapísal sám cez `ctx_init_submit` a druhýkrát cez odpoveď, takže poznatky a rozhodnutia vznikali dvakrát (`…-2`). Preset `llm-codex` odovzdáva `-c mcp_servers.kontext.enabled=false`, `llm-claude` `--strict-mcp-config`. Presety žijú v tvojom configu: obnov ich cez `kontext adapters add llm-codex --force` (a `llm-claude`).
- Kým beží `kontext init --deepen`, `ctx_init_submit` z iného procesu úlohy tohto behu odmietne.
- Opakovane odovzdaná úloha nahradí vlastné skoršie poznatky a rozhodnutia namiesto pridávania kópií a záznamy, ktoré napísali ľudia, nechá na pokoji.
- Vetva bez tímových znalostí sa dozvie, keď ich má iná vetva (bootstrap čakajúci na review): brief odporučí zlúčenie a `ctx_init` odmietne založiť druhé, konfliktné `.ai/`.
- Záznamy vyťažené initom nedostanú trailery commitu: pochádzajú zo starších commitov uvedených v ich `commits` a bootstrap commit už nenesie desiatky riadkov `Decision:` / `Learning:`.

**Agenti**
- Keď je kontext zaregistrovaný pre všetky repozitáre (user scope), tam, kde ho nikto nezaviedol, len číta: v repozitári bez úložiska znalostí to povedia MCP inštrukcie, `ctx_capture` odmietne tímové záznamy (súkromné poznámky fungujú ďalej), povýšenia sa odmietnu a `ctx_init` zavádza len s `bootstrap=true` (odovzdá ho prompt `kontext-init`).
- Rozhodnutie alebo poznatok zachytený bez `paths` a povýšený so zmenou sa vzťahuje na súbory tej zmeny (konvencie zostávajú pre celý repozitár), takže ho `ctx_why` nájde pri kóde, ktorý vysvetľuje.
- `kontext connect codex` zapisuje do `$CODEX_HOME/config.toml`, keď je `CODEX_HOME` nastavené (ako to robia obaly prepínajúce účty), v prípade potreby vytvorí adresár a zálohu spomenie, len keď ju naozaj vytvoril.

## 0.1.1 (2026-09-28)

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

## 0.1.0 (2026-09-28)

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
