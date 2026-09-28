# Zachytávanie a review

kontext oddeľuje **zapamätanie si** od **zdieľania**. Zachytiť sa dá čokoľvek a rýchlo; znalosťou tímu sa stane len to, čo prejde commitom.

```text
 agent / you                    local (this clone)                     team (git)
─────────────                   ─────────────────                      ──────────
 ctx_capture ─────────────────▶ inbox candidate ──┐
 kontext capture                 (team | private)  │ promote (ctx_prepare_commit,
                                                   │  ctx_inbox, kontext promote)
                                                   ▼
                                 .ai/decisions/…md staged ──▶ commit ──▶ PR review ──▶ merge
                                                               │ Decision: <id> trailer
                                                               ▼
                                                 teammates pull → sync events → their adapters
```

## Zachytávanie

Zachytenie potrebuje druh, titulok a telo (alebo zhrnutie). Dobré zachytenia sú krátke – *čo, prečo, dôsledky* – a uvádzajú cesty, ktorých sa týkajú:

```sh
kontext capture --kind decision --title "Use Tantivy for local recall" \
  --paths "src/index.rs" --body "BM25 is enough for code-shaped queries; semantic search comes from adapters."
```

Agenti volajú `ctx_capture` s rovnakými poliami. Pri zachytení kontext:

- zamaskuje hodnoty, ktoré vyzerajú ako tajné údaje (`capture.redact = true`),
- upozorní, keď už podobný záznam existuje (aby sa radšej aktualizoval),
- rozhodnutiam predvolene nastaví `status: accepted` a zapíše autora (`git config user.name`) a pôvod (`agent` alebo `cli`),
- zaradí udalosť `capture` pre adaptéry, ktoré ju odoberajú.

## Viditeľnosť

| Viditeľnosť | Zostáva | Dá sa povýšiť | Typické použitie |
| --- | --- | --- | --- |
| `team` (predvolené) | v inboxe, kým ho nepovýšiš | áno | rozhodnutia, konvencie, úskalia, ktoré má tím poznať |
| `private` | v inboxe a v osobných adaptéroch | nie | vlastné poznámky, nedokončené hypotézy, preferencie |

Súkromné zachytenia sa nikdy nedostanú do repozitára. Adaptéry ich môžu prijímať (napríklad ako pamäte v OpenVikingu), ak odoberajú `capture` s `visibility = "private"`.

## Povyšovanie

Povýšenie zapíše kandidáta do úložiska – so správnym ID, adresárom, štýlom a číslovaním – a stagne ho:

- `ctx_prepare_commit` s `promote=[ids]` (to používajú agenti pred commitom),
- `ctx_inbox` s `action=promote`,
- `kontext promote <id>…` alebo `kontext prepare-commit --promote <id>`.

`capture --promote` (alebo `promote=true` v `ctx_capture`) inbox preskočí a zapíše záznam priamo do pracovného stromu – hodí sa, keď už vieš, že patrí k aktuálnej zmene. Tak či tak sa nič nezdieľa, kým necommitneš.

## Pred commitom: `ctx_prepare_commit` {#before-committing-ctx-prepare-commit}

Report vypíše:

1. stagnutú zmenu (alebo pracovný strom, keď nie je nič stagnuté),
2. zapísané znalosti, ktorých `paths` pokrývajú zmenené súbory – *over, že stále platia*,
3. kandidátov z inboxu, najprv tých, ktorí sa týkajú zmenených súborov,
4. výsledky validácie a skenovania tajných údajov pre stagnuté znalosti (chyby commit zablokujú),
5. dokumenty modulov, ktoré možno treba obnoviť (veľa zmenených súborov alebo pridané/odobrané súbory),
6. trailery, ktoré hook pridá,
7. pripomienku, keď veľká zmena nenesie vôbec žiadne znalosti.

## Review

Znalosti cestujú v tom istom pull requeste ako kód. Posudzovatelia vidia rozhodnutie vedľa diffu, ktorý vysvetľuje; môžu žiadať zmeny, zamietnuť ho (zmazať súbor alebo nastaviť `status: rejected`) alebo neskôr navrhnúť rozhodnutie, ktoré ho nahradí. `git log --format='%(trailers:key=Decision,valueonly)'` a `kontext log` ukážu, ktorý commit zaviedol ktoré rozhodnutie.

## Po merge

Keď kolegovia pullnú, ich hooky `post-merge` / `post-rewrite` porovnajú znalosti v `HEAD` s ich posledným snapshotom a vyšlú udalosti `sync` – ich osobné systémy pamäte sa tak dozvedia rozhodnutia, ktoré tím práve prijal. Povýšené položky inboxu sa odstránia, keď je ich súbor súčasťou `HEAD`.
