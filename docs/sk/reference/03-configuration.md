# Konfigurácia

Konfigurácia je v TOML. Každý kľúč má predvolenú hodnotu; väčšine repozitárov stačí to, čo zapíše `kontext init`.

## Vrstvy {#layers}

Neskoršie vrstvy vyhrávajú. Tabuľky sa zlučujú kľúč po kľúči; polia sa nahrádzajú.

| # | Súbor | Rozsah | Adaptéry |
| --- | --- | --- | --- |
| 1 | vstavané predvolené hodnoty | – | – |
| 2 | `~/.config/kontext/config.toml` | ty, každý repozitár | dôveryhodné |
| 3 | `~/.config/kontext/repos/<slug>.toml` | ty, jeden repozitár (slug z `kontext status`) | dôveryhodné |
| 4 | `<repo>/.kontext.toml` alebo `<repo>/<store.dir>/kontext.toml` (predvolene `.ai/kontext.toml`) | tím (commitnuté) | až po `kontext trust` |
| 5 | `<git-common-dir>/kontext/config.toml` | tento klon | dôveryhodné |

Používateľský adresár je `$KONTEXT_CONFIG_DIR`, inak `$XDG_CONFIG_HOME/kontext`, inak `~/.config/kontext`. `kontext status` vypíše vrstvy, ktoré sa načítali. Neznáme kľúče a kľúč, ktorý neskoršia vrstva zatieni, hlásia `kontext status` a `kontext check` ako varovania.

Niekoľko kľúčov berie len z tvojich vlastných vrstiev (2, 3 a 5), takže ich naklonovaný repozitár nenastaví: `[verify] pass_env`. Dva zoznamy sa naprieč vrstvami spájajú namiesto nahradenia: `secrets.redact` a `secrets.allow`. Zdieľané `[vars]` nikdy neprepíšu `vars` adaptéra deklarovaného v tvojich vlastných vrstvách.

## `[project]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `name` | nastaví ho `kontext init` z koreňového manifestu; inak názov repozitára z remote | zobrazí sa v briefe; `{{project.name}}` v šablónach. V zdieľanej konfigurácii musí byť slug a pokrýva ho trust pin |

## `[store]` {#store}

| Kľúč | Predvolene | |
| --- | --- | --- |
| `dir` | `.ai` | adresár úložiska |
| `style` | `frontmatter` | `frontmatter` alebo `fields` pre nové záznamy |
| `index_file` | `true` | udržiava `<dir>/README.md` |
| `max_body_lines` | `80` | varuje nad týmto počtom neprázdnych riadkov tela (0 = vypnuté) |

### `[store.kinds.<kind>]` {#store-kinds}

| Kľúč | Predvolene | |
| --- | --- | --- |
| `dir` | `decisions`, `conventions`, `learnings`, `incidents`, `architecture` | relatívne k `store.dir` |
| `path` | | relatívne ku koreňu repozitára; má prednosť pred `dir` (napr. `docs/adr`) |
| `style` | `store.style` | |
| `numbering` | `date` pre rozhodnutia, inak `none` | `date`, `sequential`, `none` |
| `trailer` | `Decision`, `Convention`, `Learning`, `Incident` | kľúč traileru commitu; pre architecture žiadny |
| `review_after` | žiadne | predvolený `review_by` pre nové záznamy tohto druhu, počítaný od ich dátumu: `90d`, `12w`, `6m` (mesiace po 30 dňoch) alebo `1y`. Bez nastavenia vypnuté |

Fungujú aj vlastné druhy: pridaj tabuľku s `dir`.

```toml
# dočasné obchádzky treba pozrieť znova do štvrťroka
[store.kinds.incident]
review_after = "90d"
```

## `[sources]`

Dokumenty, ktoré sa indexujú popri úložisku.

| Kľúč | Predvolene |
| --- | --- |
| `include` | `["/*.md", "**/README.md", "**/AGENTS.md", "**/CLAUDE.md", "docs/**/*.md", "doc/**/*.md", "adr/**/*.md"]` |
| `exclude` | `["**/node_modules/**", "**/CHANGELOG.md", "**/vendor/**", "**/fixtures/**", "**/testdata/**", "**/.github/ISSUE_TEMPLATE/**"]` |
| `max_bytes` | `300000` |

Globy sa riadia konvenciami gitu: vzor bez `/` zodpovedá v ľubovoľnej hĺbke, úvodné `/` ho ukotví, `**` prechádza cez adresáre, `{a,b}` sú alternatívy.

## `[index]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `commits` | `true` | indexuje históriu commitov |
| `max_commits` | `5000` | ako ďaleko do minulosti |

## `[brief]`

| Kľúč | Predvolene |
| --- | --- |
| `budget_tokens` | `1400` |
| `max_decisions` | `12` |
| `max_modules` | `14` |

## `[freshness]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `threshold_commits` | `20` | označí aktívne rozhodnutie, konvenciu či poučenie, keď sa jeho `paths` dotklo aspoň toľkoto commitov od najneskoršieho z: jeho dátumu, potvrdenia `checked` a posledného commitu, ktorý ho zmenil; `0` to vypne (pozri [Aktuálnosť](../concepts/04-context-layers.md#freshness)) |

## `[search]` {#search}

