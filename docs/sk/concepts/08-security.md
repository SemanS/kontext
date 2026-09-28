# Bezpečnosť a súkromie

kontext beží na tvojom počítači s tvojimi oprávneniami a pracuje s textom, ktorý môže skončiť v zdieľanom repozitári. Toto sú záruky, ktoré sa snaží dať.

## Čo opustí tvoj počítač

Nič, kým to nenastavíš:

- lokálny index, inbox, outbox a cache zostávajú v `.git/kontext/`,
- k ostatným sa znalosti dostanú len cez commity, ktoré pushneš,
- adaptéry posielajú dáta len tam, kam to určuje ich konfigurácia (a len pre operácie a udalosti, ktoré deklarujú).

## Dôvera pre adaptéry deklarované v repozitári {#trust-for-repository-declared-adapters}

Adaptéry môžu spúšťať procesy a volať URL. Repozitár, ktorý si naklonuješ, nesmie vedieť prinútiť kontext – ani jeho git hooky – spúšťať ľubovoľné príkazy. Preto:

- adaptéry v tvojej používateľskej konfigurácii (`~/.config/kontext/…`) a v konfigurácii pre klon sú dôveryhodné,
- adaptéry deklarované v **zdieľanej** konfigurácii repozitára (`.ai/kontext.toml`) sa **preskočia**, kým ich neposúdiš a nespustíš `kontext trust`,
- dôvera je naviazaná na hash tabuľky adaptérov (v `~/.config/kontext/trust.toml`); každá zmena si vyžaduje novú dôveru.

`kontext status` a `kontext adapters list` ukazujú nedôveryhodné adaptéry ako varovanie.

## Skenovanie tajných údajov

Pre-commit hook skenuje každý stagnutý súbor znalostí; nálezy s vysokou istotou commit zablokujú.

| Pravidlo | Závažnosť |
| --- | --- |
| súkromné kľúče (PEM, OpenSSH, PGP), JSON servisného účtu GCP | vysoká |
| prístupové kľúče AWS, tokeny GitHub a fine-grained PAT, GitLab, Slack, live kľúče Stripe | vysoká |
| kľúče Anthropic (`sk-ant-…`) a OpenAI (`sk-…`), kľúče dotenv-vault | vysoká |
| hlavičky `Authorization: Bearer …`, prihlasovacie údaje v URL | vysoká |
| API kľúče Google, JWT, priradenia `*_TOKEN=` / `password:` (zástupné hodnoty sa ignorujú) | stredná |

- `secrets.scan = "staged"` rozšíri skenovanie na každý stagnutý textový súbor (nielen znalosti).
- Riadok, ktorý obsahuje `kontext:allow-secret` alebo zodpovedá regexu v `secrets.allow`, sa ignoruje.
- `ctx_capture` a výsledky prehĺbenia nálezy automaticky zamaskujú (`[redacted:<rule>]`).

## Súbory, ktoré kontext nikdy nečíta

Súbory `.env` (okrem `.example`, `.sample`, `.template`, `.dist`), SSH kľúče, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.keystore`, `*.tfstate`, `*.tfvars`, `credentials.json`, `.npmrc`, `.pypirc`, `.netrc`, JSON súbory servisných účtov a konfiguračné súbory `*secret*` sú vylúčené zo skenovania, indexovania, `ctx_read` aj z promptov autopilota. *Názvy* premenných prostredia sa čítajú z ukážkových súborov; hodnoty nikdy.

## Agenti a repozitár

- `ctx_read` vracia len súbory vnútri repozitára.
- Nástroje, ktoré zapisujú (`ctx_capture`, `ctx_inbox promote`, `ctx_prepare_commit`, `ctx_init_submit`), menia len pracovný strom a git index – nikdy históriu, nikdy remote.
- Príkazy, ktoré spúšťajú adaptéry, dostávajú argumenty ako vektor argumentov, nikdy cez shell.

## Nahlásenie zraniteľnosti

Prosím, neotváraj verejné issue. Použi súkromné hlásenie zraniteľností na GitHube – pozri [SECURITY.md](https://github.com/SemanS/kontext/blob/main/SECURITY.md).
