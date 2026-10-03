---
layout: home

hero:
  name: kontext
  text: Deklaratívne tímové znalosti pre coding agentov
  tagline: "Rozhodnutia tímu deklaruješ v repozitári. Agenti navrhujú zmeny, ľudia ich schvaľujú v pull requestoch a každý agent na tom istom commite pracuje s tým istým záznamom."
  image:
    src: /logo.svg
    alt: kontext
  actions:
    - theme: brand
      text: Začíname
      link: /sk/getting-started/01-introduction
    - theme: alt
      text: Prečo nie CLAUDE.md?
      link: /sk/getting-started/01-introduction#why-not
    - theme: alt
      text: GitHub
      link: https://github.com/SemanS/kontext

features:
  - icon: 📄
    title: Deklaratívne
    details: To, čím sa agenti riadia, je deklarované v repozitári. Jeden Markdown súbor na rozhodnutie hovorí, čo platí, odkedy a pre ktoré cesty; .ai/kontext.toml hovorí, ako sa to podáva. Nič nežije v službe ani v UI.
  - icon: 🔁
    title: Reprodukovateľné
    details: Ten istý commit dá každému kolegovi aj agentovi tie isté tímové znalosti. Lokálny index je odvodený a dá sa zmazať; ďalšie volanie ho zostaví znova. Checkoutni starší commit a agenti uvidia rozhodnutia, ktoré platili v tom commite.
  - icon: ↩️
    title: Posúdené a vratné
    details: Agenti zachytávajú do lokálneho inboxu; znalosťou tímu sa zachytenie stane až cez commit a pull request. Zrušené rozhodnutie nahradí nové, nezmaže sa, a git revert vráti zlú zmenu ako každú inú.
  - icon: 🧭
    title: Jeden brief pre každého agenta
    details: ctx_brief dá Claude Code, Codexu, Cursoru, OpenCode či ľubovoľnému MCP klientovi rozhodnutia pre cesty, ktoré pomenuje, v rámci tokenového rozpočtu (predvolene 1400). Na cesty v submodule či inom worktree odpovedá repozitár, ktorému patria.
    link: /sk/concepts/09-several-repositories
    linkText: Viac repozitárov
  - icon: ⏳
    title: Neaktuálne rozhodnutia vyplávajú
    details: Rozhodnutie sa zapíše raz, ale jeho kód sa ďalej mení. Rozhodnutie, ktorého cesty od jeho dátumu zasiahlo 20 a viac commitov (nastaviteľné), sa označí v briefe aj v kontext status.
    link: /sk/concepts/04-context-layers#freshness
    linkText: Aktuálnosť
  - icon: 🔒
    title: Lokálny, bez služby
    details: Jedna Rust binárka je MCP server, CLI aj git hooky. Žiadny účet ani server; nič neopustí počítač, kým to nenastavíš.
---

## Deklaruj to

::: code-group

```markdown [.ai/decisions/2026-09-28-prices-are-integer-cents.md]
---
id: 2026-09-28-prices-are-integer-cents
kind: decision
title: Prices are integer cents
status: accepted
date: 2026-09-28
paths: [src/billing/**]
author: Jane Doe
---
Floats broke VAT rounding on invoices with many lines.
Every amount is stored and computed as an integer number of cents.
```

```toml [.ai/kontext.toml]
# committed, reviewed like code
[brief]
budget_tokens = 1400     # what one brief may cost an agent
max_decisions = 12

[freshness]
threshold_commits = 20   # flag a decision after this many commits on its paths

[hooks]
validate = true          # malformed entries and secrets block the commit
trailers = true          # `Decision: <id>` on the commit that ships a decision

[secrets]
scan = "staged"          # scan every staged file, not only knowledge
```

:::

Z týchto dvoch súborov kontext odvodí zvyšok:

- agent, ktorý zavolá `ctx_brief` s `focus: ["src/billing"]`, dostane rozhodnutie ako prvé,
- `kontext why src/billing/round.ts:40` ho ukáže vedľa blame a commitov, ktoré k nemu viedli,
- commit, ktorý ho prináša, nesie `Decision: 2026-09-28-prices-are-integer-cents`,
- po 20 commitoch na `src/billing/**` sa označí na kontrolu,
- novšie rozhodnutie, ktoré ho nahradí, zaujme jeho miesto a `kontext log --all` zachová reťaz.

## Prečo nie pamäť Claude Code alebo CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Automatická pamäť Claude Code | Služby pamäte | kontext |
| --- | --- | --- | --- | --- |
| Píše ju | ľudia, ako prózu | model, sám od seba | model, zo sessions | agenti navrhujú, ľudia schvaľujú |
| Zdieľaná a posúdená | v pull requestoch | nie: jeden používateľ, jeden stroj | podľa nasadenia, bez review | v pull requeste danej zmeny |
| K agentovi sa dostane | celý súbor, každú session | prvých 200 riadkov indexu, každú session | podľa podobnosti s dopytom | zoradená pre cesty, ktoré agent pomenuje, v rámci rozpočtu |
| Záznam nesie | žiadny stav, dátum ani vlastníka | čas zápisu | podľa úložiska | cesty, stav, dátum, autora |
| Keď zastará | zostane, kým si to niekto nevšimne | model ho môže prepísať | model ho môže prepísať | označí sa po N commitoch na jeho cestách |
| Funguje s | Claude Code; ostatní čítajú AGENTS.md | Claude Code | ich pluginom, SDK alebo MCP serverom | každým MCP klientom a CLI |

kontext nenahrádza CLAUDE.md: nechaj tam pár pokynov, ktoré potrebuje každá session. Rozhodnutí je priveľa na to, aby sa načítavali celé, a sú pridôležité na to, aby zostali neposúdené. [Viac o alternatívach](/sk/getting-started/01-introduction#why-not)

## Pod tvojou kontrolou

- **Schvaľovanie.** Znalosti sa menia len cez commity. S `/.ai/ @acme/architects` v CODEOWNERS a povinným review od vlastníkov kódu žiadny agent nezmení to, čím sa riadia všetci agenti, bez súhlasu tohto tímu.
- **Auditná stopa.** `git log -- .ai` a `kontext log --all` ukážu, kto čo rozhodol, kedy a čo to nahradilo. S nainštalovanými hookmi kontextu `git log --grep "Decision: <id>"` nájde commit, ktorý rozhodnutie priniesol.
- **Pravidlá v CI.** `kontext check` zastaví pull request s chybným záznamom, duplicitným id alebo tajným údajom s vysokou istotou.
- **Bez uzamknutia.** Úložisko je obyčajný Markdown v tvojom repozitári a dá sa čítať aj bez kontextu.
