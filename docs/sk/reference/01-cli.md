# CLI

```text
kontext [-C <dir>] <command> [options]
```

`-C, --dir <dir>` spustí kontext tak, akoby bol spustený v `<dir>` (funguje so všetkými príkazmi). `kontext <command> --help` vypíše voľby uvedené nižšie.

## Zavedenie

### `kontext init`

Spustí scan → history → render → wire → deepen. Opakované spustenie je bezpečné.

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `--no-hooks` | | nenainštaluje git hooky |
| `--connect <targets>` | | pripojí aj klientov, oddelených čiarkou (`claude,claude-hooks,cursor,opencode,agents-md`) |
| `--deepen` | | spustí čakajúce úlohy prehĺbenia cez LLM adaptér |
| `--llm <adapter>` | `init.llm` | adaptér s operáciou `llm` |
| `--jobs <n>` | 2 | paralelné volania LLM |
| `--max <n>` | 50 | úlohy v jednom behu |

| Podpríkaz | Význam |
| --- | --- |
| `kontext init status` | stav fáz a postup prehĺbenia |
| `kontext init next [-n <count>] [--inline]` | vypíše ďalšiu úlohu (úlohy); `--inline` priloží úryvky súborov (na vloženie do chatového modelu) |
| `kontext init submit <file\|->` | zapíše výsledok úlohy (JSON: `task_id`, `summary`, `overview`, `decisions`, `learnings`) |
| `kontext init skip <task> [--reason <text>]` | preskočí úlohu |

## Každodenná práca

### `kontext brief`

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `-f, --focus <path\|word>` | | uprednostní tieto cesty alebo témy (dá sa opakovať) |
| `--budget <tokens>` | `brief.budget_tokens` | približná veľkosť |
| `--format <fmt>` | `text` | `text`, `json`, `claude-hook` (SessionStart `additionalContext`) |
| `--no-adapters` | | vynechá sekcie `brief` z adaptérov |

### `kontext search <query…>`

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `-k, --kind <kind>` | | `decision`, `convention`, `learning`, `incident`, `architecture`, `doc`, `commit` (dá sa opakovať) |
| `-s, --source <name>` | všetky | `local` a/alebo názvy adaptérov (dá sa opakovať) |
| `-n, --limit <n>` | 10 | najviac výsledkov |
| `--json` | | výsledky ako JSON |

### `kontext read <uri>`

`-l, --level <0|1|2>` (predvolene 2). URI: `kx:<id>`, `file:<path>[#section]`, `git:<sha>`, `inbox:<id>`, obyčajná cesta v repozitári alebo URI adaptéra.

### `kontext why <target>`

`target` je cesta, `path:line`, `path:start-end`, SHA commitu alebo symbol/téma. `-n, --limit <n>` (predvolene 12) položiek na sekciu.

### `kontext log`

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `-k, --kind <kind>` | `decision` | druh, ktorý sa vypíše |
| `--all` | | zahrnie nahradené a zamietnuté záznamy |
| `-n, --limit <n>` | 50 | riadky |
| `--json` | | riadky ako JSON |

## Zachytávanie a commitovanie

### `kontext capture`

| Voľba | Význam |
| --- | --- |
| `-k, --kind <kind>` | `decision`, `convention`, `learning` (`pitfall`), `incident` – povinné |
| `-t, --title <text>` | krátke tvrdenie – povinné |
| `-b, --body <text>` / `--file <path\|->` / stdin | telo |
| `--summary <text>` | jeden riadok (keď chýba, odvodí sa z tela) |
| `-p, --paths <a,b>` | cesty alebo globy, ktorých sa týka |
| `--tags <a,b>` | štítky |
| `--supersedes <id,…>` | záznamy, ktoré nahrádza |
| `--status <status>` | `proposed` alebo `accepted` (rozhodnutia predvolene accepted) |
| `--private` | nechá ho osobným (nikdy sa nepovýši) |
| `--promote` | zapíše ho hneď do úložiska namiesto inboxu |

### `kontext inbox [list | show <id> | drop <id…>]`

Lokálni kandidáti. `list` je predvolený.

