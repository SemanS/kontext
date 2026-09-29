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
| Z adaptérov | sekcie od adaptérov s operáciou `brief` (napr. primer zo sessions) |
| Stav | počty v inboxe, postup zavádzania, varovania |

`focus` prijíma cesty (súbory alebo adresáre) a slová k téme. Záznamy, ktorých `paths` sa prekrývajú s cestou vo focuse, idú prvé; slová k téme sa porovnávajú s titulkami, zhrnutiami a štítkami.

Claude Code môže dostať brief automaticky na začiatku session. Pozri [Claude Code](../agent-integrations/02-claude-code.md).

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
| `kx:<id>` | záznam znalostí (`kx:0007`, `kx:2026-09-28-prices-are-integer-cents`) |
| `file:<path>` / `file:<path>#<section>` | súbor v repozitári alebo jednu sekciu Markdown dokumentu |
| `git:<sha>` | commit (L1 = správa a štatistika, L2 = celý diff) |
| `inbox:<id>` | lokálny kandidát |
| `viking://…`, … | čokoľvek, o čom adaptér deklaruje, že to vie prečítať (`owns`) |

Čítanie cez `file:` je obmedzené na repozitár a nikdy nevráti súbory s tajnými údajmi (`.env*`, kľúče, tfvars, …).

## Rozpočty inde

- `ctx_search` vypíše výsledky ako jednotlivé riadky v rámci `budget_tokens` (predvolene 1200) a povie agentovi, koľko ich odrezal.
- `ctx_why` drží každú sekciu krátku: najviac `limit` položiek na sekciu.
- Sekcie adaptérov v briefe dostanú každá najviac tretinu zvyšného rozpočtu.

Počty tokenov sa odhadujú na štyri znaky na token, zámerne jednoducho a konzervatívne.
