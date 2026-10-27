# CLI

```text
kontext [-C <dir>] <command> [options]
```

`-C, --dir <dir>` spustí kontext tak, akoby bol spustený v `<dir>` (funguje so všetkými príkazmi). `kontext <command> --help` vypíše voľby uvedené nižšie.

## Zavedenie

Aplikácie používajú [`kontext api <operation> [JSON|-]`](./05-application-api.md) pre verzované JSON
výsledky a chyby: dotazy na znalosti, extrakciu, idempotentný staging a overený export.

### `kontext init`

Spustí scan → history → render → wire → deepen. Opakované spustenie je bezpečné.

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `--no-hooks` | | nenainštaluje git hooky, zapamätá sa pre klon; `kontext hooks install` ich neskôr pridá |
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
| `--adapters` | | pýta sa vyhľadávacích adaptérov, aj keď lokálne zásahy stačia (pozri [`[search]`](./03-configuration.md#search)) |

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
| `-k, --kind <kind>` | `decision`, `convention`, `learning` (`pitfall`), `incident` (povinné) |
| `-t, --title <text>` | krátke tvrdenie (povinné) |
| `-b, --body <text>` / `--file <path\|->` / stdin | telo |
| `--summary <text>` | jeden riadok (keď chýba, odvodí sa z tela) |
| `-p, --paths <a,b>` | cesty alebo globy, ktorých sa týka |
| `--tags <a,b>` | štítky |
| `--supersedes <id,…>` | záznamy, ktoré nahrádza |
| `--status <status>` | `proposed` alebo `accepted` (rozhodnutia predvolene accepted) |
| `--private` | nechá ho osobným (nikdy sa nepovýši) |
| `--promote` | zapíše ho hneď do úložiska namiesto inboxu |
| `--review-by <YYYY-MM-DD>` | kedy ho znova pozrieť (inak `review_after` druhu, ak je nastavené) |
| `--verify <príkaz>` | príkaz, ktorý ho dokázal: jeden riadok, najviac 500 znakov, spúšťa ho [`kontext verify`](../concepts/03-capture-and-review.md#verify) |

### `kontext challenge <id> <prečo…> [--private]`

Povie, že záznam už neplatí, a prečo. Námietka čaká v lokálnom inboxe; `prepare-commit` ponúkne vyradenie záznamu alebo zaznamenanie náhrady, ktorá ho nahrádza.

### `kontext confirm <id…> [--because <text>] | --repair`

Potvrdí, že záznamy stále platia: nastaví [`checked`](../concepts/02-knowledge-store.md#checked) na aktuálny čas v UTC (`YYYY-MM-DDTHH:MM:SSZ`) v súbore každého záznamu, nič iné nezmení, pridá za každý záznam riadok `read` do [denníka overení](../concepts/03-capture-and-review.md#verifications) (`bulk`, keď je ID viac ako päť) a oboje stagne. `--because` uloží do denníka jednoriadkový dôvod. Odmietne neznámy alebo neaktívny záznam, záznam, ktorý ešte nie je commitnutý, a súbor s nestagnutými úpravami. Keď niektoré ID zlyhá, skončí so stavom 1.

`--repair` prepíše `checked` každého aktívneho záznamu, ktorého súbor zaostáva za jeho najnovším potvrdením v denníku (merge vzal staršiu stranu), a súbor stagne; keď niektorý prepísať nejde, skončí so stavom 1.

### `kontext verify <id…> [--because <text>] [--json]`

Spustí príkaz [`verify`](../concepts/03-capture-and-review.md#verify) každého záznamu cez `sh -c` z koreňa repozitára (zastaví ho po `KONTEXT_VERIFY_TIMEOUT_SECS`, predvolene 120), pridá do denníka riadok `verify` s výsledkom a commitom a stagne ho. Vypíše výsledok a pri zlyhaní koniec výstupu; `--json` vypíše výsledky `drafts.verify`. Odmietne záznam bez príkazu a neaktívny záznam. Keď príkaz zlyhá alebo je ID odmietnuté, skončí so stavom 1.

### `kontext drift`

Záznamy, ktoré rozsah commitov nechal za sebou: aktívne záznamy, ktorých `paths` pokrývajú zmenené súbory, kým ich vlastný súbor sa v rozsahu neupravil ani nepotvrdil. Pozri [Drift](../concepts/03-capture-and-review.md#drift).

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `--base <rev>` | merge base `HEAD` s `origin/HEAD` (`origin/main`, `origin/master`), inak `HEAD` | začiatok rozsahu |
| `--head <rev>` | pracovný strom | koniec rozsahu; bez neho sa počítajú aj necommitnuté a nesledované zmeny |
| `-k, --kind <druh>` | decision, convention, learning, incident | druhy na kontrolu (opakovateľné) |
| `--json` | | výsledok `entries.drift` ako JSON |
| `--fail` | | skončí so stavom 2, keď je vypísaný nejaký záznam: brána pre tímový kontrolný skript alebo CI, ktorá pokryje aj commity bez hookov (jj) |

### `kontext inbox [list | show <id> | drop <id…> | verify <id> <príkaz|--clear>]`

Lokálni kandidáti. `list` je predvolený. `verify` nastaví príkaz, ktorý návrh dokázal (overuje sa ako `--verify` pri `capture`); `--clear` ho odstráni. Povýšenie ho zachová.

### `kontext promote <id…> [--no-stage]`

Zapíše kandidátov do úložiska a spraví `git add`.

### `kontext prepare-commit [--promote <id>]… [--drop <id>]… [--retire <id>]…`

Report, ktorý agenti dostanú z `ctx_prepare_commit`. Keď nie je nič stagnuté, prečíta zmeny pracovného stromu voči `HEAD` (vrátane nesledovaných súborov) a povie to; to treba spustiť pred `jj commit`. `--retire` vyradí záznam s námietkou z tohto worktree: stav `deprecated`, námietka ako dôvod v poznámke `## Retired`, stagnuté.

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

### `kontext ci [--base <rev>] [--format text|markdown] [--strict]`

Pre CI job: `kontext check` nad všetkými záznamami plus `kontext drift` nad commitnutým rozsahom `base..HEAD` (`--base` má rovnaké predvolenie ako pri `drift`). `--format markdown` sa hodí do súhrnu jobu na GitHube alebo do komentára k pull requestu. Pri chybách kontroly skončí so stavom 1; s `--strict` aj so stavom 2, keď drift vypíše nejaký záznam, inak je drift len varovanie (stav 0). Pozri [Validácia znalostí v CI](../guides/07-ci.md).

## Git hooky

| Príkaz | Význam |
| --- | --- |
| `kontext hooks install [--hooks-dir <dir>]` | nainštaluje do `core.hooksPath`, `.git/hooks` alebo `<dir>` |
| `kontext hooks uninstall [--hooks-dir <dir>]` | odstráni bloky kontextu, obnoví zreťazené originály |
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
| `kontext status` | identita, vrstvy konfigurácie, úložisko, index, záznamy na prehodnotenie, záznamy po termíne `review_by`, inbox, postup initu, adaptéry, hooky, outbox |
| `kontext reindex` | zostaví lokálny vyhľadávací index od nuly |

## Premenné prostredia

| Premenná | Význam |
| --- | --- |
| `KONTEXT_CONFIG_DIR` | adresár používateľskej konfigurácie (predvolene `$XDG_CONFIG_HOME/kontext` alebo `~/.config/kontext`) |
| `KONTEXT_DIR` | repozitár pre `kontext mcp`, keď sa nespustí v ňom |
| `KONTEXT_SKIP=1` | hooky nerobia nič (jednorazové obídenie) |
| `KONTEXT_TRUST_REPO_ADAPTERS=1` | dôveruje adaptérom deklarovaným v repozitári bez `kontext trust` (CI, sandboxy) |