### `kontext promote <id…> [--no-stage]`

Zapíše kandidátov do úložiska a spraví `git add`.

### `kontext prepare-commit [--promote <id>]… [--drop <id>]…`

Report, ktorý agenti dostanú z `ctx_prepare_commit`.

### `kontext distill [<súbor>…] [--claude <id|last>]… [--codex <id|last>]… [--superset [<workspace>]]`

Vytiahne znalosti z vlákien agentov do inboxu; bez vlákna vypíše vlákna tohto repozitára na tomto počítači. Pozri [Znalosti z vlákien agentov](../guides/08-distill-threads.md).

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `--llm <adapter>` | `init.llm` | adaptér s operáciou `llm` |
| `--max <n>` | 5 | najviac záznamov na vlákno |
| `--max-parts <n>` | 12 | najviac častí prepisu na vlákno |
| `--jobs <n>` | 3 | súbežné volania modelu |
| `--list` | | vlákna len vypíše |
| `--dry-run` | | zistenia vypíše, nič nezachytí |

### `kontext check [--staged]`

Zvaliduje všetky záznamy (alebo presne to, čo je stagnuté) a preskenuje ich na tajné údaje. Pri chybách skončí so stavom 1.

## Git hooky

| Príkaz | Význam |
| --- | --- |
| `kontext hooks install [--dir <dir>]` | nainštaluje do `core.hooksPath`, `.git/hooks` alebo `<dir>` |
| `kontext hooks uninstall [--dir <dir>]` | odstráni bloky kontextu, obnoví zreťazené originály |
| `kontext hooks status` | ktoré hooky obsahujú blok |
| `kontext hook <name> [args…]` | vstupný bod, ktorý volajú hooky (`pre-commit`, `prepare-commit-msg`, `post-commit`, `post-merge`, `post-rewrite`) |

## Adaptéry

| Príkaz | Význam |
| --- | --- |
| `kontext adapters [list]` | aktívne a neaktívne adaptéry aj s dôvodom |
| `kontext adapters test [<name>] [--query <q>]` | kontrola stavu a ukážkové volanie každej operácie |
| `kontext adapters inspect <name>` | nástroje MCP adaptéra a ich vstupné schémy |
| `kontext adapters add <preset> [--scope user\|repo\|local] [--var k=v]… [--force]` | pripojí preset do vrstvy konfigurácie |
| `kontext presets [<name>]` | vypíše presety alebo jeden z nich |
| `kontext trust` | dôveruje adaptérom deklarovaným v zdieľanej konfigurácii (najprv ich ukáže) |
| `kontext outbox [list \| flush [--quiet]]` | čakajúce doručenia; doruč hneď |
| `kontext sync [--all]` | zaradí udalosti `sync` pre znalosti v `HEAD` (`--all`: každý záznam raz) |

## Klienti a servery

| Príkaz | Význam |
| --- | --- |
| `kontext mcp` | spustí MCP server na stdio |
| `kontext connect <target> [--write]` | `claude`, `claude-hooks`, `codex`, `cursor`, `opencode`, `agents-md` |
| `kontext call <tool> ['<json>']` | zavolá nástroj agenta lokálne, napr. `kontext call ctx_why '{"target":"src/api"}'` |

## Údržba

| Príkaz | Význam |
| --- | --- |
| `kontext status` | identita, vrstvy konfigurácie, úložisko, index, inbox, postup initu, adaptéry, hooky, outbox |
| `kontext reindex` | zostaví lokálny vyhľadávací index od nuly |

## Premenné prostredia

| Premenná | Význam |
| --- | --- |
| `KONTEXT_CONFIG_DIR` | adresár používateľskej konfigurácie (predvolene `$XDG_CONFIG_HOME/kontext` alebo `~/.config/kontext`) |
| `KONTEXT_DIR` | repozitár pre `kontext mcp`, keď sa nespustí v ňom |
| `KONTEXT_SKIP=1` | hooky nerobia nič (jednorazové obídenie) |
| `KONTEXT_TRUST_REPO_ADAPTERS=1` | dôveruje adaptérom deklarovaným v repozitári bez `kontext trust` (CI, sandboxy) |