| Kľúč | Predvolene | |
| --- | --- | --- |
| `adapters` | `always` | kedy sa `ctx_search` a `kontext search` pýtajú adaptérov s operáciou `search`: `always`, alebo `fallback` — iba keď lokálne zdroje našli menej ako `min_local_hits` zásahov. Adaptér menovaný v `sources` alebo `adapters: true` (`--adapters`) sa pýta vždy |
| `min_local_hits` | `3` | koľko lokálnych zásahov v režime `fallback` stačí |
| `weights` | záznamy nad kúskami dokumentácie a commitmi | tabuľka násobiteľov skóre podľa zdroja (`entry`, `doc`, `commit` alebo názov adaptéra); predvolené hodnoty sú nastavené na graded eval sade (`tests/eval/qrels.tsv`), meň ich len s meraním |

`fallback` ušetrí cestu k adaptéru, keď lokálna odpoveď už existuje (v evaluačnej sade hub s
oneskorením jednej sekundy pridal v režime `always` ku každému hľadaniu asi sekundu a v režime
`fallback` nič). Znamená to však aj, že sémantická pamäť sa nepýta, keď BM25 niečo našlo, a preto
predvolene ostáva `always`.

## `[capture]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `default_visibility` | `team` | `team` alebo `private` |
| `redact` | `true` | maskuje tajné údaje v zachyteniach |

## `[hooks]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `validate` | `true` | validuje stagnuté záznamy v pre-commit |
| `trailers` | `true` | pridáva trailery v prepare-commit-msg |
| `remind_inbox` | `true` | pripomenie kandidátov z inboxu, ktorí sa týkajú stagnutých ciest |
| `enabled` | `true` | hooky hostiteľov agentov (`kontext hook --host`) odpovedajú; `false` (alebo `KONTEXT_HOOKS=off`) spôsobí, že nevypíšu nič. Bežia len tam, kde ich nainštaloval `kontext connect <host> --hooks` |
| `session_budget_tokens` | vstavané (ukáže ho `kontext status`) | jeden rozpočet pre všetko, čo kontext vloží do session: čerpá z neho `ctx_brief` aj hooky hostiteľa; nie je to druhý rozpočet |
| `stop_note` | `false` | Stop hook, ktorý pridá jednoriadkovú pripomienku zachytenia ako `additionalContext`; nikdy neblokuje |

Prvé tri kľúče riadia git hooky, posledné tri hooky hostiteľov agentov (pozri [Hooky](../guides/09-hooks.md)).

## `[secrets]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `scan` | `store` | `store` (súbory znalostí), `staged` (každý stagnutý textový súbor) alebo `off` |
| `allow` | `[]` | regexy riadkov, ktoré sa ignorujú; spájajú sa naprieč vrstvami |
| `redact` | `[]` | regexy hodnôt, ktoré sa maskujú vo vláknach agentov a v tom, čo sa z nich vydestiluje (meno klienta, id integrácie); drž ich v súkromnej vrstve (`.git/kontext/config.toml`, `~/.config/kontext/repos/<slug>.toml`). Tvoje vzory sa spájajú s repozitárovými, nikdy ich repozitár nenahradí. Vzor, ktorý sa nedá skompilovať, je chyba v `status` aj `check` a distill, threads aj autopilot odmietnu poslať modelu čokoľvek, kým ho neopravíš |

## `[init]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `max_module_docs` | `40` | počet dokumentov modulov na zápis |
| `min_module_files` | `3` | súbory kódu pre *odvodený* modul |
| `exclude` | `[]` | ďalšie globy na preskočenie |
| `history_max_commits` | `3000` | commity na vyťaženie |
| `max_decision_tasks` | `20` | úlohy `dec:` |
| `task_max_files` | `12` | kľúčové súbory na úlohu modulu |
| `task_inline_chars` | `24000` | priložený zdroják na úlohu autopilota |
| `jobs` | `2` | paralelizmus autopilota; `--jobs` ho prepíše |
| `llm` | | predvolený adaptér autopilota |
| `decision_words` | `[]` | ďalšie slová, ktoré označia commit v tvare rozhodnutia, v jazyku tvojho tímu (napríklad `rozhodli`, `namiesto`, `prechádzame`); pridajú sa k jazykovo neutrálnym signálom |
| `bot_authors` | `[]` | ďalšie mená alebo e-maily autorov vynechaných zo skórovania histórie, popri vstavaných botoch (dependabot, renovate, `*[bot]`) |

## `[vars]`

Globálne premenné šablón, dostupné každému adaptéru ako `{{vars.<name>}}` (`vars` adaptéra ich prepíšu).

Zdieľané `[vars]` pokrýva trust pin a nikdy neprepíšu `vars` adaptéra deklarovaného v tvojich vlastných vrstvách.

## `[verify]` {#verify}

| Kľúč | Predvolene | |
| --- | --- | --- |
| `pass_env` | `[]` | mená ďalších premenných prostredia, ktoré verify príkaz dostane, popri `PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR` a `TERM`. Číta sa len z tvojich vlastných vrstiev; zdieľaná konfigurácia ho nenastaví |

Verify príkaz beží až po `kontext trust --verify`; pozri [Bezpečnosť](../concepts/08-security.md#verify).

## `[adapters.<name>]`

Pozri [Konfigurácia adaptérov](../adapters/02-configuration.md).

## Príklad: repozitár s existujúcimi ADR

```toml
[project]
name = "shop"

[store]
dir = ".ai"

[store.kinds.decision]
path = "docs/adr"
style = "fields"
numbering = "sequential"
trailer = "Decision"

[sources]
include = ["/*.md", "**/README.md", "docs/**/*.md", "handbook/**/*.md"]

[init]
exclude = ["legacy/**"]
```
