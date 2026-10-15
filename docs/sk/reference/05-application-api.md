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
| `inbox.list` | `state?`: `pending` (predvolene), `dismissed`, `promoted`, `all` | kandidáti a zachovaná lokálna história |
| `inbox.drop` | `id`, `reason?` | zamietnutie lokálneho kandidáta |
| `drafts.capture` | `entry`, `source?`, `operation_id?` | kandidát v inboxe, poznámky a počet redakcií |
| `drafts.stage` | `entry` alebo `candidate_id`, `operation_id`, `source?`, `requested_path?` | záznam, cesta, staging, poznámky a príznak opakovania |
| `drafts.extract` | `text`, `source?`, `llm?`, `max?` | extrahovaní kandidáti bez zápisu |
| `drafts.formalize` | `entry`, `sources?`, `context?`, `llm?` | formulovaný záznam a poznámky bez zápisu |
| `guidance.propose` | `kept`, `dismissed`, `missed`, `current?`, `profile_tag?`, `llm?` | navrhnutá konvencia bez stagingu |
| `operations.get` | `operation_id`, `fingerprint?` | pôvodný výsledok dokončeného zápisu alebo chyba nedokončenej/konfliktnej operácie |

Objekt `entry` obsahuje `kind`, `title`, `body` a voliteľné `summary`, `paths`, `tags`, `status`,
`supersedes`, `visibility`, `commits`. Cesty zostávajú po celý čas poľom. Formulovanie vybraného
poznatku je oddelené od hľadania nových poznatkov, aby deduplikácia neodstránila to, čo človek
výslovne vybral. Extrakcia a návrh konvencie vyžadujú použiteľný LLM; formulovanie bez vybraného
modelu môže zachovať dodaný poznatok.

Načítaný záznam má `id`, `kind`, `title`, `body`, `summary`, `status`, `date`, `paths`, `tags`, `file`,
`markdown`, `style`, polia nahradenia a dodatočné metadáta. Parser a párovanie vlastní engine;
klienti nemajú vytvárať druhý parser alebo matcher. `active_only` vynechá nahradené, zastarané a
zamietnuté záznamy. Neznamená, že návrh z pracovného stromu prešiel review.
Zoznam vracia predvolene 100 záznamov (najviac 500); pokračujte cez `next_offset`, kým nie je null.
Snapshot sa zmení aj pri úprave obsahu bez zmeny počtu záznamov.

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
