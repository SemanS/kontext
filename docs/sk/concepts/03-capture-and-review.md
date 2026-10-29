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

## Zachytávanie {#capturing}

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
- založí ho v repozitári, ktorému patria jeho `paths`, napríklad v submodule s vlastným úložiskom ([Viac repozitárov](09-several-repositories.md)),
- zachová `supersedes` a `extends` ako plné ID cez inbox aj povýšenie; nejednoznačný alebo neznámy cieľ sa odmietne.

Poznatok, ktorý stavia na rozhodnutí, sa zachytí ako nový záznam s `extends: [<id>]` (`--extends` v CLI); samotné rozhodnutie sa nikdy nedopĺňa. Konvencia, poznatok alebo incident môže namiesto toho dostať datovanú sekciu `## Added` (pozri [Dodatky len pridávajú](./02-knowledge-store.md#amendments)).

Poučenie alebo rozhodnutie, ktoré si dokázal príkazom, nesie ten príkaz, aby sa dalo neskôr [dokázať znova](#verify):

```sh
kontext capture --kind learning --title "Totals round per line, not per invoice" \
  --paths "src/billing/**" --body "…" --verify "cargo test -p billing rounding_is_per_line"
```

`--verify` (`verify` v `ctx_capture` a `drafts.capture`) sa overuje ako pole záznamu: jeden riadok, najviac 500 znakov; iné sa odmietne (v API `invalid_input`). Návrh ho zachová a rovnako záznam, do ktorého sa povýši, v oboch štýloch súborov. Návrh zachytený bez neho ho dostane neskôr: `kontext inbox verify <id> "<príkaz>"` (`ctx_inbox` s `action=verify`, operácia `inbox.verify`); `--clear` (prázdny reťazec) ho odstráni.

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

`ctx_prepare_commit` sa pozerá na jednu zmenu. `kontext drift` (`ctx_drift`, `entries.drift`) sa pozerá na rozsah: vypíše aktívne záznamy, ktorých `paths` pokrývajú súbory zmenené v `base..head` a ktorých vlastný súbor sa v tom istom rozsahu **nezmenil**. Úprava záznamu, jeho vyradenie, [potvrdenie](./02-knowledge-store.md#checked) aj prejdená kontrola sa rátajú ako pohľad naň, ale len keď je ten pohľad v poslednom commite rozsahu, ktorý zmenil jeho riadené súbory, alebo po ňom: pohľad pred zmenou ju nepokrýva. Zlyhaná kontrola nie je pohľad.

```text
$ kontext drift
Knowledge drift in 3f2a91c4..working tree: 1 entry governs changed files but was not edited or confirmed in the range.

- [convention] Prices are integer cents <kx:prices-are-integer-cents> — .ai/conventions/prices-are-integer-cents.md
  changed: src/billing/invoice.rs, src/billing/tax.rs
  commits: 9c1d2e3f feat(billing): VAT per line

For each, read it against the change: if it still holds, confirm it (`kontext confirm <id>`); if it needs a correction, edit it; if it no longer holds, supersede it (`kontext capture --supersedes <id> …`) or retire it (`kontext challenge <id> "why"`, then `kontext prepare-commit --retire <id>`).
```

- `--base` je predvolene miesto, kde `HEAD` opustil predvolenú vetvu remote (merge base s `origin/HEAD`, inak `origin/main`, inak `origin/master`, inak upstream alebo predvolená vetva `checkout.defaultRemote`), a bez remote `HEAD`; vtedy `--fail` skončí kódom 3.
- Bez `--head` sa počíta aj pracovný strom: commity `base..HEAD` plus necommitnuté a nesledované zmeny. Presne tak vidí git pracovnú kópiu [jj](./07-git-integration.md#jj), takže drift funguje bez commitov aj bez hookov. S `--head` sa počítajú len commitnuté zmeny a záznamy sa čítajú tak, ako sú commitnuté tam.
- Kontrolujú sa rozhodnutia, konvencie, poučenia a incidenty (výber cez `--kind`); dokumenty modulov obnovuje `kontext init`, nepotvrdzujú sa.
- Nezúčastňuje sa žiadny model: odpoveď je deterministická funkcia repozitára. `--json` vypíše výsledok `entries.drift`; `--fail` skončí so stavom 2, keď je vypísaný nejaký záznam. Bez základu (žiadny remote, žiadny merge base, žiadne `--base`) to drift povie namiesto hlásenia, že drift nie je, a `--fail` skončí nenulou. V CI ho spúšťa [`kontext ci`](../guides/07-ci.md).
- **Bránou je skript alebo CI, nie hook.** Tím, ktorý commituje cez jj alebo cez klientov, čo git hooky obchádzajú, nemá pri commite žiadnu kontrolu, preto brána patrí do tímového kontrolného skriptu (`bun run check`, `make check`) alebo do CI: `kontext drift --fail` (alebo `kontext ci --strict`). kontext nepridáva hook, ktorý by commit pre drift zablokoval.

## Overenia: denník {#verifications}

Dátum `checked:` v súbore záznamu hovorí, že ho niekto potvrdil, nie však, na čo sa pozeral a ako; a merge, ktorý v ňom vyrieši konflikt staršou stranou, novší pohľad stratí. Preto sa každé potvrdenie aj každá spustiteľná kontrola pridá aj do **`.ai/confirmations.log`**, po jednom riadku oddelenom tabulátormi:

```text
2026-10-27T09:12:44Z	prices-are-integer-cents	read	a7450a5c…	Jana Nová	-	overené voči zmene DPH
2026-10-27T09:40:02Z	invoice-totals-tested	verify	a7450a5c…	Jana Nová	1	
```

Polia sú čas (UTC, na sekundu), ID záznamu, spôsob pohľadu, celý commit, na ktorý v tej chvíli ukazoval `HEAD` (`-` pred prvým commitom), kto (`git config user.name`), či príkaz `verify` prešiel (`1`/`0`, inak `-`) a voliteľný jednoriadkový dôvod. Denník sa len dopĺňa: žiaden riadok sa neprepisuje. `kontext init` zapíše do `.ai/.gitattributes` riadok `confirmations.log merge=union` (riadky, ktoré tam už sú, zostanú) a prvý zápis overí, že tam je, takže dve vetvy, ktoré potvrdzujú záznamy, sa zlúčia bez konfliktu a zachovajú oba riadky. Denník patrí do úložiska: drift ani aktuálnosť ho nepočítajú ako kód a `kontext check` ho nečíta ako záznam.

Spôsob pohľadu:

| Druh | Zapisuje | Platí ako overenie |
| --- | --- | --- |
| `read` | `kontext confirm` na termináli; `drafts.confirm` s `kind: read` (zapísané `by: application:<meno>`) | áno |
| `agent` | `ctx_confirm`; `kontext confirm` bez terminálu; `drafts.confirm` predvolene | nie: pečiatka, uchovaná a zobrazená ako taká |
| `bulk` | každé potvrdenie viac ako piatich záznamov jedným volaním; `kontext confirm --migrate-legacy` | nie: pečiatka |
| `verify` | `kontext verify`, `ctx_verify`, `drafts.verify` (len pinnuté príkazy) | keď príkaz prešiel |
| `cache` | nezapisuje sa: dátum `checked:` bez riadku v denníku | nie |

MCP nikdy nezapíše `read`. `kontext confirm --kind read` ho vynúti aj bez terminálu a varuje. Terminál nie je dôkaz, že záznam čítal človek; je to najlepší signál, aký príkazový riadok má, a denník zaznamená, ktorá cesta riadok zapísala.

`kontext confirm <id…> --because "…"` (`because` v nástroji aj v API) uloží dôvod. `checked:` v zázname sa zapisuje a stagne ako doteraz, aby staršie čítačky fungovali ďalej; je to vyrovnávacia kópia denníka, nikdy nie dôkaz. Keď kópia zaostáva za denníkom (merge vzal staršiu stranu alebo riadok prišiel z inej vetvy), `kontext check` varuje (`checked date … behind its ledger; run kontext confirm --repair`) a **`kontext confirm --repair`** prepíše `checked:` každého takého aktívneho záznamu podľa denníka a súbory stagne. Riadok `verify` nie je potvrdenie a kópiu nemení.

### Záznam, ktorý sa overí sám {#verify}

Záznam môže uviesť jeden shellový príkaz, ktorý ukáže, že stále platí:

```yaml
verify: cargo test -p billing totals_are_integer_cents
```

Jeden riadok, najviac 500 znakov (dlhšia kontrola patrí do skriptu, ktorý príkaz zavolá); iné `kontext check` odmietne. Príkaz je kód z repozitára, preto beží až po tom, čo jeho text pinneš z terminálu: `kontext trust --verify <id>` (alebo `--all`). Bez pinu `kontext verify <id…>` (`ctx_verify`, `drafts.verify`) nič nespustí a vráti `untrusted_verify` s príkazom; cez MCP ho môže agent spustiť vo vlastnom sandboxe a výsledok zapísať cez `ctx_confirm`. Pinnutý príkaz beží cez `sh -c` z koreňa repozitára so zatvoreným vstupom a povoleným zoznamom premenných prostredia, zastaví sa po 120 sekundách (mení to `KONTEXT_VERIFY_TIMEOUT_SECS`) a pridá riadok `verify` s výsledkom a commitom, voči ktorému bežal; necommitnuté zmeny záznamu alebo jeho riadených ciest uvedie dôvod riadku (`[dirty: <cesty>]`) a cez MCP sa taký príkaz namiesto spustenia vráti agentovi. Vráti, či prešiel, návratový kód a zamaskované posledné 2 KB výstupu. Čo pin pokrýva a čo nie, je v [Bezpečnosti](./08-security.md#verify). Príkaz nahradeného alebo vyradeného záznamu sa nikdy nespustí a záznam bez `verify:` je odmietnutý: prečítajte ho a potvrďte. Takto sa najlepšie overujú fakty, ktoré stroj vie zistiť; čo niekto zamýšľal, potvrdí človek.

### Čo sa odvtedy zmenilo {#since}

Pre každý záznam, ktorý riadi cesty z `focus`, pridá [`ctx_brief`](./04-context-layers.md), ako bol naposledy overený a čo sa odvtedy stalo s jeho cestami:

```text
- [prices-are-integer-cents] Prices are integer cents — Every amount is an integer number of cents.
  read by Jana Nová at a7450a5 (2026-10-27); 3 commits on its paths since: feat(billing): VAT per line; fix(billing): round totals; …
```

Prvé slová povedia, aký bol posledný pohľad, a „verified“ sa pri pečiatke nikdy nepoužije:

| Riadok | Význam |
| --- | --- |
| `read by <kto> at <commit>` | potvrdil ho človek (riadok `read`) |
| `check passed at <commit>` | jeho pinnutý príkaz `verify` prešiel |
| `stamped by agent at <commit>` | potvrdil ho agent: pohľad, nie overenie |
| `bulk stamp at <commit>` | potvrdený naraz s viac ako piatimi ďalšími |
| `checked <dátum> (cache)` | len dátum `checked:`, bez riadku v denníku |
| `CHECK FAILING at <commit>` | najnovší `verify` zlyhal; zobrazí sa vedľa posledného pohľadu, ktorý nenahrádza |
| `never verified` | žiadny pohľad |

Počíta sa podľa predkov: ne-merge commity na riadených súboroch záznamu (bez súborov znalostí), ktoré `HEAD` dosiahne a commit pohľadu nie. Zaráta sa aj vetva vyvinutá pred potvrdením a zlúčená po ňom. Commit pohľadu je ten, ktorý riadok denníka uviedol, alebo commit, ktorý riadok priniesol, ak je novší. Čas sa použije len vtedy, keď taký commit nie je. Keď má niektorý vypísaný záznam commity odvtedy, brief raz pridá: *Read these before changing their paths: an entry that limits the direction you are about to take outranks a fresh one.* Na rozdiel od aktuálnosti tu nie je prah: už jeden commit na riadenej ceste je dôvod čítať.

Strojové API vracia to isté pri každom zázname (`entries.list`, `entries.get`, `entries.governing`, `entries.drift`): `verification`, `since`, `verify` a `premises`. Pozri [aplikačné API](../reference/05-application-api.md#verification).

`verification.landed` hovorí, kde je overenie súčasťou histórie: celé ID commitu v histórii `HEAD`, ktorý ako prvý pridal najnovší riadok denníka pre záznam, alebo `null`, kým je ten riadok len stagnutý alebo v pracovnom strome. Odberateľ tak odlíši „kontrola prešla a je commitnutá“ od „prešla, ešte necommitnutá“ bez čítania denníka. Pri overení známom len z dátumu `checked:` je to posledný commit, ktorý zmenil súbor záznamu (`null`, kým má súbor necommitnuté zmeny alebo nebol nikdy commitnutý).

### Zmenené predpoklady {#premises}

Nahradenie alebo vyradenie záznamu nič nezmení v záznamoch, ktoré na ňom stavajú. Aktívny záznam, ktorý odkazuje na záznam teraz nahradený, vyradený alebo zastaraný — cez `[[id]]` v tele, cez `supersedes` alebo cez pole `extends` — má zmenený predpoklad: brief povie `premise changed: <id> is superseded`, `kontext check` vypíše informačný riadok (`<id> links to <old>, which is superseded by <new>: does it still hold?`) a API ho uvedie v `premises`. Záznam, ktorý nahrádzal, sa za to neoznačí. Je to otázka pre toho, kto záznam číta ďalší, nikdy nie automatická úprava: agenti k záznamu pridávajú alebo ho nahrádzajú, neprepisujú ho.

## Námietky {#challenges}

Niekedy agent zistí, že zaznamenaná znalosť už neplatí. Povie to cez `ctx_challenge <id> "prečo"` (alebo `kontext challenge`): lokálny kandidát v inboxe druhu `challenge`, voči záznamu a commitu, v ktorom bol prečítaný. Námietka sama nič nemení a povýšiť sa nedá. Brief označí záznam `challenged locally` a `ctx_prepare_commit` ponúkne dva spôsoby, ako ju vybaviť v zmene na review:

- **vyradiť** záznam (`retire=[id]`): jeho súbor dostane stav `deprecated` a poznámku `## Retired` s námietkou ako dôvodom, stagnuté;
- **nahradiť** ho: zachyť nové pravidlo so `supersedes: [id]` a povýš ho.

Oboje vezme námietku z inboxu spolu s commitom. Ak sa záznam od námietky zmenil, report to povie; námietku, ktorá sa ukázala ako mylná, zahoď. Námietka je vybavená, až keď zmena prejde review a merge, nie keď ju jeden worktree pripravil: ostáva v stave *resolving*, kým nový blob záznamu nepristane v `HEAD`, takže ju reset ani opustená vetva nestratia. Zdieľané námietky s počítaním nezávislých hlasov patria aplikácii, napríklad Peer Hubu; kontext drží lokálnu, posúdenú časť.

## Review

Znalosti cestujú v tom istom pull requeste ako kód. Posudzovatelia vidia rozhodnutie vedľa diffu, ktorý vysvetľuje; môžu žiadať zmeny, zamietnuť ho (zmazať súbor alebo nastaviť `status: rejected`) alebo neskôr navrhnúť rozhodnutie, ktoré ho nahradí. `git log --format='%(trailers:key=Decision,valueonly)'` a `kontext log` ukážu, ktorý commit zaviedol ktoré rozhodnutie.

## Po merge

Keď kolegovia pullnú, ich hooky `post-merge` / `post-rewrite` porovnajú znalosti v `HEAD` s ich posledným snapshotom a vyšlú udalosti `sync`. Ich osobné systémy pamäte sa tak dozvedia rozhodnutia, ktoré tím práve prijal. Povýšené položky inboxu sa odstránia, keď je ich súbor súčasťou `HEAD`.
