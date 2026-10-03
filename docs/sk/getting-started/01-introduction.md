# Úvod

kontext sú **deklaratívne tímové znalosti pre coding agentov**. Rozhodnutia, konvencie a úskalia, ktorými sa tvoji agenti riadia, sú súbory v repozitári, schválené v pull requestoch ako kód. kontext ich dáva každému agentovi a prepája ich s commitmi, ktorých sa týkajú.

Je to jedna Rust binárka, ktorá je zároveň tromi vecami:

- **MCP server** (`kontext mcp`), ktorý môžu volať Claude Code, Codex, Cursor, OpenCode a každý ďalší MCP klient,
- **CLI** pre ľudí (`kontext brief`, `kontext why`, `kontext log`, …),
- sada **git hookov**, ktoré znalosti validujú, prepájajú s commitmi a udržiavajú aktuálny vyhľadávací index.

## Problém

Coding agenti si dnes vedú vlastnú pamäť a žiadna z nich nie je stavaná pre tím:

- **Automatickú pamäť Claude Code** píše model sám od seba a ukladá ju pre každého používateľa zvlášť do `~/.claude/projects/<project>/memory/`. Prvých 200 riadkov jej indexu sa načíta do každej session. Agenti kolegov ju nikdy neuvidia a nikto ju neschvaľuje.
- **CLAUDE.md a AGENTS.md** sú zdieľané, ale každý je próza, ktorá sa celá načíta do každej session. Pravidlo nemá vlastníka, dátum ani stav a zastarané tam zostane, kým si to niekto nevšimne.
- **Služby pamäte** ako mem0, claude-mem či Zep nechajú model vyťažiť zo sessions fakty a vybavujú ich podľa podobnosti. Zlý záver uložený raz sa vybavuje všetkým a nikto ho neodsúhlasil.

Ani jedna nevie odpovedať na to, na čo sa pýta tech lead alebo audítor: *ktorými rozhodnutiami sa naši agenti riadia v kóde fakturácie, kto ich prijal a kedy?*

## Deklarované, schválené, dohľadateľné

kontext narába so znalosťami tímu tak, ako Nix so systémom: želaný stav je deklarovaný v súboroch a všetko ostatné sa z nich odvodí.

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

- **Deklarované.** Súbor hovorí, čo platí, odkedy a pre ktoré cesty. Brief, vyhľadávanie a `kontext why` sa odvodzujú z týchto súborov a z histórie gitu; index a cache sa dajú kedykoľvek zmazať. Checkoutni minuloročné vydanie a agenti uvidia rozhodnutia, ktoré platili vtedy.
- **Schválené.** Agenti zachytávajú do lokálneho inboxu. Zachytenie sa stane znalosťou tímu, až keď sa povýši do commitu a zlúči cez pull request, takže ľudia, ktorí posudzujú kód, posúdia aj to, čím sa budú agenti riadiť.
- **Dohľadateľné.** Commit, ktorý rozhodnutie prináša, nesie trailer `Decision:`. Rozhodnutie sa ruší novším, ktoré ho nahradí, a `kontext log --all` ukáže reťaz. Po 20 commitoch na jeho cestách sa označí na kontrolu.

## Prečo nie niečo iné? {#why-not}

| | CLAUDE.md, AGENTS.md | Automatická pamäť Claude Code | Služby pamäte | kontext |
| --- | --- | --- | --- | --- |
| Píše ju | ľudia, ako prózu | model, sám od seba | model, zo sessions | agenti navrhujú, ľudia schvaľujú |
| Zdieľaná a posúdená | v pull requestoch | nie: jeden používateľ, jeden stroj | podľa nasadenia, bez review | v pull requeste danej zmeny |
| K agentovi sa dostane | celý súbor, každú session | prvých 200 riadkov indexu, každú session | podľa podobnosti s dopytom | podľa ciest, ktoré mení |
| Záznam nesie | žiadny stav, dátum ani vlastníka | čas zápisu | podľa úložiska | cesty, stav, dátum, autora, commity |
| Keď zastará | zostane, kým si to niekto nevšimne | model ho môže prepísať | model ho môže prepísať | označí sa po 20 commitoch na jeho cestách |
| Funguje s | Claude Code; ostatní čítajú AGENTS.md | Claude Code | ich pluginom, SDK alebo MCP serverom | každým MCP klientom a CLI |

