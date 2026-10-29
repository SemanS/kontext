# Aplikačné API

Aplikácie môžu používať kontext ako svoj engine projektových znalostí. Dodajú text zdrojov, externé
identifikátory a cesty repozitára; kontext spracuje konfiguráciu úložiska, Markdown/ADR formáty,
vyhľadávanie, platnosť podľa ciest, extrakciu a staging do Gitu. Rozhranie, autentifikácia, tímové
fronty a doručovanie agentom zostávajú aplikácii.

## Transport a kompatibilita

```sh
kontext -C /cesta/k/repozitaru api status
kontext api entries.list '{"limit":20,"offset":0}'
kontext api drafts.stage - < request.json
```

Posledný argument je JSON objekt alebo `-` na načítanie jedného objektu zo stdin (najviac 2 MiB).
Predvolene je `{}`. Každé volanie zapíše na stdout jednu úplnú JSON odpoveď:

```json
{"schema_version":1,"ok":true,"result":{"entries":[],"total":0}}
```

Chyba má tvar `{"schema_version":1,"ok":false,"error":{"code":"invalid_input","message":"…"}}`
a nenulový návratový kód. Klienti sa riadia `error.code`, nie znením správy. Neznáma operácia vracia
`unsupported_operation`, chýbajúci záznam `not_found`. Ostatné chyby majú vlastný kód alebo
`operation_failed`. Klienti musia tolerovať nové polia odpovede.

MCP nástroj `ctx_api` prijíma `{operation,input?,dir?}` a používa rovnakú aplikačnú službu. Výsledok
obsahuje odpoveď v `structuredContent`, schému `outputSchema`, úplnú textovú JSON kópiu pre starších
klientov a pri chybe `isError: true`. Strojové odpovede sa nekrátia a nedostávajú textové prefixy.
Existujúce príkazy pre ľudí a nástroje pre agentov zostávajú dostupné.

Pomocou `status` overte `api_version: 1`, podporované `capabilities` a skutočný výber LLM. Samotná
nainštalovaná binárka nedokazuje konfiguráciu modelu. Aplikácia má zabezpečiť kompatibilnú binárku;
ak ju nemožno použiť, môže ponechať posledný úspešný snapshot s jasným označením neaktuálnosti.

### Politika kompatibility {#compatibility}

V rámci `api_version: 1`:

- **Môžu pribudnúť:** operácie, voliteľné vstupy, polia odpovedí, kódy chýb, enum hodnoty a
  capability reťazce. Klienti ignorujú, čo nepoznajú.
- **Zvýši verziu:** odstránenie alebo premenovanie operácie, vstupu alebo poľa, alebo zmena typu či
  významu poľa.
- **Zúženie vstupu** je dovolené len tam, kde staré správanie odporovalo zaznamenanému rozhodnutiu
  alebo bezpečnostnému sľubu. Oznamuje sa capability reťazcom a poznámkou k migrácii v zozname zmien.
  Doterajšie zúženia: `drafts.stage amends` prijme len aditívne telá (`amend_not_additive`,
  capability `drafts.stage.amends.additive`); guard revízie odmietne `HEAD` a iné mená refov
  (`invalid_input`); `drafts.verify` spustí len pinnuté príkazy (`untrusted_verify`, capability
  `drafts.verify.pinned`); `export` bez commitnutej konfigurácie číta vstavané predvolené hodnoty
  (`config_source.kind: "defaults"`, úložisko `.ai`) namiesto lokálnej konfigurácie úložiska.

Parametre dostávajú aliasy, nikdy sa nepremenúvajú. `api.describe {}` vráti JSON Schema vstupu každej
operácie, `{"operations": {"<name>": {"input": {…}}}}`, takže klient overí, čo nainštalovaná binárka
prijíma; `api.describe {"operation": "<name>"}` vráti `{operation, input}` pre jednu. Capability
reťazce, ktoré hľadať v `status.capabilities` popri menách operácií (`api.describe` je jedna z nich):
`drafts.stage.amends`, `drafts.stage.extends`, `drafts.stage.amends.additive`,
`drafts.capture.extends`, `drafts.confirm.operation_id`, `drafts.verify.pinned`,
`verification.last_verified` a `drift.ordered`.

**Jeden pojem revízie.** Revízia je blob id: `revision` v JSON záznamu, z `entries.related` a
`entries.changed`; `entry_revision` v `drafts.extend`, `drafts.revise` a `retire` aj `amends.revision`
prijímajú blob id súboru záznamu, plný hash commitu, ktorého blob súboru je súčasný, alebo 12-hex
revíziu, ktorú vracali staršie binárky. `HEAD`, meno vetvy alebo krátky hash je `invalid_input`.
Revízia, ktorá už nesedí, je všade `stale_entry`.

**Aliasy chýb.** Zlyhanie môže niesť `error.aliases`, staršie kódy, podľa ktorých sa klient pri tej
istej situácii ešte môže vetviť: `stale_entry` uvádza `operation_conflict` a `model_unavailable` uvádza
`llm_unavailable`. Vetvi sa najprv podľa `code`, potom podľa `aliases`.


## Operácie

