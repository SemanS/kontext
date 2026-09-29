# Tímový workflow

## Nasadenie

1. Jeden človek na vetve zavedie kontext do repozitára (`kontext init`, prehĺbenie, review) a otvorí pull request s `.ai/`, `.mcp.json` (`kontext connect claude --write`) a blokom v `AGENTS.md` (`kontext connect agents-md --write`).
2. Tím ho posúdi ako kód: chybné zhrnutia modulov alebo rozhodnutia je lacnejšie opraviť teraz.
3. Po merge si každý člen tímu nainštaluje kontext a v každom klone raz spustí `kontext hooks install` (ak sa hooky nezdieľajú cez verzovaný adresár s hookmi).

Členov tímu, ktorí kontext nemajú nainštalovaný, sa to nijako nedotkne: bloky v hookoch bez binárky nerobia nič a `.ai/` je obyčajný Markdown.

## Každodenný cyklus

| Krok | Agent | Človek |
| --- | --- | --- |
| Začiatok | `ctx_brief` s cestami, na ktoré sa sústreďuje | `kontext brief --focus <path>` |
| Pochopenie | `ctx_why`, `ctx_search`, `ctx_read` | `kontext why <path[:line]>`, `kontext search …` |
| Zaznamenanie | `ctx_capture` | `kontext capture …` |
| Pred commitom | `ctx_prepare_commit` (povýšenie relevantných kandidátov) | `kontext prepare-commit [--promote id]` |
| Commit | hooky validujú, obnovujú index, pridávajú trailery | rovnako |

Dobré návyky:

- zachytávaj vo chvíli, keď padne rozhodnutie, nie na konci týždňa (inbox nič nestojí),
- jedno rozhodnutie na záznam, každé na pár riadkov; na dlhšie texty odkazuj, nevkladaj ich,
- každému záznamu daj `paths`, vďaka tomu sa zobrazí pri správnej zmene.

## Review znalostí v pull requestoch

K záznamom pristupuj ako ku kódu:

- **Je to pravda a stále to tak chceme?** Agent mohol z jedného commitu príliš zovšeobecniť.
- **Má správny rozsah?** `paths` by mali pokrývať to, na čo sa rozhodnutie vzťahuje, nie celý repozitár.
- **Je stručný?** Kontext, rozhodnutie, dôsledky (každé na pár riadkov).
- **Nahrádza niečo?** Potom by mal starý záznam nahradiť cez `supersede`, a nie mu potichu odporovať.

## Keď zmeníš názor

Rozhodnutia sa kvôli zvráteniu neupravujú. Zaznamenaj nové, ktoré nahradí to staré:

```sh
kontext capture --kind decision --title "Orders move to event sourcing" \
  --supersedes 0007 --paths "src/orders/**" --body "…"
```

Starý záznam prejde do stavu `superseded` a odkazuje na nový; `kontext log --all` ukáže celý reťazec a brief uvádza iba aktívne rozhodnutia.

## Úskalia a incidenty

- `--kind pitfall` (poznatok so štítkom `pitfall`) pre zradné veci, ktoré niekoho stáli celé popoludnie.
- `--kind incident` pre to, čo sa pokazilo, príčinu a čo sa zmenilo (stručne); odkáž na úplný post-mortem.

## Zaúčanie nováčika

```sh
kontext brief                 # the map
kontext log                   # the decisions, newest first
kontext why src/payments      # where to be careful
```

Agent, ktorý dostane `ctx_brief` s prvou úlohou nováčika ako `focus`, pripraví dobrú úvodnú prehliadku.

## Udržuj to v dobrom stave

- `kontext check` (alebo [v CI](./07-ci.md)) zvaliduje všetky záznamy a nájde cesty, ktoré už ničomu nezodpovedajú.
- `ctx_prepare_commit` označí dokumenty modulov, ktorých moduly sa výrazne zmenili; `ctx_init` potom ponúkne úlohy `refresh:`.
- Po veľkých reštrukturalizáciách spusti `kontext init` znova: fakty sa obnovia, napísaný text zostane zachovaný.
