# Bezpečnosť a súkromie

kontext beží na tvojom počítači s tvojimi oprávneniami a pracuje s textom, ktorý môže skončiť v zdieľanom repozitári. Toto sú záruky, ktoré sa snaží dať, a miesta, kde končia.

## Čo opustí tvoj počítač

Nič, kým to nenastavíš:

- lokálny index, inbox, outbox a cache zostávajú v `.git/kontext/`,
- k ostatným sa znalosti dostanú len cez commity, ktoré pushneš,
- adaptéry posielajú dáta len tam, kam to určuje ich konfigurácia (a len pre operácie a udalosti, ktoré deklarujú),
- `kontext distill` pošle skrátený prepis vlákna LLM adaptéru, ktorý si vyberieš, po redakcii. Čo sa maskuje a čo pred promptom vypadne, je v [citlivých vláknach](../guides/08-distill-threads.md#sensitive-threads). Neplatný vzor `secrets.redact` zastaví distill, threads aj autopilot skôr, než sa čokoľvek odošle.

## Dôvera pre adaptéry deklarované v repozitári {#trust-for-repository-declared-adapters}

Adaptéry môžu spúšťať procesy a volať URL. Repozitár, ktorý si naklonuješ, nesmie vedieť prinútiť kontext (ani jeho git hooky) spúšťať ľubovoľné príkazy. Preto:

- adaptéry v tvojej používateľskej konfigurácii (`~/.config/kontext/…`) a v konfigurácii pre klon sú dôveryhodné,
- adaptéry deklarované v **zdieľanej** konfigurácii repozitára (`.ai/kontext.toml`) sa **preskočia**, kým ich neposúdiš a nespustíš `kontext trust` z terminálu,
- dôvera je naviazaná (v `~/.config/kontext/trust.toml`) na SHA-256 hash nad kanonickým TOML všetkého zdieľaného, čo sa dostane do adaptéra: tabuľky `adapters`, `[vars]` a `project.name`. Každá ich zmena si vyžaduje novú dôveru,
- starý 12-hex FNV pin z verzie pred 0.10 sa odmietne s hláškou, nikdy sa ticho nepovýši: spusti `kontext trust` znova,
- zdieľané `[vars]` nikdy neprepíšu vars používateľského ani lokálneho adaptéra, zdieľané `project.name` musí byť slug a `{{repo.remote}}` nenesie userinfo.

`kontext status` a `kontext adapters list` ukazujú nedôveryhodné adaptéry ako varovanie.

Čo dôvera pokrýva:

- **Deklarovaný text, nie skripty, ktoré spúšťa.** Ak dôveryhodný adaptér spúšťa `./scripts/sync.sh`, pin pokrýva riadok, ktorý skript menuje, nie skript. Neskorší commit, ktorý skript zmení, sa nezachytí. Skripty, ktoré adaptér spúšťa z repozitára, posudzuj ako každý kód, ktorý spúšťaš.
- **Voči procesu, ktorý vie upraviť `trust.toml`, platí na čestné slovo.** `kontext trust` bez terminálu nebeží, ale agent, ktorý vie alokovať terminál alebo zapísať tvoju konfiguráciu, pinne čokoľvek. `kontext connect` nainštaluje pravidlo hostiteľa, ktoré zakáže `kontext trust` spustený agentom (`PreToolUse` na `Bash` v Claude Code a Codexe, `beforeShellExecution` v Cursore); je to zábradlie, nie hranica.

## Verify príkazy sú hranica dôvery {#verify}

Pole `verify:` záznamu je shellový príkaz z repozitára a jeho spustenie je spustenie kódu z repozitára. Preto:

- verify príkaz sa spustí, len keď `trust.toml` drží pin jeho textu: `sha256(cesta záznamu, NUL, príkaz)` pod `[repos.<slug>.verify]`,
- `kontext trust --verify <id>` (alebo `--all`) pinuje len z interaktívneho terminálu a vypíše každý príkaz, ktorý pinuje. Žiadny MCP nástroj ani operácia `ctx_api` nepinuje,
- bez pinu `kontext verify`, `drafts.verify` ani `ctx_verify` nič nespustia a vrátia `untrusted_verify` s príkazom, jeho SHA-256 (`command_sha256`) a riadkom, ktorým ho pinneš (`hint`). Úprava príkazu pin zruší,
- cez MCP sa nepinnutý príkaz vráti agentovi, ktorý ho spustí vo vlastnom sandboxe; výsledok sa zapíše ako `agent` pečiatka, nie ako prejdená kontrola,
- príkaz beží s povoleným zoznamom premenných prostredia: `PATH`, `HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR`, `TERM` a mená z `[verify] pass_env` z tvojej používateľskej konfigurácie (zdieľaná ani lokálna konfigurácia klonu nepridá žiadne),
- cez MCP sa nespustí ani pinnutý príkaz, kým má súbor záznamu alebo jeho riadené cesty necommitnuté zmeny; inde sa taký beh označí `[dirty: <cesty>]` v dôvode riadku denníka,
- `output_tail` sa rediguje a skupina procesov sa zabije pri timeoute, keď stdout ostane otvorený 2 s po skončení shellu, a keď sa MCP požiadavka zruší alebo sa zatvorí stdin,
- `ctx_verify` má anotácie `destructiveHint: true` a `openWorldHint: true`, takže hostitelia sa pred volaním pýtajú.

Čo pin **nepokrýva**:

- **Pin viaže súhlas na text príkazu, nie na kód, ktorý spustí.** `cargo test` alebo `npm test` spustí, čokoľvek je v pracovnom strome; vetva môže zmeniť testy bez zmeny príkazu.
- **`HOME` ostáva čitateľný a sieť otvorená.** Povolený zoznam odstráni tokeny z prostredia, nie prístup k súborom ani sieti.
- **V Codexe beží `ctx_verify` v MCP serveri, mimo sandboxu agenta.** Radšej nech agent vrátený príkaz spustí sám.

Jediná cesta, na ktorej pull request nevie prepašovať príkaz do behu s credentials CI, je spúšťať v CI znenie z base revízie; pozri [CI](../guides/07-ci.md).

## Skenovanie tajných údajov

Pre-commit hook skenuje každý stagnutý súbor znalostí; nálezy s vysokou istotou commit zablokujú.

| Pravidlo | Závažnosť |
| --- | --- |
| súkromné kľúče (PEM, OpenSSH, PGP), JSON servisného účtu GCP | vysoká |
| prístupové kľúče AWS, tokeny GitHub a fine-grained PAT, GitLab, Slack, live kľúče Stripe | vysoká |
| kľúče Anthropic (`sk-ant-…`) a OpenAI (`sk-…`), kľúče dotenv-vault | vysoká |
| hlavičky `Authorization: Bearer …`, prihlasovacie údaje v URL | vysoká |
| API kľúče Google, JWT, priradenia `*_TOKEN=` / `password:` (zástupné hodnoty sa ignorujú) | stredná |
| tajné údaje vo vetách („the secret is …“, „rotate the api key …“), keď hodnota vyzerá náhodne (zmiešané veľké a malé písmená, číslice, vysoká entropia; cesty a zástupné hodnoty sa ignorujú) | stredná |

- `secrets.scan = "staged"` rozšíri skenovanie na každý stagnutý textový súbor (nielen znalosti).
- Riadok, ktorý obsahuje `kontext:allow-secret` alebo zodpovedá regexu v `secrets.allow`, sa ignoruje.
- `secrets.redact` a `secrets.allow` z tvojej používateľskej alebo lokálnej konfigurácie sa spájajú s repozitárovými, nikdy ich repozitár nenahradí.
- `ctx_capture` a výsledky prehĺbenia nálezy automaticky zamaskujú (`[redacted:<rule>]`).

## Súbory, ktoré kontext nikdy nečíta

Súbory `.env` (okrem `.example`, `.sample`, `.template`, `.dist`), SSH kľúče, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.keystore`, `*.tfstate`, `*.tfvars`, `credentials.json`, `.npmrc`, `.pypirc`, `.netrc`, JSON súbory servisných účtov a konfiguračné súbory `*secret*` sú vylúčené zo skenovania, indexovania, `ctx_read` aj z promptov autopilota. *Názvy* premenných prostredia sa čítajú z ukážkových súborov; hodnoty nikdy.

## Agenti a repozitár

- `ctx_read` vracia len súbory vnútri repozitára a odmietne `.git/`, `.git-credentials` a `.ssh`.
- `ctx_threads` číta len `claude:`, `codex:`, `superset:`, `last` a cesty pod známymi koreňmi prepisov.
- `dir` nástrojov `ctx_verify`, `ctx_inbox` a `ctx_prepare_commit` je obmedzený na tento repozitár, jeho worktrees a submoduly.
- Nástroje, ktoré zapisujú (`ctx_capture`, `ctx_inbox promote`, `ctx_prepare_commit`, `ctx_init_submit`, `ctx_confirm`, `ctx_challenge`), menia len pracovný strom a git index, nikdy históriu, nikdy remote. `ctx_prepare_commit drop` odmietne kandidáta, ktorého zapísal iný worktree.
- `ctx_verify` spúšťa pinnutý príkaz (pozri [vyššie](#verify)); hostitelia by sa pred ním mali pýtať.
- Príkazy, ktoré spúšťajú adaptéry, dostávajú argumenty ako vektor argumentov, nikdy cez shell. Chybové hlášky adaptérov nenesú query string ani userinfo.

## Nahlásenie zraniteľnosti

Prosím, neotváraj verejné issue. Použi súkromné hlásenie zraniteľností na GitHube (pozri [SECURITY.md](https://github.com/SemanS/kontext/blob/main/SECURITY.md)).
