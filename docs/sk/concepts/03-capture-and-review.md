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

Zachytenie potrebuje druh, titulok a telo (alebo zhrnutie). Dobré zachytenia sú krátke (*čo, prečo, dôsledky*) a uvádzajú cesty, ktorých sa týkajú:

```sh
kontext capture --kind decision --title "Use Tantivy for local recall" \
  --paths "src/index.rs" --body "BM25 is enough for code-shaped queries; semantic search comes from adapters."
```

Agenti volajú `ctx_capture` s rovnakými poliami. Pri zachytení kontext:

- zamaskuje hodnoty, ktoré vyzerajú ako tajné údaje (`capture.redact = true`),
- upozorní, keď už podobný záznam existuje (aby sa radšej aktualizoval),
- rozhodnutiam predvolene nastaví `status: accepted` a zapíše autora (`git config user.name`) a pôvod (`agent` alebo `cli`),
- zaradí udalosť `capture` pre adaptéry, ktoré ju odoberajú,
- založí ho v repozitári, ktorému patria jeho `paths`, napríklad v submodule s vlastným úložiskom ([Viac repozitárov](09-several-repositories.md)).

## Viditeľnosť

| Viditeľnosť | Zostáva | Dá sa povýšiť | Typické použitie |
| --- | --- | --- | --- |
| `team` (predvolené) | v inboxe, kým ho nepovýšiš | áno | rozhodnutia, konvencie, úskalia, ktoré má tím poznať |
| `private` | v inboxe a v osobných adaptéroch | nie | vlastné poznámky, nedokončené hypotézy, preferencie |

Súkromné zachytenia sa nikdy nedostanú do repozitára. Adaptéry ich môžu prijímať (napríklad ako pamäte v OpenVikingu), ak odoberajú `capture` s `visibility = "private"`.

Poznámky v inboxe, tímové aj súkromné, sa dajú nájsť: `ctx_search` ich nájde (označené *local note*) a brief uvedie tie, ktoré zodpovedajú jeho focusu. Nájdenie poznámku nepovýši: commit stále berie len kandidátov, ktorých zachytil jeho vlastný worktree.

## Povyšovanie

Povýšenie zapíše kandidáta do úložiska (so správnym ID, adresárom, štýlom a číslovaním) a stagne ho:

- `ctx_prepare_commit` s `promote=[ids]` (to používajú agenti pred commitom),
- `ctx_inbox` s `action=promote`,
- `kontext promote <id>…` alebo `kontext prepare-commit --promote <id>`.

`capture --promote` (alebo `promote=true` v `ctx_capture`) inbox preskočí a zapíše záznam priamo do pracovného stromu, čo sa hodí, keď už vieš, že patrí k aktuálnej zmene. Tak či tak sa nič nezdieľa, kým necommitneš.

## Pred commitom: `ctx_prepare_commit` {#before-committing-ctx-prepare-commit}

Report vypíše:

