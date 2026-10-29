# Vyhľadávanie

kontext vyhľadáva lokálne cez BM25 a pridá k tomu všetko, čo vrátia pripojené adaptéry.

## Lokálny index

Index [Tantivy](https://github.com/quickwit-oss/tantivy) pre každý worktree, odvodený z troch zdrojov:

| Zdroj | Dokumenty | URI |
| --- | --- | --- |
| Záznamy úložiska | jeden na záznam: titulok, telo, štítky, cesty, ID | `kx:<id>` |
| Dokumenty v repozitári | Markdown zodpovedajúci `sources.include` (`*.md` v koreni, každý `README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/**`), rozdelený na nadpisoch `##` | `file:<path>` alebo `file:<path>#<section>` |
| História commitov | predmet, telo a trailery posledných `index.max_commits` commitov (5000); názvy súborov sa dajú vyhľadať, ale do úryvkov sa nedostanú | `git:<sha>` |

Šum sa odfiltruje: commity „Merge branch …“, riadky `Co-authored-by`/`Signed-off-by`, súbory väčšie ako `sources.max_bytes` a všetko, čo vyzerá ako súbor s tajnými údajmi.

Index sa pred dopytom obnovuje inkrementálne (záznamy a dokumenty podľa času zmeny a veľkosti, commity podľa SHA), najviac raz za sekundu v jednom procese. Keď ho obnovil iný proces (capture z CLI, `kontext status`, druhá session agenta v tom istom worktree), dlhobežiaci MCP server zmenený manifest zbadá a pred odpoveďou znova načíta čítač. Nová verzia schémy sa stavia v dočasnom adresári a premenuje na miesto, nikdy zmazaním adresára, ktorý stavia iný proces. `kontext reindex` ho zostaví od nuly.

### Identifikátory

Slová v tvare kódu sa rozkladajú, aby sa našli obe podoby: `orderTotal` sa indexuje aj ako `order total`, `user_account_balances` ako `user account balances`. Dopyt na identifikátor navyše hľadá jeho slová ako *frázu*, takže `RunContext` nájde „run context“ bez toho, aby zasiahol každý dokument so slovom „run“.

### Skladanie a stemming

Dve tieňové polia, `title_f` a `body_f`, držia ten istý text malými písmenami, bez diakritiky (ASCII folding) a prehnaný anglickým stemmerom. Prehľadávajú sa vedľa presných polí s nižším boostom, takže `namietka` nájde `námietka`, `adapter` nájde „Adapters are configuration“ a presný identifikátor je stále prvý. Stemmer pre slovenčinu nie je; folding pokryje diakritiku, nie skloňovanie.

### Poradie

BM25 nad titulkom (váha 2,5), pomocným textom s rozloženými identifikátormi (1,5), telom a skladanými tieňovými poliami s nižšou váhou. Celý dopyt sa počíta aj ako fráza (váha 2): záznam s presne takým znením ide prvý, no fráza sama nepridá žiadny výsledok. Lokálne výsledky majú váhu 1,0; adaptéry predvolene 0,9 (`weight` pri každom adaptéri).

V rámci lokálneho zdroja má každý zdroj pred spojením skupín váhu: záznamy 1.0, kúsky dokumentácie 0.7, commity 0.6, záznamy vnoreného úložiska 0.95, takže reviewovaný záznam je pri rovnakom poradí nad kúskom dokumentácie alebo commitom. Váhy sú vstavané (kľúče `[search]` pre ne sú plánované) a nastavené na graded eval sade (`tests/eval/qrels.tsv`, nDCG@5, recall@10, MRR); zmena poradia potrebuje toto meranie. Nahradený, vyradený alebo zamietnutý výsledok dostane polovičnú váhu, nikdy sa neskryje, a jeho stav ukáže nástupcu: `superseded · → superseded by [<id>] <Title>`. Frázový bonus sa ráta z vyčistených slov dopytu, bez rozložených identifikátorov. Dopyt, ktorý je presne id záznamu alebo cesta, ide skratkou priamo na ten záznam.

## Federované vyhľadávanie

`ctx_search` pošle dopyt paralelne každému adaptéru s operáciou `search`, s limitom 12 sekúnd. S [`[search] adapters = "fallback"`](../reference/03-configuration.md#search) tak urobí, iba keď lokálne zdroje našli menej ako `min_local_hits` zásahov, alebo keď volajúci adaptér pomenuje či pošle `adapters: true`; predvolené `always` sa ich pýta zakaždým. Zdroje sa spájajú váženým recipročným poradím (každý výsledok má skóre `weight / (8 + rank)`), takže škála skóre žiadneho zdroja (tu BM25, tam kosínusová podobnosť) nevytlačí ostatné a jediný zdroj si zachová vlastné poradie.

Vypadnú iba preukázané kópie: zásah s už videným URI, kópia lokálneho záznamu z adaptéra (synchronizované `…/<id>.md`) a zásah, ktorého celý normalizovaný titulok a text sa zhodujú s predošlým. Z dvoch takých kópií nikdy neostane nahradená nad aktívnym nástupcom. Výsledky adaptérov sa radia podľa mapovaného skóre, nie podľa poradia skupín v odpovedi; výsledky z vnoreného úložiska nesú jeho značky. Dve tvrdenia s rovnakým začiatkom nie sú to isté tvrdenie: zásah, ktorý iba začína ako predošlý, ostane a ukáže sa pod ním s časťou, ktorou sa líši (`↳ similar, differs: …`). Pomalý alebo zlyhávajúci adaptér nikdy nezablokuje odpoveď: objaví sa v poznámke a na 30 sekúnd si dá pauzu.

Vyhľadávanie zúžiš cez `kinds` (`decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit`) a `sources` (`local` a/alebo názvy adaptérov).

## `ctx_why`

„Prečo je to takto?“ kombinuje viac signálov podľa cieľa:

| Cieľ | Odpoveď |
| --- | --- |
| `path` alebo adresár | znalosti, ktorých `paths` ho pokrývajú, zhrnutie modulu, commity v tvare rozhodnutí spomedzi posledných 60, ktoré sa ho dotkli (plus tri najnovšie), adaptéry `history` |
| `path:line` / `path:start-end` | to isté plus commity, ktoré tieto riadky naposledy zmenili (`git blame`) |
| SHA commitu | správa, znalosti, s ktorými súvisí (cez `commits` alebo `paths`), zmenené súbory |
| symbol alebo téma | adaptéry `code` ho nájdu (CodeGraph, Serena), potom lokálne znalosti a história, potom adaptéry `history` |

Commity sa zoraďujú podľa signálov rozhodnutí (slová ako *replace*, *migrate*, *instead of*, *because*; typ Conventional Commits; breaking changes; dĺžka vysvetlenia) a podľa trailerov kontextu (`Decision: …`).

## `ctx_log`

Časová os rozhodnutí: dátum, ID, titulok, stav, nahradenie a commit, ktorý súbor pridal. Pre skripty `kontext log --json`.