| Operácia | Vstup | Výsledok |
| --- | --- | --- |
| `status` | `{}` | verzia, koreň, revízia, úložisko, schopnosti a dostupnosť LLM |
| `setup` | `{}` | inicializácia znalostí bez implicitnej inštalácie hookov |
| `entries.list` | `limit?`, `offset?`, `active_only?`, `kinds?`, `tags?` | záznamy, počet, ďalší offset a snapshot |
| `entries.get` | `id` (celé/jednoznačný prefix, číslo ADR alebo cesta) | jeden záznam |
| `entries.search` | `query`, `limit?`, `active_only?` | nájdené záznamy a počet |
| `entries.governing` | `paths`, `exclude_ids?`, `limit?`, `min_specificity?` | záznamy s pravidlom, cestou a špecifickosťou; snapshot |
| `entries.export` | `revision`, `review_ref`, `paths?` | commitnuté záznamy s presným Markdownom a hashmi blobov |
| `entries.related` | `text`, `paths?`, `exclude_ids?`, `llm?` | najviac päť súvisiacich záznamov s dôvodom; s `llm` aj vzťahy podľa modelu ([podrobnosti](#related)) |
| `entries.drift` | `base?`, `head?`, `kinds?` | záznamy, ktorých riadené súbory sa v rozsahu zmenili bez pohľadu na záznam ([podrobnosti](#drift)) |
| `entries.changed` | `base`, `head`, `kinds?` | commitnuté záznamy pridané, upravené, nahradené alebo vyradené medzi dvoma commitmi ([podrobnosti](#changed)) |
| `inbox.list` | `state?`: `pending` (predvolene), `dismissed`, `promoted`, `all` | kandidáti a zachovaná lokálna história |
| `inbox.drop` | `id`, `reason?` | zamietnutie lokálneho kandidáta |
| `inbox.verify` | `id`, `verify` (`""` ho odstráni) | `id`, `verify` a `entry`: návrh s príkazom, ktorý ho dokázal; povýšenie ho zachová ([podrobnosti](#inbox-verify)) |
| `drafts.capture` | `entry`, `source?`, `operation_id?` | kandidát v inboxe, poznámky, počet redakcií, `evidence` a `related_entries` |
| `drafts.stage` | `entry` alebo `candidate_id`, `operation_id`, `source?`, `requested_path?`, `amends?` ([podrobnosti](#amend)) | záznam, cesta, staging, poznámky, príznak opakovania, `evidence`, `related_entries` a `resolved_challenges` |
| `drafts.retire` | `operation_id`, `entry_id`, `entry_revision`, `reason` | záznam stagnutý so stavom `deprecated` a poznámkou `## Retired` ([podrobnosti](#retire)) |
| `drafts.confirm` | `id` alebo `ids`, `because?`, `kind?`, `operation_id?`; alebo `repair: true` | `id`, `path`, `checked` a `ledger`: záznam potvrdený teraz, jeho riadok v denníku, oboje stagnuté ([podrobnosti](#confirm)) |
| `drafts.verify` | `id`, `because?` | výsledok príkazu `verify` záznamu, zapísaný do denníka a stagnutý ([podrobnosti](#verify)) |
| `drafts.revise` | `entry_id`, `entry_revision`, `objections`, `llm?` | návrh, ktorý záznam nahrádza; nič nezapisuje ([podrobnosti](#revise)) |
| `drafts.extend` | `entry_id`, `entry_revision`, `text`, `llm?` | záznam s poznatkom doplneným do tela, rovnaké id aj súbor; nič nezapisuje ([podrobnosti](#extend)) |
| `drafts.extract` | `text`, `source?`, `llm?`, `max?` | extrahovaní kandidáti bez zápisu |
| `drafts.formalize` | `entry`, `sources?`, `context?`, `llm?` | formulovaný záznam, poznámky a `related_entries` bez zápisu |
| `guidance.propose` | `kept`, `dismissed`, `missed`, `current?`, `profile_tag?`, `llm?` | navrhnutá konvencia bez stagingu |
| `operations.get` | `operation_id`, `fingerprint?` | pôvodný výsledok dokončeného zápisu alebo chyba nedokončenej/konfliktnej operácie |

Objekt `entry` obsahuje `kind`, `title`, `body` a voliteľné `summary`, `paths`, `tags`, `status`,
`supersedes`, `extends`, `visibility`, `commits`, `review_by`, `verify` a `evidence` ([nižšie](#maintenance)). `verify` je príkaz, ktorý
záznam dokázal: jeden riadok s najviac 500 znakmi, inak `invalid_input`. Cesty zostávajú po celý čas poľom. Formulovanie vybraného
poznatku je oddelené od hľadania nových poznatkov, aby deduplikácia neodstránila to, čo človek
výslovne vybral. Extrakcia a návrh konvencie vyžadujú použiteľný LLM; formulovanie bez vybraného
modelu môže zachovať dodaný poznatok.

Načítaný záznam má `id`, `kind`, `title`, `body`, `summary`, `status`, `date`, `review_by`, `checked`, `verify`, `paths`, `tags`, `file`,
`markdown`, `style`, polia nahradenia a dodatočné metadáta (zdroje sú v `extra.external_refs`). `entries.list`, `entries.get`,
`entries.search`, `entries.governing` a `entries.drift` pridávajú `verification`, `since` a `premises` ([nižšie](#verification)). Parser a párovanie vlastní engine;
klienti nemajú vytvárať druhý parser alebo matcher. `active_only` vynechá nahradené, zastarané a
zamietnuté záznamy. Neznamená, že návrh z pracovného stromu prešiel review.
Zoznam vracia predvolene 100 záznamov (najviac 500); pokračujte cez `next_offset`, kým nie je null.
Snapshot sa zmení aj pri úprave obsahu bez zmeny počtu záznamov.

## Údržba znalostí {#maintenance}

Pribudlo vo verzii 0.3.0. `status.capabilities` obsahuje `entries.related`, `drafts.retire` a
`drafts.revise`, keď ich binárka podporuje; overte to pred volaním.

**Termín kontroly.** `entry.review_by` (`YYYY-MM-DD`) hovorí, kedy sa má niekto na záznam znova
pozrieť, napríklad na pravidlo, ktoré platí počas migrácie. Nie je to koniec platnosti: po termíne je
záznam označený (`review due` vo vyhľadávaní, v briefe a v `kontext status`), nikdy nie skrytý. Zlý
dátum alebo dátum pred `date` záznamu je `invalid_input`. Druh ho môže dopĺňať cez
[`review_after`](./03-configuration.md#store-kinds); bez toho záznamy termín nemajú.

**Evidencia.** `entry.evidence` je zoznam `{ref, quote?}` (najviac 50). Tvary odkazov sú `pr:<id>`,
`issue:<id>`, `task:<id>`, `code:<repo>@<rev>:<cesta>#L<a>-<b>` (rozsah riadkov je voliteľný) a
`url:<http(s) URL>`; čokoľvek iné je `invalid_input`. Odkazy sa uložia do `extra.external_refs`, za
odkazy, ktoré tam klient dal sám. Citát (`quote`, najviac 200 znakov) pri odkaze `code:` do tohto
repozitára sa overí cez `git show <rev>:<cesta>`; každý výsledok vracia odkazy ako
`evidence: [{ref, kind, quote?, verification}]`, kde `verification` je `verified`, `quote_not_found`,
`unverifiable` (iný repozitár, neznáma revízia alebo odkaz, ktorý nie je kód) alebo `none` (bez
citátu). Nič sa neodmieta preto, že sa nedá overiť: citované odkazy a ich výsledok idú do sekcie
`## Evidence` v tele záznamu, kde ich číta reviewer. Osobné údaje do odkazov nepatria: e-maily a id
sessions nemajú v Gite čo robiť.

### Súvisiace záznamy {#related}

`entries.related {text, paths?, exclude_ids?, llm?}` odpovie, ako text súvisí so zaznamenanými
záznamami. Bez modelu vráti `candidates`: najviac päť aktívnych záznamov s podobným znením (lokálne
BM25, blízko najlepšieho zásahu) alebo s prekrývajúcimi sa `paths`, každý ako
`{id, title, kind, status?, reason, label, revision}` s `label: "related, check"`. `revision` je krátky
hash Markdownu záznamu: klient ho vráti ako `entry_revision` pri `drafts.extend` alebo `amends.revision`
pri `drafts.stage`. S `llm: true` (alebo menom
adaptéra) sa model raz opýta otázkou, v ktorej je vzťah aj cieľ spolu; `relations` je zoznam
`{relation, id?, title?, label}`, kde `relation` je `new`, `same`, `extends`, `conflicts` alebo
`uncertain`, a platiť ich môže viac. Odpoveď o zázname, ktorý model nevidel, sa zahodí; ak nič
neostane, odpoveď je `uncertain`. Štítky znejú `possible conflict, check`, `possibly the same, check`,
`extends it, check` — rozhoduje človek; nič nenavrhuje ani nepredvypĺňa `supersedes`.
`model: {asked, cached}` hovorí, či sa model volal. Odpoveď sa ukladá do lokálneho stavu worktree
podľa hashu textu a obsahu porovnaných záznamov, takže `drafts.capture`, `drafts.stage` a
`drafts.formalize`, ktoré vracajú rovnaký objekt ako `related_entries`, použijú uloženú odpoveď a
model samy nevolajú. Bez nakonfigurovaného modelu skončí `llm: true` chybou `model_unavailable`.

### Vyradenie záznamu {#retire}

```json
{"operation_id":"retire-42","entry_id":"2026-01-10-use-cron","entry_revision":"<publication.revision>","reason":"Exporty bežia od septembra cez frontu."}
```

`drafts.retire` zmení vlastný súbor záznamu: stav `deprecated` a poznámku `## Retired` s dátumom a
dôvodom, a stagne ho. Nič iné sa neprepisuje; pôvodné tvrdenie ostáva v Gite čitateľné.
`entry_revision` je commit, v ktorom volajúci záznam čítal (`publication.revision` z `entries.get`),
alebo blob záznamu (`publication.blob_hash`); neskorší commit, ktorý súbor nezmenil, ju nerobí zastaranou.
Operácia skončí `operation_conflict`, keď sa súbor v tejto revízii líši od HEAD, keď má lokálne alebo
stagnuté zmeny, alebo keď je záznam už neaktívny. Vyžaduje `operation_id`, ukladá potvrdenie a
opakuje sa ako `drafts.stage` (`replayed: true`, `operations.get`). Otvorené námietky tohto worktree
voči záznamu tým vybaví a vráti ich v `resolved_challenges`. Vyradenie aj nahradenie fungujú pre
každý druh, nielen pre rozhodnutia.

Námietka je spôsob, ako agent povie, že záznam už neplatí, bez toho, aby ho menil: `ctx_challenge`
(alebo `kontext challenge`) zapíše do inboxu kandidáta druhu `challenge` s `extra.against` (id
záznamu), `extra.against_revision` (commit) a `extra.against_blob`. `inbox.list` ho vráti; ako záznam
sa stagnúť nedá. Vybaví ho vyradenie alebo stagnutá náhrada, ktorá záznam nahrádza (`supersedes`).

### Drift {#drift}

Pribudol v 0.4.0 (`status.capabilities` uvádza `entries.drift`).

```json
{"base": "origin/main", "head": "HEAD", "kinds": ["decision", "convention"]}
```

Všetky polia sú voliteľné. `base` je git revízia; bez nej merge base `HEAD` s `origin/HEAD` (inak
`origin/main`, inak `origin/master`), inak s upstreamom vetvy, inak s predvolenou vetvou
`checkout.defaultRemote`, a ak nie je, `HEAD`. `head` je git revízia; bez nej sa počíta
aj pracovný strom: commity `base..HEAD` plus necommitnuté a nesledované zmeny (pracovná kópia jj je
z pohľadu gitu necommitnutá). `kinds` je predvolene `["decision","convention","learning","incident"]`;
architektúra sa kontroluje, len keď ju pomenuješ. Neznáma revízia alebo revízia začínajúca `-` je
`invalid_input`; repozitár bez commitov je `no_commits`.

```json
{"base": "<full sha>", "head": "<full sha>" | "working", "entries": [
  {"id": "…", "kind": "convention", "title": "…", "path": ".ai/conventions/….md", "date": "2026-01-10", "checked": "2026-03-01",
   "changed": ["src/billing/invoice.rs"], "commits": [{"sha": "<full sha>", "subject": "feat(billing): …"}]}
]}
```

Záznam je vo výsledku, keď je aktívny, má `paths`, aspoň jeden súbor zmenený v rozsahu (okrem
súborov znalostí) im zodpovedá — rovnakým porovnaním ako v `entries.governing` — a vlastný súbor
záznamu sa v tom istom rozsahu **nezmenil**: úprava, vyradenie aj potvrdenie sa rátajú ako pohľad naň.
`date` a `checked` sú tam, keď ich záznam má; `verification`, `since`, `verify` a `premises` sa pridajú ako pri
ostatných čítaniach ([nižšie](#verification)). `commits` sú ne-merge commity z `base..head`, ktoré
sa dotkli zodpovedajúcich súborov, najnovšie prvé; keď je zmena len necommitnutá, je prázdne. S
explicitným `head` sa záznamy čítajú tak, ako sú commitnuté v `head`. Záznamy sú zoradené podľa ID.

Pribudlo v 0.10.0 (capability `drift.ordered`): výsledok nesie `base_source`, odkiaľ základ pochádza:
`explicit` (vstup `base`), `origin-head`, `origin-main` (aj `origin/master`), `upstream`,
`default-remote` (`checkout.defaultRemote`), alebo `fallback-none`, keď sa nič nevyriešilo a základom je
samotný `HEAD` (žiadny remote, plytký klon). Pri `fallback-none` je rozsah prázdny a výsledok nesie
`warning` s dôvodom; `kontext drift --fail` a `kontext ci --strict` skončia kódom 3 namiesto hlásenia,
že drift nie je. Záznam sa ráta ako pozretý,
len keď je pohľad (úprava, potvrdenie alebo prejdená kontrola) v poslednom commite rozsahu, ktorý
zmenil jeho riadené súbory, alebo po ňom; pohľad pred touto zmenou ju už nepokrýva.
Na odpovedi sa nezúčastňuje žiadny model.

### Potvrdenie záznamu {#confirm}

`drafts.confirm {id}` zaznamená, že záznam stále platí: nastaví `checked` na aktuálny čas v UTC
na sekundu (`YYYY-MM-DDTHH:MM:SSZ`) v súbore záznamu, nič iné nezmení ani o bajt, pridá riadok do
[denníka overení](../concepts/03-capture-and-review.md#verifications) (`.ai/confirmations.log`) a oboje
stagne ako `drafts.retire`. Výsledok je `{id, path, checked, ledger}` s tým istým reťazcom v `checked` aj `ledger.at`; jeho prvých desať znakov je
deň, takže porovnanie s `YYYY-MM-DD` ako reťazcom ostáva správne. Ďalšie potvrdenie neskôr v ten istý
deň súbor zmení, takže drift nový pohľad uvidí. Ručne zapísaný `checked` môže byť aj samotný
`YYYY-MM-DD`; načítané záznamy ho vrátia tak, ako je zapísaný. Neznámy záznam je `not_found`;
neaktívny, ešte necommitnutý alebo súbor s nestagnutými úpravami je `operation_conflict`. `checked`
majú načítané záznamy aj [`CommittedEntry`](#rust-embedding) vo vkladacej fasáde, takže aplikácia
spozná potvrdenie v posúdenej revízii.

Pridané po 0.5.0, všetko nepovinné:

- `because`: jeden riadok (najviac 500 znakov), ktorý ostane v denníku.
- `kind`: `read`, keď záznam v klientovi potvrdil človek, `agent` (predvolene), keď agent. Viac ako
  päť ID v jednom volaní sa zapíše ako `bulk` bez ohľadu na `kind`.
- `ids`: viac záznamov jedným volaním. Výsledok je potom `{confirmed: [{id, path, checked, ledger}],
  failed: [{id, code, message}]}`.
- `repair: true` (samotné): prepíše `checked` každého aktívneho záznamu, ktorého súbor zaostáva za
  najnovším potvrdením v denníku, a súbory stagne. Výsledok je `{repaired: [{id, path, from, to}],
  skipped: [{id, path, reason}]}`.
- `operation_id` (0.10.0, capability `drafts.confirm.operation_id`): opakované volanie s tým istým ID
  vráti prvý výsledok s `replayed: true` namiesto druhého riadku v denníku.

`ledger` je `{at, kind, commit, because, path}`: čas riadku, jeho druh, `HEAD` (pred prvým commitom null),
dôvod (bez neho null) a cesta k denníku.

### Spustenie kontroly záznamu {#verify}

`drafts.verify {id, because?}` (pridané po 0.5.0) spustí príkaz `verify` záznamu cez `sh -c` z koreňa
repozitára so zatvoreným vstupom, zastaví ho po `KONTEXT_VERIFY_TIMEOUT_SECS` (predvolene 120), potom
pridá riadok `verify` s výsledkom a denník stagne:

```json
{"id": "…", "command": "cargo test -p billing cents", "command_sha256": "<hex>", "ran": true,
 "passed": true, "exit_code": 0, "timed_out": false,
 "output_tail": "…posledné 2 KB stdout a stderr…", "at": "2026-10-27T09:40:02Z", "commit": "<celý sha>",
 "dirty": false, "dirty_paths": [], "path": ".ai/confirmations.log"}
```

Pribudlo v 0.10.0: `ran`, `command_sha256`, `dirty` a `dirty_paths` (necommitnuté zmeny súboru
záznamu alebo jeho riadených ciest v čase, keď príkaz bežal).

`exit_code` je null, keď bol príkaz zastavený. Záznam bez `verify` je `invalid_input`; neaktívny je
`operation_conflict` a jeho príkaz sa nikdy nespustí; neznámy je `not_found`. Príkaz beží bez zámku
zápisu, takže ostatné zápisy čakajú len na pridanie riadku.

Príkaz beží, len keď je jeho text pinnutý v používateľovom `trust.toml` (`kontext trust --verify <id>`
z terminálu; žiadna operácia nepinuje; capability `drafts.verify.pinned`). Inak nebeží nič a chyba je
`untrusted_verify` s týmito poliami popri `code` a `message`:

```json
{"code": "untrusted_verify", "message": "…", "id": "…", "command": "cargo test -p billing cents",
 "command_sha256": "<hex>", "hint": "kontext trust --verify <id>"}
```

Príkaz ukáž človeku, neopakuj volanie. Príkaz dostane povolený zoznam premenných prostredia (`PATH`,
`HOME`, `USER`, `LANG`, `LC_*`, `TMPDIR`, `TERM` a používateľov `[verify] pass_env`), jeho skupina
procesov sa zabije pri timeoute alebo 2 s po skončení shellu s ešte otvoreným stdout a `output_tail`
sa rediguje. Keď bol strom špinavý, dôvod v riadku denníka končí `[dirty: <cesty>]`. Pozri
[Bezpečnosť](../concepts/08-security.md#verify).

### Overenie, čo sa odvtedy zmenilo, predpoklady {#verification}

Pridané po 0.5.0. Každý záznam, ktorý vrátia `entries.list`, `entries.get`, `entries.search`,
`entries.governing` a `entries.drift`, nesie:

```json
{"checked": "2026-10-27T09:12:44Z",
 "verify": "cargo test -p billing cents",
 "verification": {"at": "2026-10-27T09:12:44Z", "kind": "read", "commit": "<celý sha>", "by": "Jana Nová",
                  "because": "overené voči zmene DPH", "passed": null, "source": "ledger", "verified": true,
                  "landed": "<celý sha>"},
 "since": {"commits": 3, "head": "<celý sha>", "paths": ["src/billing/invoice.rs"],
           "recent": [{"sha": "<celý sha>", "subject": "feat(billing): VAT per line"}]},
 "premises": [{"id": "2026-09-01-prices-are-floats", "status": "superseded", "superseded_by": ["2026-09-28-prices-are-integer-cents"]}]}
```

- `verification` je novší z najnovšieho riadku záznamu v denníku a jeho poľa `checked`; null, keď nemá
  ani jedno. `kind` je `read`, `agent`, `bulk` alebo `verify`; `passed` je pri `verify` boolean, inak
  null; `source` je `ledger`, alebo `frontmatter` pre `checked` bez novšieho riadku v denníku (vtedy je
  `kind` `read` a `commit`, `by` a `because` sú null). `verified` je true pri `read` a pri prejdenom
  `verify`: riadky `agent` a `bulk` sú pečiatky.
- `verification.landed` (pridané po 0.6.0) je celé ID commitu v histórii `HEAD`, ktorý ako prvý pridal
  najnovší riadok záznamu v denníku, alebo null, kým je ten riadok len stagnutý alebo v pracovnom strome:
  „prešlo a je commitnuté“ oproti „prešlo, ešte necommitnuté“. Pri `source: "frontmatter"` je to
  posledný commit v histórii `HEAD`, ktorý zmenil súbor záznamu, null, kým má súbor necommitnuté zmeny
  alebo nebol nikdy commitnutý. `since` sa nemení: už počíta od neskoršieho z `commit` a commitu, ktorý
  riadok priniesol.
- `checked` ostáva reťazcom: novší z poľa v súbore a najnovšieho potvrdenia záznamu v denníku (riadok
  `verify` nie je potvrdenie).
- `since` počíta ne-merge commity na riadených súboroch záznamu (bez súborov znalostí) po overení: po
  commite, ktorý uviedlo, alebo po commite, ktorý priniesol jeho riadok denníka, ak je novší; podľa času,
  keď ani jeden nie je v histórii `HEAD`; pri `checked` bez riadku v denníku tak, ako počíta aktuálnosť.
  `paths` sú zmenené riadené súbory, zoradené, najviac 20; `recent` najnovšie z commitov, najviac päť;
  `head` je `HEAD`. Null pri zázname, ktorý nikto neoveril, pri neaktívnom a pri zázname bez `paths`.
- `verify` je príkaz záznamu alebo null.
- `premises` uvádza záznamy, na ktoré tento aktívny záznam odkazuje (`[[id]]` v tele, `supersedes`,
  pole `extends`) a ktoré sú nahradené, vyradené alebo zastarané, okrem toho, ktorý sám nahradil.
  Prázdne, keď také nie sú. V žiadnom zázname sa nič neprepíše.

Pribudlo v 0.10.0 (capability `verification.last_verified`), popri poliach vyššie:

- `verification.kind` môže byť aj `cache`: dátum `checked` bez riadku v denníku. Hlási
  `verified: false`; `kontext confirm --migrate-legacy` pre také záznamy zapíše riadky `bulk`.
  `checked` sa ďalej zapisuje, lebo ho klienti čítajú.
- `verification.by` je `application:<meno>` pri `read` zapísanom cez `kontext api`, aby sa dal odlíšiť
  od čítania na termináli.
- `last_verified` je najnovší pohľad, ktorý sa ráta ako overenie (`read` alebo prejdený `verify`), v
  tvare `verification`, alebo null; `since_verified` od neho počíta commity tak ako `since`.
- `failing: true`, keď najnovší riadok `verify` zlyhal. Zlyhaná kontrola sa ukáže, ale nie je pohľad:
  nenahradí skorší pohľad ani nevynuluje `since`.
- `amended_since: true`, keď sa telo záznamu zmenilo po jeho poslednom overujúcom pohľade.
- `since` počíta podľa predkov: ne-merge commity na riadených cestách dosiahnuteľné z `HEAD` a nie z
  commitu pohľadu, takže sa zaráta aj vetva zlúčená po potvrdení. Čas sa použije len bez takého
  commitu. `since.truncated: true`, keď commit pohľadu leží za `index.max_commits` prečítanými
  commitmi, takže sa zaráta každý prečítaný commit; `since.lower_bound: true` v plytkom klone. Obe sú
  prítomné, len keď sú true, aj pri `since_verified`.

Jeden `git log` a jeden `git log -p` denníka pokryjú všetky záznamy, uložené v cache podľa `HEAD`; zoznam
stojí jeden `rev-parse`, kým sa nič necommitne. `landed` pridá pre každý `HEAD` jeden `git log` súborov,
ktorých overenie je len dátum `checked`, a pri každom čítaní `git status` týchto súborov. `snapshot`
pracovného stromu sa zmení, keď sa zmení denník.

### Doplnenie dôkazu návrhu {#inbox-verify}

`inbox.verify {id, verify}` nastaví alebo nahradí príkaz `verify` čakajúceho návrhu v inboxe, ktorý
zachytil tento worktree; takto sa doplní návrh zachytený bez príkazu, ktorý ho dokázal, bez nového
zachytenia. Prázdny reťazec príkaz odstráni. Príkaz sa overuje ako pole záznamu (`invalid_input`);
neznámy návrh je `not_found`, návrh iného aktívneho worktree `worktree_conflict`, námietka
`invalid_input`. Povýšenie (`drafts.stage` s `candidate_id`, `ctx_prepare_commit`, `kontext promote`) ho
zachová.

```json
{"id": "2026-10-27-cache-keys-carry-the-tenant", "verify": "cargo test cache_key",
 "entry": {"id": "2026-10-27-cache-keys-carry-the-tenant", "verify": "cargo test cache_key", "publication": {"state": "local"}, "…": "…"}}
```

### Návrh náhrady {#revise}

`drafts.revise {entry_id, entry_revision, objections, llm?}` požiada model o náhradu, ktorá odpovedá
na námietky (1 až 20 reťazcov, každý najviac 300 znakov). Nič nezapisuje. Návrh si zachová druh,
cesty a tagy záznamu, nastaví `supersedes: [entry_id]` a pri rozhodnutí má stav `proposed`; klient ho
ukáže človeku a stagne cez `drafts.stage`. Zastaraná `entry_revision` alebo zmenený súbor je
`operation_conflict`, chýbajúci model `model_unavailable` a model, ktorý vráti staré telo nezmenené,
`invalid_model_output`.

### Doplnenie záznamu {#extend}

Pribudlo vo verzii 0.9.0 (`status.capabilities` uvádza `drafts.extend`, `drafts.stage.amends` a
`drafts.stage.extends`).

```json
{"entry_id":"2026-10-20-relay-tests-fake-the-node","entry_revision":"<revision kandidáta>","text":"Falošný server patrí za uzol.","llm":true}
```

`drafts.extend {entry_id, entry_revision, text, llm?}` pripraví návrh záznamu s doplneným poznatkom.
Nič nezapisuje. Výsledok je `{entry, old_body, as_written, notes, amends}`: `entry` si zachová id, súbor,
druh, cesty, tagy aj ostatné polia a má nové telo; `old_body` je telo, ktoré nahrádza; `amends` je
`{id, revision}` pre `drafts.stage`. Poznatok sa vždy pridá doslovne pod `## Added <YYYY-MM-DD>` a
`as_written` je true; model nikdy nepíše do záznamu. `llm` sa ďalej prijíma, aby existujúci klienti
fungovali, a `notes` povie, že poznatok bol pridaný doslovne. Tvar výsledku sa nemení. `text` má najviac
4000 znakov.

Dopĺňajú sa len konvencie, poznatky (learning) a incidenty. Rozhodnutie (alebo `adr`) sa odmietne s
`decision_not_amended`: rozhodnutie sa mení len nahradením, preto sa poznatok, ktorý na ňom stavia,
uloží ako nový záznam, ktorý ho `extends`. Iný druh je `invalid_input`, neaktívny záznam
`operation_conflict`. `entry_revision` je `revision` kandidáta z `entries.related` (blob id súboru) alebo
plný hash commitu s rovnakým obsahom súboru; `HEAD`, meno vetvy alebo krátky hash je `invalid_input`
a revízia, ktorá už nesedí, je `stale_entry` (porovnajte znova). Súbor s lokálnymi, stagnutými alebo
necommitnutými zmenami je `operation_conflict`.

### Doplnenie a prepojenie pri stagingu {#amend}

```json
{"operation_id":"amend-42","amends":{"id":"2026-10-20-relay-tests-fake-the-node","revision":"<revision kandidáta>"},"entry":{"body":"<telo z drafts.extend>"}}
```

`drafts.stage` s `amends: {id, revision}` zapíše `entry.body` do vlastného súboru záznamu a stagne ho;
zvyšok `entry` sa ignoruje. Všetko pred telom (frontmatter alebo polia ADR, nadpis) ostane bajt po
bajte, takže diff v review je len telo. Revízia sa overí znova ako pri `drafts.extend`
(`stale_entry`, `decision_not_amended`, `operation_conflict`); telo zhodné so súčasným je
`invalid_input`. Doplnenie len pridáva: súčasné telo musí byť bajtovým prefixom `entry.body` a zvyšok
musia byť sekcie `## Added <YYYY-MM-DD>`; čokoľvek iné je `amend_not_additive` s odkazom na nahradenie.
Po spojení sa záznam znova parsuje a názov aj všetky polia mimo tela musia ostať nezmenené. Jediné
úpravy reviewovaného textu na mieste, ktoré kontext robí, sú metadátové riadky: `verify`, `checked` a
`status` / `superseded_by` pri vyradení, nahradení a `kontext repair`. `amends` spolu so `supersedes` alebo `extends` v tom istom návrhu, alebo s
`candidate_id`, je `invalid_input`. Výsledok je bežný výsledok stagingu s `amended: <id>`; námietky
voči záznamu ostávajú otvorené, doplnenie na ne neodpovedá. Vyžaduje `operation_id` a opakuje sa ako
každý staging.

`entry.extends: [<ids>]` pri novom zázname ho prepojí so záznamami, na ktorých stavia: id (celé id,
jednoznačný prefix, číslo ADR alebo cesta) sa vyhodnotia a zapíšu do frontmatteru ako
`extends: [...]`, najviac 20. Id, ktoré úložisko nemá, je `unknown_entry`. Keď sa niektorý z nich
neskôr nahradí alebo vyradí, označí sa zmenený predpoklad ([overenie](#verification)). V `extra` sa
`extends` neprijíma.

### Zmenené záznamy {#changed}

Pribudlo vo verzii 0.9.0 (`status.capabilities` uvádza `entries.changed`).

`entries.changed {base, head, kinds?}` porovná záznamy commitnuté v dvoch revíziách, bez modelu, a vráti
`{base, head, changes}` s vyhodnotenými commitmi. Každá zmena je
`{id, kind, title, file, paths, revision, change, superseded_by?}`, zoradené podľa id, kde `change` je:

- `added`: id v `base` nie je;
- `revised`: rovnaké id s iným obsahom (doplnený záznam); samotná zmena `checked` je pohľad, nie
  úprava, a vynechá sa;
- `superseded`: aktívny v `base`, v `head` nahradený; `superseded_by` uvádza záznamy v `head`, ktoré
  ho nahrádzajú;
- `deprecated`: aktívny v `base`, potom vyradený (deprecated, retired, rejected) alebo odstránený
  (`removed: true`).

`revision` je revízia záznamu v `head` (pri odstránenom v `base`) v tvare, aký dáva `entries.related`.
`kinds` sú predvolene rozhodnutia, konvencie, poznatky a incidenty. Obe strany čítajú commitnuté
stromy: pracovný strom ani stagnuté súbory do odpovede nepatria.

Pribudlo v 0.10.0: každá zmena nesie aj `status` (stav záznamu v `head`, pri odstránenom v `base`) a
každá strana sa číta s konfiguráciou commitnutou v danej revízii, takže druh presunutý do iného
adresára medzi `base` a `head` sa nehlási ako odstránený a pridaný.

Commitnuté záznamy v každom čítaní (`CommittedEntry`) nesú popri skorších poliach aj `date`, `verify`,
`review_by`, `supersedes`, `commits` a `author`.

## Vloženie do Rust aplikácie {#rust-embedding}

Crate `kontext` sprístupňuje `application::request(&Path, operation, &input)` s rovnakou JSON
odpoveďou. Interné moduly enginu zostávajú súkromné. Služba overujúca vlastný Git mirror môže použiť
`application::KnowledgePolicy` bez spúšťania binárky:

```rust
use kontext::application::KnowledgePolicy;

let policy = KnowledgePolicy::from_config(".ai", Some(reviewed_config_text))?;
let entry = policy.inspect("docs/adr/0001-storage.md", verified_blob_text)?;
```

`inspect` vráti `CommittedEntry` s `id`, `kind`, `title`, `body`, `summary`, `status`, `checked`,
`paths`, `tags`, `file` a ostatnými metadátami v `extra`.
`config_candidates` určuje poradie hľadania konfigurácie v commite. Volajúci načíta jej obsah z
overenej revízie a cez `inspect` vyhodnotí nakonfigurované druhy/cesty, frontmatter či ADR štýl a
platnosť tímového poznatku. Rozhodnutie musí byť výslovne prijaté; súkromný, navrhovaný, zamietnutý
či nahradený záznam importom nezíska autoritu. Overenie nečíta súbory a nespúšťa modely/adaptéry.
Volajúci nezávisle dokazuje pôvod, aktuálny review ref, celú revíziu, bežný súbor a presný blob/hash.
Objekt rozparsovaný klientom nenahrádza tento dôkaz. Pri použití Git zdroja pripnite revíziu cratu.

Archívy vydaní obsahujú binárku a manifest verzie API, platformy a SHA-256 pre jej pribalenie do
aplikácie aj pri balení pre inú platformu. Ciele vydaní pokrývajú macOS ARM64/x64, Linux ARM64/x64 a
Windows x64. Volanie aplikačného API samo osebe nevydáva verziu ani nič nesťahuje.

## Platnosť a doručovanie

`entries.governing` používa spoločný matcher kontextu. Výsledok vysvetľuje, ktoré pravidlo sa zhoduje
s ktorým súborom. Špecifickosť umožňuje úzke upozornenia (`min_specificity: 3`), zatiaľ čo širšie
pravidlá zostávajú v bežnom briefe/indexe. Načasovanie, už doručené ID a text upozornenia patria
aplikácii. Trvalé MCP spojenie nemusí spúšťať nový proces pri každom dotaze; výsledky možno cachovať
podľa snapshotu.

Každá operácia môže vybrať iný worktree alebo repozitár cez vstupné `dir`. MCP prijíma `dir` aj vedľa
`operation`. Zápis s cestami v zázname používa bežné smerovanie podľa vlastníka repozitára. Lokálne
potvrdenia operácií patria cieľovému worktree; nesmú sa použiť na zápis do iného checkoutu.

## Staging a bezpečné opakovanie

```json
{
  "operation_id": "external-candidate-42",
  "source": "external:task/42@v7",
  "entry": {
    "kind": "decision",
    "title": "Sumy používajú celé centy",
    "body": "Celé centy predchádzajú nepresnému zaokrúhľovaniu.",
    "paths": ["src/pricing/**"],
    "status": "proposed"
  }
}
```

Logický zápis musí mať stabilné ID operácie. Opakovanie po reštarte klienta či chybe vzdialenej
fronty vráti pôvodný výsledok s `replayed: true`, aj po neskoršej legitímnej úprave toho istého
záznamu: replay vráti uložený receipt a nikdy znova nestagne. Rovnaké ID s odlišným vstupom je
konflikt. `operations.get` nájde aj receipt zachytenia, ktoré bolo routované do úložiska iného
repozitára.
Engine koordinuje súbežné aplikačné zápisy a lokálne ukladá plán aj výsledok operácie. Zosúladenie
so vzdialenou frontou rieši klient: lokálny Git a vzdialená služba netvoria jednu transakciu.

Ak model pred stagingom preformuluje poznatok, dodajte `fingerprint` pri `drafts.stage`: stabilný hash
pôvodného kandidáta, nie vytvoreného znenia. Pred opakovaným volaním modelu overte `operations.get`
s rovnakým ID a fingerprintom. Vráti uložený výsledok, pri úprave kandidáta `operation_conflict` a pri
uloženom nedokončenom pláne `operation_incomplete`. Plán obnovíte cez `drafts.stage` obsahujúci iba
`operation_id` a rovnaký `fingerprint`; engine použije uložený záznam. Chýbajúce potvrdenie vracia
`not_found`, až vtedy vytvorte nový plán.

Staging zachová nesúvisiaci obsah indexu, nevpustí súkromné zachytenia do tímového úložiska a odmietne
konfliktný cieľ. Vyžiadaná cesta musí patriť nakonfigurovanému úložisku. Pripravený záznam stále
podlieha bežnému commitu a review.

CLI aj aplikácie používajú spoločný zámok zápisov. Potvrdenia operácií a inbox/história sú lokálny
stav workflow, odlišný od obnoviteľnej cache vyhľadávania; ponechajte ich počas možného opakovania.
Ak `requested_path` vyberá nakonfigurovaný adresár druhu, staging vie odvodiť chýbajúce `entry.kind`
z tejto konfigurácie. Klienti nemajú druh odhadovať z pevne určených názvov priečinkov.

## Dôkaz publikovania a export

`status` záznamu opisuje obsah; dôkaz publikovania hovorí, kde tento obsah existuje. Pracovné čítanie
rozlišuje lokálne návrhy, upravené súbory, staging a commitnutý obsah. Žiadny z týchto stavov sám
osebe nedokazuje tímové schválenie. `publication.revision` je commit, ktorý čítanie videlo
(HEAD, rovnaký pre všetky záznamy); `publication.blob_hash` je Git blob vlastného súboru záznamu
(null, kým má súbor nestagnuté úpravy alebo nie je sledovaný), takže sa zmení, len keď sa zmení ten
záznam. Podľa neho klient zistí, či sa záznam zmenil, odkedy ho naposledy ukázal.

Pri exporte dodajte celý SHA commitu a celý lokálny review ref, napríklad `refs/remotes/origin/main`.
Revízia sa musí rovnať aktuálnemu tipu tohto refu. Export číta commitnuté bloby, nie checkout,
vynechá navrhované/neaktívne záznamy a vráti presný Markdown s hashmi. Lokálne úpravy súborov ani
zdieľanej konfigurácie nenahradia commitnutú konfiguráciu exportu. Pôvod konfigurácie odpoveď označí
v `config_source`. Bez commitnutej konfigurácie v revízii je `config_source.kind` `defaults` a adresár
úložiska `.ai`; lokálna konfigurácia úložiska sa pre export nikdy nepoužije (0.10.0; predtým
`local_configuration`).

Politiku, ktorá vetva je schválená, prípadný fetch a kontroly review u poskytovateľa Gitu určuje
volajúci. Pole accepted ani ľubovoľný `HEAD` nie sú dôkazom schválenej vetvy. Klienti majú pri importe
zachovať pôvod revízie/refu/blobu a opakovaný import musí byť idempotentný.