- **Pravidlá s cestami.** `.claude/rules/` v Claude Code vie obmedziť pravidlo na cesty. Pravidlo však stále nemá stav, vlastníka ani históriu a ostatní agenti ho nečítajú.
- **Priečinok s ADR.** Nechaj ho. kontext číta existujúce ADR tam, kde sú, a dáva ich agentom podľa ciest: pozri [Zavedenie do existujúceho repozitára](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).
- **Wiki.** Žije mimo repozitára, takže sa neposudzuje s kódom, ktorý opisuje, a agent nevie, ktorá stránka ešte platí.

kontext CLAUDE.md nenahrádza. Nechaj v ňom pár pokynov, ktoré potrebuje každá session. Rozhodnutí je priveľa na to, aby sa načítavali celé, a sú pridôležité na to, aby zostali neposúdené; `kontext connect claude-hooks` dá brief na začiatok každej session Claude Code.

## Pod tvojou kontrolou

- **Schvaľovanie.** Znalosti sa menia len cez commity. S `/.ai/ @acme/architects` v CODEOWNERS a povinným review od vlastníkov kódu žiadny agent nezmení to, čím sa riadia všetci agenti, bez súhlasu tohto tímu.
- **Auditná stopa.** `git log -- .ai` a `kontext log` ukážu, kto čo rozhodol a kedy. `git log --grep "Decision: <id>"` nájde commit, ktorý rozhodnutie priniesol.
- **Pravidlá v CI.** `kontext check` zastaví pull request s chybným záznamom, duplicitným id alebo uniknutým tajným údajom: pozri [Validácia znalostí v CI](../guides/07-ci.md).
- **Dáta zostávajú lokálne.** Žiadny účet ani server. Nič neopustí počítač, kým to nenastavíš, a adaptéry deklarované v repozitári sa spustia až po `kontext trust`: pozri [Bezpečnosť a súkromie](../concepts/08-security.md).
- **Bez uzamknutia.** Úložisko je obyčajný Markdown a dá sa čítať aj bez kontextu. Keď kontext odstrániš, súbory zostanú.

## Ako vyzerá deň s kontextom

1. Agent začne úlohu a zavolá `ctx_brief` s cestami, na ktoré siahne. Dostane rozhodnutia a úskalia, ktoré pre ne platia.
2. Počas práce volá `ctx_search`, `ctx_read` a `ctx_why` namiesto toho, aby históriu objavoval znova.
3. Keď sa ustáli niečo trvalé (rozhodnutie, konvencia, úskalie), zavolá `ctx_capture`.
4. Pred commitom zavolá `ctx_prepare_commit`, ktorý povýši relevantných kandidátov do zmeny. Hook ich zvaliduje, preskenuje na tajné údaje a doplní trailery `Decision: <id>`.
5. Pull request nesie kód aj znalosti spolu. Po merge dostanú agenti všetkých kolegov nové rozhodnutie v najbližšom briefe.

## Princípy

- **Git + Markdown je zdroj pravdy; všetko ostatné je odvodené.** Vyhľadávací index, cache, snapshoty synchronizácie a outbox udalostí sa dajú kedykoľvek zmazať a znova sa zostavia zo súborov a histórie.
- **Agenti nikdy sami nemenia zdieľanú pravdu.** Zachytenie skončí v lokálnom inboxe. Znalosťou tímu sa stane, až keď ho povýšiš *do commitu*. Vtedy sa objaví v diffe a posudzuje sa spolu s kódom, ktorý vysvetľuje.
- **Stručné je lepšie než úplné.** Každá odpoveď pre agenta má tokenový rozpočet. Detaily sú dostupné na požiadanie cez úrovne (L0 jeden riadok, L1 prehľad, L2 celé).
- **Hooky nikdy nečakajú na sieť.** Commity zostávajú okamžité.
- **Všetko ostatné je voliteľné.** Archívy sessions, grafy kódu a sémantickú pamäť možno pripojiť ako [adaptéry](../adapters/01-overview.md), každý pár riadkov konfigurácie. kontext funguje aj bez nich.

## Kam ďalej

- [Nainštaluj kontext](./02-installation.md)
- [Rýchly štart](./03-quickstart.md): zavedenie do repozitára za päť minút
- [Architektúra](../concepts/01-architecture.md): ako do seba časti zapadajú
