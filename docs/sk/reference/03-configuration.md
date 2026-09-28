# Konfigurácia

Konfigurácia je v TOML. Každý kľúč má predvolenú hodnotu; väčšine repozitárov stačí to, čo zapíše `kontext init`.

## Vrstvy {#layers}

Neskoršie vrstvy vyhrávajú. Tabuľky sa zlučujú kľúč po kľúči; polia sa nahrádzajú.

| # | Súbor | Rozsah | Adaptéry |
| --- | --- | --- | --- |
| 1 | vstavané predvolené hodnoty | — | — |
| 2 | `~/.config/kontext/config.toml` | ty, každý repozitár | dôveryhodné |
| 3 | `~/.config/kontext/repos/<slug>.toml` | ty, jeden repozitár (slug z `kontext status`) | dôveryhodné |
| 4 | `<repo>/.kontext.toml` alebo `<repo>/<store.dir>/kontext.toml` (predvolene `.ai/kontext.toml`) | tím (commitnuté) | až po `kontext trust` |
| 5 | `<git-common-dir>/kontext/config.toml` | tento klon | dôveryhodné |

Používateľský adresár je `$KONTEXT_CONFIG_DIR`, inak `$XDG_CONFIG_HOME/kontext`, inak `~/.config/kontext`. `kontext status` vypíše vrstvy, ktoré sa načítali.

## `[project]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `name` | nastaví ho `kontext init` z koreňového manifestu; inak názov repozitára z remote | zobrazí sa v briefe; `{{project.name}}` v šablónach |
| `description` | | |

## `[store]` {#store}

| Kľúč | Predvolene | |
| --- | --- | --- |
| `dir` | `.ai` | adresár úložiska |
| `style` | `frontmatter` | `frontmatter` alebo `fields` pre nové záznamy |
| `index_file` | `true` | udržiava `<dir>/README.md` |
| `max_body_lines` | `80` | varuje nad týmto počtom neprázdnych riadkov tela (0 = vypnuté) |

### `[store.kinds.<kind>]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `dir` | `decisions`, `conventions`, `learnings`, `incidents`, `architecture` | relatívne k `store.dir` |
| `path` | | relatívne ku koreňu repozitára; má prednosť pred `dir` (napr. `docs/adr`) |
| `style` | `store.style` | |
| `numbering` | `date` pre rozhodnutia, inak `none` | `date`, `sequential`, `none` |
| `trailer` | `Decision`, `Convention`, `Learning`, `Incident` | kľúč traileru commitu; pre architecture žiadny |

Fungujú aj vlastné druhy – pridaj tabuľku s `dir`.

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

## `[secrets]`

| Kľúč | Predvolene | |
| --- | --- | --- |
| `scan` | `store` | `store` (súbory znalostí), `staged` (každý stagnutý textový súbor) alebo `off` |
| `allow` | `[]` | regexy riadkov, ktoré sa ignorujú |

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
| `jobs` | `2` | paralelizmus autopilota (aj `--jobs`) |
| `llm` | | predvolený adaptér autopilota |

## `[vars]`

Globálne premenné šablón, dostupné každému adaptéru ako `{{vars.<name>}}` (`vars` adaptéra ich prepíšu).

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
