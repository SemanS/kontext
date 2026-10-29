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

### Flows {#flows}

Sklad, ktorý rástol zachytávaním, je kopa zoradená podľa dátumu a druhu. **Flow** je záznam druhu `architecture`, ktorý ju číta ako proces: `architecture/flows/flow-<name>.md`, id `flow-<name>` (alebo štítok `flow`), `paths` s modulmi, cez ktoré proces prechádza, a `## Trail` — číslovaný zoznam `[[id-záznamu]] — jedna veta` v poradí, v akom čitateľ záznamy stretáva, s nadpismi `###` pre etapy. Flow záznamy prepája a nič na nich nemení: nekopíruje, nepremenúva, nevyraďuje, takže trailery, denník overení a ostatné odkazy ďalej platia. Brief s focusom vypíše flows cez cesty focusu s počtom krokov; píše ich [skill](../agent-integrations/02-claude-code.md#skills) `kontext-flows`.

## Formát záznamu

Predvolený štýl je plochý front matter a krátke telo:

```markdown
---
id: 2026-09-28-prices-are-integer-cents
kind: decision
title: Prices are integer cents
status: accepted            # proposed | accepted | superseded | deprecated | rejected
date: 2026-09-28
review_by: 2027-03-31        # optional: when to look at it again (not an expiry)
checked: 2026-10-20T14:05:09Z  # optional: when someone last confirmed it still holds (or a date)
verify: cargo test -p billing cents   # optional: one command that shows it still holds
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

## Alternatives
Decimal columns: exact, but every language on the path needs a decimal type.

## Consequences
Conversions happen at the edges (API, UI). Existing float columns are migrated.
```

`## Alternatives` je nepovinná: zvážené možnosti a prečo každú zamietli, aby neskoršia námietka znova neotvárala tú, ktorá už neprešla.

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

Rozpoznané polia: Status, Date, Review by, Checked, Tags, Paths, Supersedes, Superseded by, Summary, Commits, Author/Authors/Deciders/Owner. `Status: Superseded by 0009` nastaví stav aj odkaz naraz.

ADR písané cez adr-tools držia stav v sekcii `## Status` (`Accepted`, `Superseded by [9. …](0009-….md)`). kontext túto sekciu číta a pri nahradení alebo vyradení ju zapíše v rovnakom štýle. Záznam v štýle polí bez riadku Status ho dostane, keď sa jeho stav zmení.

## ID a názvy súborov

| Číslovanie | Názov súboru | Typicky pre |
| --- | --- | --- |
| `date` (predvolené pre rozhodnutia) | `2026-09-28-prices-are-integer-cents.md` | nové úložiská (žiadne kolízie medzi vetvami) |
| `sequential` | `0008-orders-live-in-postgresql.md` | existujúce adresáre ADR |
| `none` | `prices-are-integer-cents.md` | konvencie, poznatky |

ID je názov súboru bez prípony (alebo `id` vo front matter). Čítacie nástroje (`ctx_read`, `kontext read`, `confirm`, `challenge`) prijmú jednoznačný prefix alebo pri sekvenčných ADR číslo (`0007`). Čo sa zapisuje do záznamu (`supersedes`, `extends`, `[[odkazy]]`), sa kanonizuje na plné ID, lebo prefix, ktorý je dnes jednoznačný, ním prestane byť, keď pribudne dlhšie ID; nejednoznačný cieľ sa odmietne. Brief z rovnakého dôvodu tlačí plné ID. Duplicitné číslo ADR je chyba s menami oboch súborov a nové ID sa prideľuje naprieč všetkými druhmi. Názov bez latinských písmen dostane krátke hash ID.

## Stavy a nahrádzanie

Záznamy sú *aktívne*, pokiaľ nie sú `superseded`, `deprecated` alebo `rejected`. Keď zachytíš záznam so `supersedes: [0003]`, starý záznam sa označí ako `superseded` a dostane spätný odkaz (`superseded_by`); pri ADR v štýle polí sa riadok `**Status:**` upraví na mieste. `kontext log --all` ukáže reťazce. Funguje to pre každý druh, nielen pre rozhodnutia.

Nahradenie sa odvodzuje od aktívneho nástupcu, nielen zo starého súboru: záznam uvedený v `supersedes` aktívneho záznamu je nahradený a `superseded_by` sa doplní pri čítaní úložiska, aj keď starý súbor nikto neupravil (merge vzal jednu stranu, niekto napísal nový záznam ručne). `kontext check` také jednostranné nahradenie hlási a `kontext repair` (`--dry-run` na náhľad) dopíše chýbajúce riadky `status` a `superseded_by`. Upravujú sa ako jednotlivé riadky, takže vnorený YAML a komentáre prežijú.

Záznam, ktorý už neplatí a nemá náhradu, sa *vyradí*: jeho stav bude `deprecated` a pribudne sekcia `## Retired` s dátumom a dôvodom, takže tvrdenie aj dôvod, prečo padlo, ostanú čitateľné. Nič sa neprepisuje na mieste. Vyradenie prechádza review ako každá zmena: agent záznam [napadne](./03-capture-and-review.md#challenges) a vyradenie sa stagne s commitom (`ctx_prepare_commit retire=[id]` alebo `drafts.retire` v [aplikačnom API](../reference/05-application-api.md#retire)).

## Dodatky len pridávajú {#amendments}

Reviewovaný text sa nikdy neprepisuje na mieste. Konvencia, poznatok alebo incident rastie datovanými dodatkami: sekciou `## Added <YYYY-MM-DD>` pridanou pod telo, čo pripraví `drafts.extend` a zapíše `drafts.stage amends`. Dodatok, ktorý mení existujúce telo, sa odmietne (`amend_not_additive`); tvrdenie zmeň nahradením záznamu. Rozhodnutie sa nikdy nedopĺňa: poznatok, ktorý na ňom stavia, je nový záznam s `extends: [<id>]`, ktorý `ctx_capture` a inbox prenesú až do povýšenia. Žiadny model nepíše do záznamu. Jediné úpravy na mieste sú metadátové riadky: `verify`, `checked` a `status` / `superseded_by` pri vyradení, nahradení a `kontext repair`.

## Termín kontroly {#review-by}

`review_by: YYYY-MM-DD` hovorí, kedy sa má niekto na záznam znova pozrieť: pravidlo, ktoré platí počas migrácie, dočasné obchádzanie po incidente. Je to termín kontroly, nie koniec platnosti. Po ňom je záznam označený *review due* v briefe (na svojom riadku aj v *State*), vo vyhľadávaní a v `kontext status`; nič sa neskryje. Potvrď ho (posuň dátum), nahraď alebo vyraď. Zlý dátum alebo dátum pred `date` záznamu je chyba validácie. V pochybnostiach pole vynechaj.

Druh môže novým záznamom dávať predvolenú lehotu cez `review_after` (`90d`, `12w`, `6m`, `1y`), napríklad pre incidenty:

```toml
[store.kinds.incident]
review_after = "90d"
```

Bez nastavenia je vypnutá.

## Potvrdenia {#checked}

`checked` je chvíľa, keď niekto naposledy potvrdil, že záznam stále platí: čas v UTC na sekundu (`YYYY-MM-DDTHH:MM:SSZ`, ten zapisuje `kontext confirm`) alebo deň (`YYYY-MM-DD`, ručne stačí). Kód sa mení, záznam ostáva, ako bol napísaný; keď sa naň niekto pozrie popri zmene a zistí, že stále platí, `kontext confirm <id>` (alebo `ctx_confirm`, či `drafts.confirm` v [aplikačnom API](../reference/05-application-api.md#confirm)) nastaví `checked` na aktuálny čas v UTC v súbore záznamu a stagne ho, takže potvrdenie prejde review spolu so zmenou, ktorá ho vyvolala. Vďaka času je aj druhé potvrdenie neskôr v ten istý deň zmenou súboru, a práve tú drift potrebuje vidieť. Nič iné sa v súbore nezmení, ani bajt: diff je jeden riadok (`**Checked:** …` v ADR v štýle polí).

[Aktuálnosť](./04-context-layers.md#freshness) aj [drift](./03-capture-and-review.md#drift) počítajú od neho, rovnako ako od úpravy záznamu. Čas sa počíta po svojej sekunde, deň od začiatku nasledujúceho dňa v UTC. `kontext check` odmietne `checked` v inom tvare alebo v budúcnosti. Vyradenie aj nahradenie záznamu pole zachová.

Každé potvrdenie je zároveň riadkom v [denníku overení](./03-capture-and-review.md#verifications), `.ai/confirmations.log`, s commitom, na ktorý sa pozeralo, a so spôsobom (`read`, `agent`, `bulk`); `checked` je jeho vyrovnávacia kópia. Keď merge nechá `checked` za denníkom, `kontext check` varuje a `kontext confirm --repair` ho prepíše. Voliteľné pole `verify:` uvádza jeden príkaz, ktorý ukáže, že záznam platí; spúšťa ho [`kontext verify`](./03-capture-and-review.md#verify).

## Evidencia {#evidence}

Záznam môže povedať, odkiaľ pochádza. Jeho zdroje sú odkazy v `external_refs` (vo front matter, alebo v riadku *Kontext metadata* pri ADR v štýle polí):

| Odkaz | Príklad |
| --- | --- |
| pull request, issue, úloha | `pr:123`, `issue:45`, `task:krk-335` |
| riadky súboru v revízii | `code:kontext@3f2a91c:src/x.rs#L10-20` |
| čokoľvek iné, čo sa dá otvoriť | `url:https://…` |

Odkaz na súbor bez revízie sa časom posunie, preto `code:` revíziu vždy nesie. Zachytenie môže k odkazu pridať krátky citát (do 200 znakov). Citát pri odkaze `code:` do tohto repozitára sa pri zachytení alebo stagingu overí cez `git show <rev>:<cesta>`; čokoľvek iné sa overiť nedá a tak sa aj označí, nikdy sa neodmietne. Citáty a ich výsledok idú do sekcie `## Evidence` v tele, kde ich číta reviewer:

```markdown
## Evidence

- code:kontext@3f2a91c:src/payments/retry.rs#L4 — "one idempotency key per checkout attempt" (verified)
- url:https://provider.example/docs/keys — "keys expire after 24 hours" (not verified)
```

Brief povie iba, koľko zdrojov záznam má (`(3 sources)`); `ctx_read` na úrovni 1 ich vypíše. E-mailové adresy a id sessions do odkazov nepatria.

## Ako sa záznamy parsujú {#parsing}

Front matter sa číta tak, ako ho píšu ľudia a formátery:

- zoznamy ako flow zoznamy (`[a, b]`), aj zalomené na viac riadkov (ako to robí prettier), alebo ako blokové zoznamy (`- a`),
- koncové `# komentáre`, UTF-8 BOM, konce riadkov CRLF (pri úprave riadku sa zachovajú),
- statusy `retired` a `archived` a statusy s emoji sa čítajú ako `deprecated` s varovaním.

Doslovná cesta v `paths` je ukotvená ku koreňu repozitára; segment v hranatých zátvorkách ako `app/[slug]/page.tsx` sa najprv porovná s doslovným súborom a potom ako glob, tak ako to robí git, bez ohľadu na obsah pracovného stromu. Záznam, ktorý sa nedá parsovať, je chyba v `kontext check`, nikdy sa ticho nepreskočí.

## Záznamy nech sú stručné

Záznamy majú čítať agenti v rámci tokenového rozpočtu. `kontext check` upozorní, keď telo prekročí `store.max_body_lines` (predvolene 80 neprázdnych riadkov). Detaily presuň do bežnej dokumentácie a daj na ňu odkaz.

## Indexový súbor

`.ai/README.md` vypíše všetky záznamy podľa druhu s dátumom, stavom a jednoriadkovým zhrnutím. Text, ktorý napíšeš nad alebo pod značky, zostane; časť medzi `<!-- kontext:index:start … -->` a `<!-- kontext:index:end -->` sa generuje. Keďže sa súbor znova generuje pri každom commite, ktorý sa dotkne znalostí, a je označený `merge=union`, paralelné vetvy na ňom nekolidujú.

## Validácia

`kontext check` (všetky záznamy) a `kontext check --staged` (presne to, čo by sa commitlo; to spúšťa pre-commit hook) hlásia:

| Úroveň | Kontrola |
| --- | --- |
| chyba | záznam, ktorý sa nedá parsovať, chýbajúci titulok, neznámy druh, duplicitné ID alebo číslo ADR, nejednoznačný cieľ `supersedes` alebo `extends`, jednostranné nahradenie, keď starý súbor stále hovorí, že platí (opraví ho `kontext repair`), zlý `review_by` alebo `review_by` pred dátumom záznamu, `checked`, ktorý nie je dátum alebo leží v budúcnosti, `verify`, ktorý nie je jeden riadok s najviac 500 znakmi, tajný údaj s vysokou istotou |
| varovanie | rozhodnutie bez stavu alebo dátumu, nezvyčajný stav, zlý formát dátumu, dlhý záznam, `supersedes` odkazujúce nikam, odkaz, ktorý je prefixom namiesto plného id, nahradený súbor, ktorý nemenuje nástupcu (opraví ho `kontext repair`), stav čítaný ako `deprecated`, neparsovateľné alebo budúce riadky v denníku, vzor v `paths`, ktorému nezodpovedá žiadny sledovaný súbor, `checked` za svojím [denníkom](./03-capture-and-review.md#verifications) (len pri všetkých záznamoch), tajný údaj so strednou istotou |
| info | aktívny záznam odkazuje na nahradený, vyradený alebo zastaraný záznam ([zmenený predpoklad](./03-capture-and-review.md#premises); len pri všetkých záznamoch) |
