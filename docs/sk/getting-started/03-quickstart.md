# Rýchly štart

Tento návod zavedie kontext do existujúceho repozitára, pripojí agenta a zapíše prvé rozhodnutie.

## 1. Zaveď kontext do repozitára

```sh
cd your-repo
kontext init
```

```text
kontext init · github.com/acme/shop
[1/5] scan     ✓ 3149 files · TypeScript 91% · Nx/npm workspaces · 94 modules · 63 manifests (1.2s)
[2/5] history  ✓ 1498 commits · 220 decision-shaped in 36 areas (0.7s)
[3/5] render   ✓ .ai/architecture: overview (written) + 40 module docs (40 new, 0 updated) · config .ai/kontext.toml (0.1s)
[4/5] wire     ✓ git hooks in .git/hooks: post-commit, post-merge, post-rewrite, pre-commit, prepare-commit-msg
[5/5] deepen   · 0/51 tasks done — let your agent continue with the `kontext-init` prompt …
```

Prvé štyri fázy sú deterministické a trvajú sekundy. Zapíšu do `.ai/` prehľad architektúry a jeden dokument na každý modul, použijú existujúci adresár ADR, ak ho máš, a nainštalujú git hooky vedľa hookov, ktoré už používaš. Nič sa necommituje.

Pozri si výsledok a commitni ho na vetve, aby tím videl zavedenie v pull requeste:

```sh
git switch -c kontext/bootstrap
git add .ai && git commit -m "docs: bootstrap team knowledge"
```

## 2. Pripoj svojho agenta

```sh
kontext connect claude --write      # writes .mcp.json (project scope, shared with the team)
kontext connect agents-md --write   # adds a short "how to use kontext" block to AGENTS.md / CLAUDE.md
kontext connect codex               # prints the ~/.codex/config.toml snippet (--write appends it)
```

Cursor, OpenCode a automatický brief na začiatku session nájdeš v [Nastavení agentov](./04-setup-for-agents.md).

## 3. Nechaj agenta prehĺbiť zavedenie

V Claude Code spusti prompt **`/mcp__kontext__kontext-init`** alebo požiadaj ktoréhokoľvek pripojeného agenta, nech „pokračuje v zavádzaní kontextu“. Cez `ctx_init` si ťahá malé úlohy – opísať projekt, zhrnúť modul, vydestilovať rozhodnutia zo zhluku commitov – a každú odovzdá cez `ctx_init_submit`. Zastaviť môžeš kedykoľvek; postup sa uchováva v súboroch.

Nemáš po ruke agenta? Použi LLM CLI ako autopilota:

```sh
kontext adapters add llm-claude
kontext init --deepen --llm llm-claude --jobs 3 --max 20
```

## 4. Pracuj s ním

```sh
kontext brief --focus src/billing      # what an agent sees first
kontext search "rounding of prices"    # knowledge, docs, commit history, adapters
kontext why src/billing/round.ts:40    # decisions, module summary, history, blame
kontext log                            # the decision timeline
```

## 5. Zapíš rozhodnutie a pošli ho so zmenou

```sh
kontext capture --kind decision --title "Prices are integer cents" \
  --paths "src/billing/**" \
  --body "Floats broke VAT rounding. Every amount is an integer number of cents."

git add src/billing/round.ts
kontext prepare-commit                          # shows the candidate next to the staged change
kontext prepare-commit --promote <inbox-id>     # writes .ai/decisions/…md and stages it
git commit -m "fix(billing): round in cents"
git log -1 --format=%B                          # … Decision: 2026-09-28-prices-are-integer-cents
```

Agenti robia to isté cez `ctx_capture` a `ctx_prepare_commit`. Pre-commit hook záznam zvaliduje, preskenuje na tajné údaje a obnoví `.ai/README.md`; hook prepare-commit-msg doplní trailer.

## Ďalšie kroky

- [Tímový workflow](../guides/02-team-workflow.md) – review, nahrádzanie rozhodnutí, zaúčanie kolegov
- [Adaptéry](../adapters/01-overview.md) – zapoj OpenViking, CodeGraph, Serenu, archívy sessions
- [Zavedenie do existujúceho repozitára](../guides/01-bootstrap-an-existing-repository.md) – monorepá, existujúce ADR, klientske repozitáre
