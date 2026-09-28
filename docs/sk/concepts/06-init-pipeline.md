# Inicializačný pipeline

`kontext init` zavádza kontext do repozitára v piatich fázach. Prvé štyri sú deterministické a skončia za pár sekúnd; piata odovzdá sémantickú prácu agentovi po malých úlohách, ktoré sa dajú prerušiť a obnoviť.

```text
[1/5] scan     inventory: languages, manifests, stack, modules, dependencies, docs, ADRs
[2/5] history  decision-shaped commits, dependency swaps, churn, commit conventions
[3/5] render   .ai/kontext.toml, architecture overview, module docs, .ai/README.md
[4/5] wire     git hooks (and agent clients with --connect)
[5/5] deepen   purpose · mod:<module> · dec:<area> · refresh:<module> tasks
```

Opakované spustenie `kontext init` je bezpečné: generované fakty sa obnovia, všetko, čo napísali ľudia alebo agenti, zostane.

## 1 · scan

Číta `git ls-files` (sledované súbory plus nesledované, ktoré nie sú ignorované). Submoduly sa nahlásia, ale neprechádzajú sa; generované a vendorované cesty (`node_modules`, `dist`, `build`, `target`, `*.min.js`, lockfily, `*.d.ts`, …) a všetko, čo vyzerá ako súbor s tajnými údajmi, sa preskočí. Vlastné výnimky pridáš cez `init.exclude`.

Zisťuje:

- **jazyky** podľa prípony, s počtom súborov a riadkov,
- **manifesty**: `package.json`, `Cargo.toml`, `pyproject.toml`, `setup.py`, `go.mod`, Nx `project.json`, moon `moon.yml`, `pom.xml`, `build.gradle(.kts)`, `composer.json`, `Gemfile`, `mix.exs`, `deno.json`,
- **nástroje workspace** (Nx, moon, Turborepo, pnpm/npm workspaces, Cargo workspaces, Bun, uv, Deno, Lerna, Go workspaces),
- **stack a nástroje** zo závislostí a súborov (React, NestJS, Axum, FastAPI, Firebase, BigQuery, Terraform, Docker, Jest, Playwright, …),
- **moduly**: každý adresár s manifestom (*deklarovaný* projekt, zostane aj keď je malý), adresáre na najvyššej úrovni s aspoň `init.min_module_files` súbormi kódu (*odvodené*) a – pri moduloch so 150+ súbormi – *oblasti* o úroveň nižšie (alebo pod jeho `src/`, `src/lib/`),
- **závislosti** medzi modulmi zo závislostí workspace, Cargo path závislostí, moon `dependsOn`, aliasov ciest v tsconfig a relatívnych či balíčkových importov,
- **symboly** (exportované funkcie, triedy, typy) cez jednoduché regexy pre TypeScript/JavaScript, Python, Rust, Go a jazyky JVM – presná inteligencia kódu je úlohou [adaptérov kódu](../guides/04-code-intelligence.md),
- **dokumenty, adresáre ADR** (s ich štýlom a číslovaním), **súbory s pravidlami pre agentov** (`AGENTS.md`, `CLAUDE.md`, …) a **názvy premenných prostredia** zo súborov typu `.env.example` (len názvy, nikdy hodnoty).

## 2 · history

Vyťaží posledných `init.history_max_commits` commitov bez merge (3000):

- **signály rozhodnutí** v správach: *replace*, *migrate*, *switch to*, *instead of*, *deprecate*, *because*, *breaking*; typy Conventional Commits (`refactor`, `feat!`); zdôvodnenie v tele,
- **výmeny závislostí** z diffov manifestov – „v tom istom commite odobral X a pridal Y“ je jeden z najsilnejších signálov,
- **churn** pre súbory a moduly a dátum poslednej zmeny,
- **konvencie**: podiel Conventional Commits, kľúče tiketov v predmetoch.

Commity, ktoré sa dotýkajú len existujúcich ADR alebo `.ai/`, sa preskočia (už sú znalosťami). Kandidáti sa zhlukujú podľa modulov; najsilnejšie zhluky sa stanú úlohami `dec:` (`init.max_decision_tasks`, predvolene 10).

## 3 · render

- `.ai/kontext.toml` (keď ešte neexistuje zdieľaná konfigurácia) – vrátane nájdeného adresára ADR ako domova rozhodnutí,
- `.ai/architecture/overview.md` – sekcia *Purpose* (úvod z README, kým ho agent nespresní) a generované fakty: stack, tabuľka modulov, ako v repozitári pracovať, história,
- `.ai/architecture/modules/<module>.md` pre prvých `init.max_module_docs` modulov (40) podľa poradia – zástupná sekcia *Overview* a generované fakty: druh, jazyk, veľkosť, manifesty, závislosti, závislé moduly, kľúčové súbory, exporty, balíčky, dokumenty, aktivita,
- `.ai/README.md` a `.ai/.gitattributes`.

Generované fakty sú medzi `<!-- kontext:facts:start … -->` a `<!-- kontext:facts:end -->`. Pri ďalších spusteniach sa prepisuje len tento blok.

Moduly sa zoraďujú podľa druhu (aplikácie a služby prvé), veľkosti, počtu modulov, ktoré od nich závisia, churnu a toho, či majú dokumentáciu.

## 4 · wire

Nainštaluje [git hooky](./07-git-integration.md), ak nepoužiješ `--no-hooks`. `--connect claude,agents-md` pripojí aj klientov agentov.

## 5 · deepen

Sémantická časť je rozdelená na úlohy, ktoré agent zvládne v jednom kroku:

| Úloha | Žiada | Zapíše |
| --- | --- | --- |
| `purpose` | čo je projekt, pre koho, jeho hlavné pohyblivé časti | zhrnutie prehľadu + *Purpose* |
| `mod:<module>` | zodpovednosť modulu, toky, invarianty, úskalia (z uvedených dokumentov a kľúčových súborov) | zhrnutie modulu + *Overview*, voliteľne poznatky |
| `dec:<area>` | najviac tri trvalé rozhodnutia vydestilované zo zhluku commitov | záznamy rozhodnutí s `commits` |
| `refresh:<module>` | aktualizované zhrnutie po zmene štruktúry modulu | zhrnutie modulu + *Overview* |

Poradie: purpose → prvých osem modulov → zhluky rozhodnutí → ostatné moduly → obnovy.

**Postup žije v súboroch.** Dokument modulu nesie `deepened` a `deepened_fingerprint`; rozhodnutia nesú commity, z ktorých vznikli; preskočené úlohy sa zapisujú do spoločného adresára klonu. Zastaviť teda môžeš kedykoľvek, pokračovať v inom worktree alebo to po pulle dokončí kolega. Keď sa súbory modulu zmenia natoľko, že sa zmení jeho odtlačok, objaví sa úloha `refresh:`.

Kto robí prácu:

- **pripojený agent** – prompt `kontext-init` alebo `ctx_init` / `ctx_init_submit` (a `ctx_init` sám spustí fázy 1–3, keď `.ai/` ešte neexistuje),
- **LLM adaptér** – `kontext init --deepen --llm <adapter>`, ktorý ku každej úlohe priloží relevantné úryvky súborov a čaká odpoveď v JSON ([návod na autopilota](../guides/06-autopilot.md)),
- **ty** – `kontext init next` vypíše úlohu, `kontext init submit answer.json` ju zapíše, `kontext init skip <task>` ju preskočí (preskočenie úlohy `refresh:` potvrdí, že zhrnutie modulu stále platí).

Odovzdané výsledky sa validujú (limity dĺžky, maskovanie tajných údajov) a zapíšu do pracovného stromu na review.
