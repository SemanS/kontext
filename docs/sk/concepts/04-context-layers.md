# Vrstvy kontextu a rozpočty

Agenti platia za každý prečítaný token. kontext odpovedá v malých, vrstvených častiach a necháva na agentovi, kedy si zaplatí za viac.

## Brief

`ctx_brief` (alebo `kontext brief`) je vstupný bod, jeden blok s približne 1–2k tokenmi (`brief.budget_tokens`, predvolene 1400):

| Sekcia | Obsah |
| --- | --- |
| Hlavička | projekt, ID repozitára, vetva, *About* (zhrnutie prehľadu), *Stack* |
| Pravidlá pre agentov | `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, inštrukcie pre Copilot a vnorené súbory `AGENTS.md` na cestách vo focuse |
| Rozhodnutia | aktívne rozhodnutia, najrelevantnejšie pre focus ako prvé, každé na jeden riadok (najviac `brief.max_decisions`) |
| Konvencie · Poznatky a úskalia · Incidenty | každé na jeden riadok, zoradené podľa focusu |
| Moduly | zhrnutia modulov (najviac `brief.max_modules`); pri moduloch vo focuse aj začiatok ich prehľadu |
| Submoduly | sekcia pre submodul s vlastnými znalosťami, keď v ňom ležia cesty z focusu alebo keď tento repozitár žiadne nemá; inak jednoriadkový odkaz |
| Lokálne poznámky | poznámky v inboxe tohto klonu (tímoví kandidáti aj súkromné poznámky), ktoré zodpovedajú focusu, inak tri najnovšie |
| Súvisiace dokumenty a história | v repozitári bez úložiska, keď už vyhľadávanie postavilo lokálny index: dokumenty a commity, ktoré zodpovedajú focusu |
| Z adaptérov | sekcie od adaptérov s operáciou `brief` (napr. primer zo sessions) |
| Stav | počty v inboxe, postup zavádzania, rozhodnutia na prehodnotenie, chýbajúce git hooky, varovania; miesto pre ňu je vyhradené, takže ju plný brief nevytlačí |

`focus` prijíma cesty (súbory alebo adresáre) a slová k téme. Záznamy, ktorých `paths` sa prekrývajú s cestou vo focuse, idú prvé; slová k téme sa porovnávajú s titulkami, zhrnutiami a štítkami. Keď všetky cesty vo focuse ležia v inom repozitári, brief je briefom toho repozitára ([Viac repozitárov](09-several-repositories.md)).

Na záznam sa odkazuje najkratšou jednoznačnou predponou jeho id (`[2026-09-28-adapters]` namiesto id, ktoré opakuje celý titulok), alebo číslom ADR. `ctx_read` rozpozná oboje.

Claude Code môže dostať brief automaticky na začiatku session. Pozri [Claude Code](../agent-integrations/02-claude-code.md).

### Aktuálnosť {#freshness}

Rozhodnutie sa zapíše raz, ale kód, ktorý riadi, sa ďalej mení. *Stav* v briefe a `kontext status` vypíšu aktívne rozhodnutia, ktorých `paths` sa od dátumu rozhodnutia dotklo aspoň `freshness.threshold_commits` commitov (predvolene 20), najviac zmenené ako prvé:

```text
- May need a refresh (20+ commits on their paths since they were made): [0007] Prices are integer cents (34 commits), … — check they still hold; supersede what no longer does.
```

Commity z dňa samotného rozhodnutia a merge commity sa nepočítajú. Commit sa počíta raz, nech sa zhoduje koľkokoľvek jeho súborov, a započíta sa aj presun kódu mimo riadenej cesty. Rozhodnutia bez `date` alebo `paths` sa neoznačia nikdy. Označenie je výzva na kontrolu, nie verdikt: ak rozhodnutie stále platí, netreba nič meniť; ak nie, nahraď ho novým (supersede).

Počty pochádzajú z jedného `git log` pre všetky rozhodnutia, od dátumu najstaršieho rozhodnutia (obmedzeného `index.max_commits`), a ukladajú sa do cache podľa `HEAD` v stavovom adresári worktree: brief nepridá žiadne volanie gitu na rozhodnutie a do ďalšieho commitu žiadne. `threshold_commits = 0` to vypne.

## Úrovne: L0 · L1 · L2

Každý výsledok nesie URI. `ctx_read` ho vráti v úrovni, o ktorú agent požiada:

| Úroveň | Veľkosť | Použitie |
| --- | --- | --- |
| 0 | jeden riadok | je to relevantné? |
| 1 | do ~4k znakov | väčšinou stačí na to, aby agent mohol konať (predvolené pre `ctx_read`) |
| 2 | celý obsah (najviac 80k znakov) | keď záleží na detailoch |

## URI

| URI | Ukazuje na |
| --- | --- |
| `kx:<id>` | záznam znalostí (`kx:0007`, `kx:2026-09-28-prices-are-integer-cents` alebo jednoznačná predpona ako `kx:2026-09-28-prices`) |
| `kx:<submodul>/<id>` | záznam z úložiska submodulu (`git:<submodul>/<sha>` pre jeho commity) |
| `file:<path>` / `file:<path>#<section>` | súbor v repozitári alebo jednu sekciu Markdown dokumentu |
| `git:<sha>` | commit (L1 = správa a štatistika, L2 = celý diff) |
| `inbox:<id>` | lokálny kandidát |
| `viking://…`, … | čokoľvek, o čom adaptér deklaruje, že to vie prečítať (`owns`) |

Čítanie cez `file:` je obmedzené na repozitár a nikdy nevráti súbory s tajnými údajmi (`.env*`, kľúče, tfvars, …).

## Rozpočty inde

- `ctx_search` vypíše výsledky ako jednotlivé riadky v rámci `budget_tokens` (predvolene 1200) a povie agentovi, koľko ich odrezal.
- `ctx_why` drží každú sekciu krátku: najviac `limit` položiek na sekciu.
- `ctx_threads` vracia prepis po častiach veľkých `budget_chars` (predvolene 20000), aby sa každá zmestila do jedného výsledku nástroja.
- Sekcie adaptérov v briefe dostanú každá najviac tretinu zvyšného rozpočtu.

Počty tokenov sa odhadujú na štyri znaky na token, zámerne jednoducho a konzervatívne.
