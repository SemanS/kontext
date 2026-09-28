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

Index sa pred dopytom obnovuje inkrementálne (záznamy a dokumenty podľa času zmeny a veľkosti, commity podľa SHA) – najviac raz za sekundu v jednom procese. `kontext reindex` ho zostaví od nuly.

### Identifikátory

Slová v tvare kódu sa rozkladajú, aby sa našli obe podoby: `orderTotal` sa indexuje aj ako `order total`, `user_account_balances` ako `user account balances`. Dopyt na identifikátor navyše hľadá jeho slová ako *frázu*, takže `RunContext` nájde „run context“ bez toho, aby zasiahol každý dokument so slovom „run“.

### Poradie

BM25 nad titulkom (váha 2,5), pomocným textom s rozloženými identifikátormi (1,5) a telom. Lokálne výsledky majú váhu 1,0; adaptéry predvolene 0,9 (`weight` pri každom adaptéri).

## Federované vyhľadávanie

`ctx_search` pošle dopyt paralelne každému adaptéru s operáciou `search`, s limitom 12 sekúnd. Výsledky sa normalizujú pre každý zdroj (najlepší výsledok každého zdroja má skóre 1,0), zvážia, zlúčia a zbavia duplicít podľa URI a takmer rovnakého titulku/úryvku. Pomalý alebo zlyhávajúci adaptér nikdy nezablokuje odpoveď: objaví sa v poznámke a na 30 sekúnd si dá pauzu.

Vyhľadávanie zúžiš cez `kinds` (`decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit`) a `sources` (`local` a/alebo názvy adaptérov).

## `ctx_why`

„Prečo je to takto?“ kombinuje viac signálov podľa cieľa:

| Cieľ | Odpoveď |
| --- | --- |
| `path` alebo adresár | znalosti, ktorých `paths` ho pokrývajú, zhrnutie modulu, commity v tvare rozhodnutí spomedzi posledných 60, ktoré sa ho dotkli (plus tri najnovšie), adaptéry `history` |
| `path:line` / `path:start-end` | to isté plus commity, ktoré tieto riadky naposledy zmenili (`git blame`) |
| SHA commitu | správa, znalosti, s ktorými súvisí (cez `commits` alebo `paths`), zmenené súbory |
| symbol alebo téma | adaptéry `code` ho nájdu (CodeGraph, Serena), potom lokálne znalosti a história, potom adaptéry `history` |

Commity sa zoraďujú podľa signálov rozhodnutí – slová ako *replace*, *migrate*, *instead of*, *because*; typ Conventional Commits; breaking changes; dĺžka vysvetlenia – a podľa trailerov kontextu (`Decision: …`).

## `ctx_log`

Časová os rozhodnutí: dátum, ID, titulok, stav, nahradenie a commit, ktorý súbor pridal. Pre skripty `kontext log --json`.
