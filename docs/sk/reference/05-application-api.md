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
| `inbox.list` | `state?`: `pending` (predvolene), `dismissed`, `promoted`, `all` | kandidáti a zachovaná lokálna história |
| `inbox.drop` | `id`, `reason?` | zamietnutie lokálneho kandidáta |
| `drafts.capture` | `entry`, `source?`, `operation_id?` | kandidát v inboxe, poznámky, počet redakcií, `evidence` a `related_entries` |
| `drafts.stage` | `entry` alebo `candidate_id`, `operation_id`, `source?`, `requested_path?` | záznam, cesta, staging, poznámky, príznak opakovania, `evidence`, `related_entries` a `resolved_challenges` |
| `drafts.retire` | `operation_id`, `entry_id`, `entry_revision`, `reason` | záznam stagnutý so stavom `deprecated` a poznámkou `## Retired` ([podrobnosti](#retire)) |
| `drafts.revise` | `entry_id`, `entry_revision`, `objections`, `llm?` | návrh, ktorý záznam nahrádza; nič nezapisuje ([podrobnosti](#revise)) |
| `drafts.extract` | `text`, `source?`, `llm?`, `max?` | extrahovaní kandidáti bez zápisu |
| `drafts.formalize` | `entry`, `sources?`, `context?`, `llm?` | formulovaný záznam, poznámky a `related_entries` bez zápisu |
| `guidance.propose` | `kept`, `dismissed`, `missed`, `current?`, `profile_tag?`, `llm?` | navrhnutá konvencia bez stagingu |
| `operations.get` | `operation_id`, `fingerprint?` | pôvodný výsledok dokončeného zápisu alebo chyba nedokončenej/konfliktnej operácie |

Objekt `entry` obsahuje `kind`, `title`, `body` a voliteľné `summary`, `paths`, `tags`, `status`,
`supersedes`, `visibility`, `commits`, `review_by` a `evidence` ([nižšie](#maintenance)). Cesty zostávajú po celý čas poľom. Formulovanie vybraného
poznatku je oddelené od hľadania nových poznatkov, aby deduplikácia neodstránila to, čo človek
výslovne vybral. Extrakcia a návrh konvencie vyžadujú použiteľný LLM; formulovanie bez vybraného
modelu môže zachovať dodaný poznatok.

Načítaný záznam má `id`, `kind`, `title`, `body`, `summary`, `status`, `date`, `review_by`, `paths`, `tags`, `file`,
`markdown`, `style`, polia nahradenia a dodatočné metadáta (zdroje sú v `extra.external_refs`). Parser a párovanie vlastní engine;
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
`{id, title, kind, status?, reason, label}` s `label: "related, check"`. S `llm: true` (alebo menom
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
`entry_revision` je commit, v ktorom volajúci záznam čítal (`publication.revision` z `entries.get`).
Operácia skončí `operation_conflict`, keď sa súbor v tejto revízii líši od HEAD, keď má lokálne alebo
stagnuté zmeny, alebo keď je záznam už neaktívny. Vyžaduje `operation_id`, ukladá potvrdenie a
opakuje sa ako `drafts.stage` (`replayed: true`, `operations.get`). Otvorené námietky tohto worktree
voči záznamu tým vybaví a vráti ich v `resolved_challenges`. Vyradenie aj nahradenie fungujú pre
každý druh, nielen pre rozhodnutia.

Námietka je spôsob, ako agent povie, že záznam už neplatí, bez toho, aby ho menil: `ctx_challenge`
(alebo `kontext challenge`) zapíše do inboxu kandidáta druhu `challenge` s `extra.against` (id
záznamu), `extra.against_revision` (commit) a `extra.against_blob`. `inbox.list` ho vráti; ako záznam
sa stagnúť nedá. Vybaví ho vyradenie alebo stagnutá náhrada, ktorá záznam nahrádza (`supersedes`).

### Návrh náhrady {#revise}

`drafts.revise {entry_id, entry_revision, objections, llm?}` požiada model o náhradu, ktorá odpovedá
na námietky (1 až 20 reťazcov, každý najviac 300 znakov). Nič nezapisuje. Návrh si zachová druh,
cesty a tagy záznamu, nastaví `supersedes: [entry_id]` a pri rozhodnutí má stav `proposed`; klient ho
ukáže človeku a stagne cez `drafts.stage`. Zastaraná `entry_revision` alebo zmenený súbor je
`operation_conflict`, chýbajúci model `model_unavailable` a model, ktorý vráti staré telo nezmenené,
`invalid_model_output`.

## Vloženie do Rust aplikácie

Crate `kontext` sprístupňuje `application::request(&Path, operation, &input)` s rovnakou JSON
odpoveďou. Interné moduly enginu zostávajú súkromné. Služba overujúca vlastný Git mirror môže použiť
`application::KnowledgePolicy` bez spúšťania binárky:

```rust
use kontext::application::KnowledgePolicy;

let policy = KnowledgePolicy::from_config(".ai", Some(reviewed_config_text))?;
let entry = policy.inspect("docs/adr/0001-storage.md", verified_blob_text)?;
```

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
fronty vráti pôvodný výsledok s `replayed: true`. Rovnaké ID s odlišným vstupom je konflikt.
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
osebe nedokazuje tímové schválenie.

Pri exporte dodajte celý SHA commitu a celý lokálny review ref, napríklad `refs/remotes/origin/main`.
Revízia sa musí rovnať aktuálnemu tipu tohto refu. Export číta commitnuté bloby, nie checkout,
vynechá navrhované/neaktívne záznamy a vráti presný Markdown s hashmi. Lokálne úpravy súborov ani
zdieľanej konfigurácie nenahradia commitnutú konfiguráciu exportu. Ak je potrebná lokálna
konfigurácia úložiska, odpoveď označí jej pôvod.

Politiku, ktorá vetva je schválená, prípadný fetch a kontroly review u poskytovateľa Gitu určuje
volajúci. Pole accepted ani ľubovoľný `HEAD` nie sú dôkazom schválenej vetvy. Klienti majú pri importe
zachovať pôvod revízie/refu/blobu a opakovaný import musí byť idempotentný.