1. stagnutú zmenu, alebo, keď nie je nič stagnuté, zmeny pracovného stromu voči `HEAD` (vrátane nesledovaných súborov) — správa povie, čo čítala,
2. zapísané znalosti, ktorých `paths` pokrývajú zmenené súbory (*over, že stále platia*: `ctx_confirm` / `kontext confirm <id>`),
3. námietky z tohto worktree a dva spôsoby, ako ich vybaviť ([nižšie](#challenges)),
4. kandidátov z inboxu, najprv tých, ktorí sa týkajú zmenených súborov,
5. výsledky validácie a skenovania tajných údajov pre stagnuté znalosti (chyby commit zablokujú),
6. dokumenty modulov, ktoré možno treba obnoviť (veľa zmenených súborov alebo pridané/odobrané súbory),
7. trailery commitu a či ich pridá hook prepare-commit-msg (klon bez hookov kontextu ich potrebuje dopísať ručne, alebo `kontext hooks install`),
8. pripomienku, keď veľká zmena nenesie vôbec žiadne znalosti.

Zachytenie tiež pomenuje zaznamenané záznamy, ktoré môže duplikovať, rozširovať alebo im protirečiť (`related, check: … <kx:id> — governs the same paths (src/pricing/**)`). Je to výzva pozrieť sa, nikdy pokyn nahradiť: či nový záznam niečo nahrádza, rozhoduje autor a reviewer.

## Drift: znalosti, ktoré zmena nechala za sebou {#drift}

`ctx_prepare_commit` sa pozerá na jednu zmenu. `kontext drift` (`ctx_drift`, `entries.drift`) sa pozerá na rozsah: vypíše aktívne záznamy, ktorých `paths` pokrývajú súbory zmenené v `base..head` a ktorých vlastný súbor sa v tom istom rozsahu **nezmenil**. Úprava záznamu, jeho vyradenie aj [potvrdenie](./02-knowledge-store.md#checked) sa rátajú ako pohľad naň.

```text
$ kontext drift
Knowledge drift in 3f2a91c4..working tree: 1 entry governs changed files but was not edited or confirmed in the range.

- [convention] Prices are integer cents <kx:prices-are-integer-cents> — .ai/conventions/prices-are-integer-cents.md
  changed: src/billing/invoice.rs, src/billing/tax.rs
  commits: 9c1d2e3f feat(billing): VAT per line

For each, read it against the change: if it still holds, confirm it (`kontext confirm <id>`); if it needs a correction, edit it; if it no longer holds, supersede it (`kontext capture --supersedes <id> …`) or retire it (`kontext challenge <id> "why"`, then `kontext prepare-commit --retire <id>`).
```

- `--base` je predvolene miesto, kde `HEAD` opustil predvolenú vetvu remote (merge base s `origin/HEAD`, inak `origin/main`, inak `origin/master`), a bez remote `HEAD`.
- Bez `--head` sa počíta aj pracovný strom: commity `base..HEAD` plus necommitnuté a nesledované zmeny. Presne tak vidí git pracovnú kópiu [jj](./07-git-integration.md#jj), takže drift funguje bez commitov aj bez hookov. S `--head` sa počítajú len commitnuté zmeny a záznamy sa čítajú tak, ako sú commitnuté tam.
- Kontrolujú sa rozhodnutia, konvencie, poučenia a incidenty (výber cez `--kind`); dokumenty modulov obnovuje `kontext init`, nepotvrdzujú sa.
- Nezúčastňuje sa žiadny model: odpoveď je deterministická funkcia repozitára. `--json` vypíše výsledok `entries.drift`; `--fail` skončí so stavom 2, keď je vypísaný nejaký záznam. V CI ho spúšťa [`kontext ci`](../guides/07-ci.md).

## Námietky {#challenges}

Niekedy agent zistí, že zaznamenaná znalosť už neplatí. Povie to cez `ctx_challenge <id> "prečo"` (alebo `kontext challenge`): lokálny kandidát v inboxe druhu `challenge`, voči záznamu a commitu, v ktorom bol prečítaný. Námietka sama nič nemení a povýšiť sa nedá. Brief označí záznam `challenged locally` a `ctx_prepare_commit` ponúkne dva spôsoby, ako ju vybaviť v zmene na review:

- **vyradiť** záznam (`retire=[id]`): jeho súbor dostane stav `deprecated` a poznámku `## Retired` s námietkou ako dôvodom, stagnuté;
- **nahradiť** ho: zachyť nové pravidlo so `supersedes: [id]` a povýš ho.

Oboje vezme námietku z inboxu spolu s commitom. Ak sa záznam od námietky zmenil, report to povie; námietku, ktorá sa ukázala ako mylná, zahoď. Námietka je vybavená, až keď zmena prejde review a merge, nie keď ju jeden worktree pripravil. Zdieľané námietky s počítaním nezávislých hlasov patria aplikácii, napríklad Peer Hubu; kontext drží lokálnu, posúdenú časť.

## Review

Znalosti cestujú v tom istom pull requeste ako kód. Posudzovatelia vidia rozhodnutie vedľa diffu, ktorý vysvetľuje; môžu žiadať zmeny, zamietnuť ho (zmazať súbor alebo nastaviť `status: rejected`) alebo neskôr navrhnúť rozhodnutie, ktoré ho nahradí. `git log --format='%(trailers:key=Decision,valueonly)'` a `kontext log` ukážu, ktorý commit zaviedol ktoré rozhodnutie.

## Po merge

Keď kolegovia pullnú, ich hooky `post-merge` / `post-rewrite` porovnajú znalosti v `HEAD` s ich posledným snapshotom a vyšlú udalosti `sync`. Ich osobné systémy pamäte sa tak dozvedia rozhodnutia, ktoré tím práve prijal. Povýšené položky inboxu sa odstránia, keď je ich súbor súčasťou `HEAD`.
