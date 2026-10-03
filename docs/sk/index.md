---
layout: home

hero:
  name: kontext
  text: Deklaratívne tímové znalosti pre coding agentov
  tagline: "Most medzi agentmi a gitom. Rozhodnutia sú súbory v repozitári, agenti navrhujú nové, ľudia ich schvaľujú v pull requestoch a každý agent pracuje s výsledkom."
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
    title: Deklarované, nie zapamätané
    details: Rozhodnutie je Markdown súbor, ktorý hovorí, čo platí, odkedy a pre ktoré cesty. Brief, vyhľadávanie a kontext why sa odvodzujú z týchto súborov a z histórie gitu. Checkoutni minuloročné vydanie a agenti uvidia rozhodnutia, ktoré platili vtedy.
  - icon: ✅
    title: Agenti navrhujú, ľudia schvaľujú
    details: Zachytenie od agenta čaká v lokálnom inboxe. Znalosťou tímu sa stane, až keď sa povýši do commitu a zlúči cez pull request, pod tými istými CODEOWNERS a CI ako kód, ktorý vysvetľuje.
  - icon: 🔗
    title: Dohľadateľné ku commitom
    details: Každé rozhodnutie má autora, dátum a stav a commit, ktorý ho prináša, nesie trailer Decision:&nbsp;s jeho id. Zrušené rozhodnutie nahradí nové, nezmaže sa, takže záznam ukáže, čo kedy platilo.
  - icon: 🧭
    title: Jeden brief pre každého agenta
    details: ctx_brief dá Claude Code, Codexu, Cursoru, OpenCode či ľubovoľnému MCP klientovi rozhodnutia pre cesty, na ktoré sa chystá siahnuť, v asi 1–2k tokenoch. Na cesty v submodule či inom worktree odpovedá repozitár, ktorému patria.
    link: /sk/concepts/09-several-repositories
    linkText: Viac repozitárov
  - icon: ⏳
    title: Neaktuálne rozhodnutia vyplávajú
    details: Rozhodnutie sa zapíše raz, ale jeho kód sa ďalej mení. Rozhodnutie, ktorého cesty od prijatia zasiahlo 20+ commitov, sa ukáže v briefe aj v kontext status.
    link: /sk/concepts/04-context-layers#freshness
    linkText: Aktuálnosť
  - icon: 🔒
    title: Lokálny, bez služby
    details: Jedna Rust binárka je MCP server, CLI aj git hooky. Žiadny účet, žiadny server a nič neopustí počítač, kým to nenastavíš. Brief trvá ~50 ms, hooky ~0,1 s.
---

## Jeden súbor na rozhodnutie

```markdown
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

Len z tohto súboru:

- agent, ktorý sa chystá upraviť `src/billing/`, dostane rozhodnutie v briefe ako prvé,
- `kontext why src/billing/round.ts:40` ho ukáže vedľa blame a commitov, ktoré k nemu viedli,
- po 20 commitoch na `src/billing/**` sa označí na kontrolu,
- novšie rozhodnutie, ktoré ho nahradí, zaujme jeho miesto a `kontext log --all` zachová reťaz.

## Prečo nie pamäť Claude Code alebo CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Automatická pamäť Claude Code | Služby pamäte | kontext |
| --- | --- | --- | --- | --- |
| Píše ju | ľudia, ako prózu | model, sám od seba | model, zo sessions | agenti navrhujú, ľudia schvaľujú |
| Zdieľaná a posúdená | v pull requestoch | nie: jeden používateľ, jeden stroj | podľa nasadenia, bez review | v pull requeste danej zmeny |
| K agentovi sa dostane | celý súbor, každú session | prvých 200 riadkov indexu, každú session | podľa podobnosti s dopytom | podľa ciest, ktoré mení |
| Záznam nesie | žiadny stav, dátum ani vlastníka | čas zápisu | podľa úložiska | cesty, stav, dátum, autora, commity |
| Keď zastará | zostane, kým si to niekto nevšimne | model ho môže prepísať | model ho môže prepísať | označí sa po 20 commitoch na jeho cestách |
| Funguje s | Claude Code; ostatní čítajú AGENTS.md | Claude Code | ich pluginom, SDK alebo MCP serverom | každým MCP klientom a CLI |

kontext nenahrádza CLAUDE.md: nechaj tam pár pokynov, ktoré potrebuje každá session. Rozhodnutí je priveľa na to, aby sa načítavali celé, a sú pridôležité na to, aby zostali neposúdené. [Viac o alternatívach](/sk/getting-started/01-introduction#why-not)

## Pod tvojou kontrolou

- **Schvaľovanie.** Znalosti sa menia len cez commity. S `/.ai/ @acme/architects` v CODEOWNERS a povinným review od vlastníkov kódu žiadny agent nezmení to, čím sa riadia všetci agenti, bez súhlasu tohto tímu.
- **Auditná stopa.** `git log -- .ai` a `kontext log` ukážu, kto čo rozhodol a kedy. `git log --grep "Decision: <id>"` nájde commit, ktorý rozhodnutie priniesol.
- **Pravidlá v CI.** `kontext check` zastaví pull request s chybným záznamom, duplicitným id alebo uniknutým tajným údajom.
- **Bez uzamknutia.** Úložisko je obyčajný Markdown v tvojom repozitári a dá sa čítať aj bez kontextu.
