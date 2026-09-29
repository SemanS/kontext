# Úložisko znalostí

Úložisko sú zdieľané, posúdené znalosti tímu: krátke Markdown súbory v repozitári, jeden na každé rozhodnutie, konvenciu, poznatok, incident alebo poznámku o architektúre.

## Rozloženie

```text
.ai/
├── README.md                 generated index (union-merged, refreshed by the pre-commit hook)
├── kontext.toml              shared configuration (committed)
├── .gitattributes            README.md merge=union
├── decisions/                2026-09-28-prices-are-integer-cents.md   (or your existing docs/adr)
├── conventions/
├── learnings/                pitfalls are learnings tagged `pitfall`
├── incidents/
└── architecture/
    ├── overview.md           purpose + generated stack, layout and history facts
    └── modules/*.md          one per module: an overview written by an agent + generated facts
```

Názov adresára aj umiestnenie každého druhu sa dajú nastaviť ([konfigurácia](../reference/03-configuration.md#store)). Existujúci adresár ADR sa použije ako domov rozhodnutí.

## Druhy

| Druh | Adresár | Trailer | Na čo |
| --- | --- | --- | --- |
| `decision` | `decisions/` | `Decision:` | zvolený smer, zamietnutá alternatíva, zavedené pravidlo |
| `convention` | `conventions/` | `Convention:` | ako tím niečo robí (pomenovanie, vrstvenie, testovanie) |
| `learning` | `learnings/` | `Learning:` | neočividné poznatky a úskalia („gotchas“) |
| `incident` | `incidents/` | `Incident:` | čo sa pokazilo, prečo a čo sa zmenilo |
| `architecture` | `architecture/` | – | prehľad a dokumenty modulov (väčšinou generované) |

Pri zachytávaní sa akceptujú aliasy: `adr` → decision, `pitfall`/`gotcha` → learning (so štítkom `pitfall`), `note`/`lesson` → learning, `rule`/`guideline` → convention, `postmortem`/`outage` → incident.

## Formát záznamu

Predvolený štýl je plochý front matter a krátke telo:

```markdown
---
id: 2026-09-28-prices-are-integer-cents
kind: decision
title: Prices are integer cents
status: accepted            # proposed | accepted | superseded | deprecated | rejected
date: 2026-09-28
summary: Every amount is an integer number of cents.   # optional; derived from the body otherwise
tags: [billing]
paths: [src/billing/**]      # what this governs — used by ctx_brief, ctx_why, prepare-commit
supersedes: []
commits: [a1b2c3d]           # optional: commits the decision came from
author: Jane Doe
---

## Context
Floats broke VAT rounding on invoices with many lines.

## Decision
Every amount is stored and computed as an integer number of cents.

## Consequences
Conversions happen at the edges (API, UI). Existing float columns are migrated.
```

Neznáme kľúče vo front matter sa pri prepise súboru zachovajú.

### Klasické ADR (štýl polí)

Repozitáre, ktoré už ADR vedú, ich zvyčajne píšu takto a kontext ich číta aj zapisuje presne v tomto tvare:

```markdown
# 0007 — Orders live in PostgreSQL

**Status:** accepted
**Date:** 2026-08-04

## Context
…
```

Rozpoznané polia: Status, Date, Tags, Paths, Supersedes, Superseded by, Summary, Commits, Author/Authors/Deciders/Owner. `Status: Superseded by 0009` nastaví stav aj odkaz naraz.

## ID a názvy súborov

| Číslovanie | Názov súboru | Typicky pre |
| --- | --- | --- |
| `date` (predvolené pre rozhodnutia) | `2026-09-28-prices-are-integer-cents.md` | nové úložiská (žiadne kolízie medzi vetvami) |
| `sequential` | `0008-orders-live-in-postgresql.md` | existujúce adresáre ADR |
| `none` | `prices-are-integer-cents.md` | konvencie, poznatky |

ID je názov súboru bez prípony (alebo `id` vo front matter). Všade, kde sa čaká ID, môžeš použiť jednoznačný prefix alebo pri sekvenčných ADR číslo (`0007`).

## Stavy a nahrádzanie

Rozhodnutia sú *aktívne*, pokiaľ nie sú `superseded`, `deprecated` alebo `rejected`. Keď zachytíš rozhodnutie so `supersedes: [0003]`, starý záznam sa označí ako `superseded` a dostane spätný odkaz (`superseded_by`); pri ADR v štýle polí sa riadok `**Status:**` upraví na mieste. `kontext log --all` ukáže reťazce.

## Záznamy nech sú stručné

Záznamy majú čítať agenti v rámci tokenového rozpočtu. `kontext check` upozorní, keď telo prekročí `store.max_body_lines` (predvolene 80 neprázdnych riadkov). Detaily presuň do bežnej dokumentácie a daj na ňu odkaz.

## Indexový súbor

`.ai/README.md` vypíše všetky záznamy podľa druhu s dátumom, stavom a jednoriadkovým zhrnutím. Text, ktorý napíšeš nad alebo pod značky, zostane; časť medzi `<!-- kontext:index:start … -->` a `<!-- kontext:index:end -->` sa generuje. Keďže sa súbor znova generuje pri každom commite, ktorý sa dotkne znalostí, a je označený `merge=union`, paralelné vetvy na ňom nekolidujú.

## Validácia

`kontext check` (všetky záznamy) a `kontext check --staged` (presne to, čo by sa commitlo; to spúšťa pre-commit hook) hlásia:

| Úroveň | Kontrola |
| --- | --- |
| chyba | chýbajúci titulok, neznámy druh, duplicitné ID, tajný údaj s vysokou istotou |
| varovanie | rozhodnutie bez stavu alebo dátumu, nezvyčajný stav, zlý formát dátumu, dlhý záznam, `supersedes` odkazujúce nikam, vzor v `paths`, ktorému nezodpovedá žiadny sledovaný súbor, tajný údaj so strednou istotou |
