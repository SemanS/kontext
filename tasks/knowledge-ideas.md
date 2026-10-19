# Nápady pre kontext a Peer Hub

Cieľ: systém, ktorý agentovi povie presne to, čo v danej chvíli potrebuje, nič navyše, a ktorý
sa o znalosti stará aj potom, čo vzniknú: nehromadí duplikáty, nenecháva zastarané veci tiché a
pri každom zázname vie, odkiaľ pochádza. Najprv v kontexte, potom v Peeri.

Jedno pravidlo platí pre všetko: pravda je v Gite a mení sa len cez review. Nástroj si veci
všimne a povie ich agentovi alebo človeku. Nič neprepisuje ani nezlučuje sám.

Rozdelenie zodpovednosti, ktoré už existuje a ktoré treba zachovať: **kontext** vlastní
výklad znalostí a ich platnosť, **Peer** doručovanie do session a pracovný postup človeka,
**Hub** zdieľaných kandidátov a námietky.

Každý návrh má rovnakú štruktúru: Čo, Princíp, Prečo, Ako, Hotové keď, Veľkosť (S deň, M pár
dní, L týždeň a viac). Čísla v zátvorke sú z predošlej verzie tohto zoznamu.

Tento zoznam prešiel druhým posudkom. Čo sa v ňom zmenilo, je v závere.

---

## Etapa 1 — správnosť a rozpočet

Najprv opraviť, čo je dnes nepresné alebo drahé, a postaviť meradlo. Bez meradla nevieme,
či ďalší mechanizmus pomáha.

### 1.1 Autorita záznamu je vidieť aj v krátkom výstupe

**Čo:** Brief a vyhľadávanie rozlišujú návrh, prijaté rozhodnutie a lokálnu neschválenú zmenu.

**Princíp:** Agent nesmie brať návrh za prijaté pravidlo. Krátky výstup musí niesť stav.

**Prečo:** `is_active()` v `src/store.rs:99` počíta `proposed` za aktívne, a riadok rozhodnutia v
briefe (`src/ops.rs:922`) stav neuvádza. Navrhnuté rozhodnutie vyzerá ako prijaté. Peer to v indexe
rozlišuje (`knowledgeTitleLine` pridáva „(proposed)“), kontext nie.

**Ako:**
1. V riadku rozhodnutia v briefe pridať značku, keď stav nie je `accepted`: `(proposed)`,
   `(deprecated)` a podobne. Prijaté bez značky, aby sa nemíňali tokeny.
2. To isté v `model::render` pre výsledky hľadania (`Hit.status` už existuje).
3. Záznam, ktorý je v pracovnom strome zmenený alebo iba staged (`publication.state`), označiť
   ako „lokálne, neschválené“ tam, kde ho brief ukazuje.

**Hotové, keď:** test briefu s jedným `proposed` a jedným `accepted` rozhodnutím ukáže značku len
pri prvom; výsledok hľadania na `deprecated` záznam ju má tiež.

**Veľkosť:** S.

---

### 1.2 Deduplikácia, ktorá nezakryje rozdiel

**Čo:** Zlúčenie výsledkov z viacerých zdrojov odstráni len preukázané kópie.

**Princíp:** Dve tvrdenia s rovnakým začiatkom nie sú to isté tvrdenie.

**Prečo:** `merge` v `src/model.rs:27` porovnáva prvých 90 znakov normalizovaného názvu a úryvku.
Dva rôzne záznamy s rovnakým úvodom sa zlejú a jeden zmizne.

**Ako:**
1. Dedup podľa textu použiť len na kópie lokálneho záznamu z adaptéra (`mirrored_id` to už rieši)
   a na úplnú zhodu celého normalizovaného textu, nie prefixu.
2. Podobné výsledky zoskupiť pod seba, nie vyhodiť: druhý sa ukáže odsadený s tým, v čom sa líši
   (zdroj, dátum).

**Hotové, keď:** test s dvoma zásahmi, ktoré sa zhodujú v prvých 90 znakoch a líšia ďalej, vráti
oba.

**Veľkosť:** S.

---

### 1.3 Doručovanie podľa verzie, nie podľa id (Peer)

**Čo:** Peer považuje záznam za „už povedaný“ len pre konkrétnu verziu. Zmena obsahu alebo nová
námietka je nová vec, ktorú treba povedať.

**Princíp:** Povedať každú vec raz. Zmenený záznam je iná vec ako pôvodný.

**Prečo:** `toldOf` v `plugins/knowledge/Knowledge.ts:238` si pamätá povedané záznamy ako množinu
id (`session.told("knowledge")`). Keď záznam niekto nahradí alebo spochybní, agent sa to nedozvie.
Po kompaktácii (`sessionMemory.ts:183`) sa potrebné pravidlá musia zopakovať, ale história
toho, čo agent počul, musí ostať pre posudzovanie nezávislosti nálezov.

**Ako:**
1. Kľúč v `told` je `id@revision` (`publication.revision` z `entries.get`) a pre námietku
   `id#challenge@finders`.
2. Po kompaktácii zopakovať iba záznamy, ktoré riadia súbory, na ktorých session práve pracuje
   (`told.touching`), nie celý index.
3. `heard` (nálezy) sa nikdy nemaže. To je iný zoznam a slúži Hubu na počítanie nezávislosti.

**Hotové, keď:** test: záznam povedaný, potom nahradený v knihe → pri ďalšom kroku sa povie
znova s poznámkou „zmenené od …“; po kompaktácii sa povedia len riadiace záznamy pre
`touching`; `heard` ostane.

**Veľkosť:** M.

---

### 1.4 Jeden rozpočet tokenov na session

**Čo:** Pri štarte úlohy jeden spoločný rozpočet pre znalosti z kontextu, Peeru aj adaptérov.
Bežný krok bez zmeny nenesie žiadny znalostný obsah. Nová relevantná zmena nesie krátky dodatok.

**Princíp:** Rozpočet sa nenásobí. Samostatný limit pre kontext, Peer a každý adaptér sa
sčíta do niečoho, čo nikto nechcel. Veľkosť odpovede a cena jej získania sú dve rôzne veci.

**Prečo:** `ctx_brief` má parameter `budget`, Peer má rozpočet indexu, adaptéry svoje sekcie.
`ctx_search` predvolene oslovuje všetky vyhľadávacie adaptéry, aj keď lokálny výsledok stačí.

**Ako:**
1. Východiskové limity na meranie (nie namerané optimum): štart úlohy 1 000–1 500 tokenov
   spolu; bežný krok 0; nová zmena alebo námietka 200–400; vyhľadávanie stručné výsledky s
   odkazmi, detail až na vyžiadanie.
2. Peer odovzdáva kontextu rozpočet už znížený o to, čo sám pridá (index, overlaps).
3. `ctx_search`: adaptéry volať až vtedy, keď lokálny výsledok má menej ako N zásahov alebo
   agent o ne požiada. Predvolené správanie zmeniť až po meraní (1.5).

**Hotové, keď:** v evaluačnej sade (1.5) je štart session pod spoločným limitom vo všetkých
scenároch a bežný krok bez zmeny nepridáva nič.

**Veľkosť:** M.

---

### 1.5 Evaluačná sada

**Čo:** 20–30 reálnych situácií a meranie: správny nález, nesprávne potlačené výsledky, tokeny,
latencia.

**Princíp:** Mechanizmus, ktorý sa nedá zmerať, sa nedá ani obhájiť ani vylepšiť.

**Prečo:** Nič z etáp 2 a 3 sa nedá posúdiť bez toho. Prahy v tomto dokumente sú odhady.

**Ako:**
1. Scenáre: staré rozhodnutie, protirečenie, podobné názvy, iný worktree, zmena po doručení,
   kompaktácia, nedostupný Hub, dotaz vlastnými slovami, dotaz s presnou frázou, `proposed`
   verzus `accepted`.
2. Repozitár s fixtúrami v `tests/`, skript, ktorý spustí brief/search/why a porovná s očakávaním.
3. Výstup: tabuľka za scenár (nájdené/minuté/nesprávne potlačené, tokeny, ms). Beží v CI.

**Hotové, keď:** sada beží, má základnú čiaru z dnešného stavu a každá zmena v etapách 2 a 3
ukáže rozdiel.

**Veľkosť:** M.

---

## Etapa 2 — údržba znalostí

### 2.1 Odkiaľ záznam pochádza — evidencia (5, 11)

**Čo:** Záznam nesie zdroje: PR, issue, úloha v Peeri, miesto v kóde s repozitárom a revíziou,
plus krátky overený citát.

**Princíp:** Tvrdenie odkazuje na zdroj, ktorý sa dá otvoriť. Odkaz na súbor bez revízie sa
časom posunie. Citát musí byť doslovný a overený, nie len správne naformátovaný.

**Prečo:** Reviewer v PR vidí len text. Nevie, či záznam stojí na troch nezávislých nálezoch
alebo na jednej vete v chate. Kandidát v Hube má až 20 zdrojov a pri `Keep` sa stratia.

**Ako:**
1. Začať bez novej schémy: `drafts.stage` už prenáša `extra.external_refs` a
   `extra.evidence_hash` (`src/api_write.rs:790`). Dohodnúť tvar odkazov: `pr:123`,
   `issue:45`, `task:krk-335`, `code:<repo>@<rev>:src/x.rs#L10-20`, `url:…`.
2. Peer pri `Keep` (`plugins/knowledge/client/kontext.ts`) zostaví odkazy z `candidate.sources`:
   úlohy, `origin`, súbory z textu nálezu, a počet nezávislých nálezcov. E-maily a id sessions
   nie, do Gitu nepatria.
3. Citát (`quote`, do 200 znakov) kontext pri `drafts.stage` overí: pri `code:` odkaze
   `git show <rev>:<path>` a hľadanie citátu; pri inom odkaze ostáva neoverený a tak sa aj označí.
4. Do briefu ide len „má N zdrojov“, detail v `ctx_read` na úrovni L1.
5. Až keď sa tvar ustáli, povýšiť `extra.external_refs` na pole `evidence:` v schéme.

**Hotové, keď:** uložený kandidát s tromi zdrojmi má tri `task:` odkazy a žiadny e-mail;
citát, ktorý vo zdroji nie je, stage odmietne alebo označí ako neoverený.

**Veľkosť:** M.

---

### 2.2 Termín kontroly záznamu (2)

**Čo:** Záznam môže mať `review_by: 2027-01-31`. Po termíne ho brief, hľadanie a `status`
označia „na kontrolu“.

**Princíp:** Je to termín kontroly, nie koniec platnosti. Záznam sa neschová; niekto ho má
pozrieť. V pochybnostiach je záznam bez termínu.

**Prečo:** „Počas migrácie nepíš do tabuľky X“ alebo incident s dočasným obchádzaním ostanú
v `.ai/` navždy ako aktívne.

**Ako:**
1. Voliteľné pole `review_by` v hlavičke (`src/store.rs`), validácia: platný dátum, nie pred
   `date`.
2. Značka v briefe, v `ctx_search` a v `kontext status`; zapojiť do `src/freshness.rs`, aby
   mali obe značky (churn a termín) jedno miesto.
3. Voliteľná predvolená lehota podľa druhu v `.ai/kontext.toml` (`incident = "90d"`),
   predvolene vypnuté.

**Hotové, keď:** záznam s termínom v minulosti má značku na troch miestach; bez poľa sa nič
nemení; zlý dátum je chyba validácie.

**Veľkosť:** S.

---

### 2.3 Námietky voči záznamom — jednoduchá prvá verzia (3, 9)

**Čo:** Agent alebo človek povie „tento záznam už neplatí a prečo“. Námietka má zdroj, verziu
napadnutého obsahu a stav. Vybavuje sa cez review: náhradou alebo vyradením.

**Princíp:** Hlas má váhu, len keď je nezávislý. Námietka nemení poradie ani nič neschová; je
to signál pre ľudí. Vyriešená je až vtedy, keď je zmena prijatá vo vetve na review, nie keď
niekto niečo pripravil v jednom worktree.

**Prečo:** Peer agentovi hovorí „ak záznam neplatí, povedz svojmu človeku“ a tam to končí.
`ctx_prepare_commit` vypisuje „confirm it still holds“ (`src/ops.rs:1593`) a agent nemá kam
odpovedať.

**Ako:**
1. **Kontext bez Peeru:** `ctx_challenge <id> "prečo"` zapíše do inboxu kandidáta druhu
   `challenge` s `against: <id>` a `against_revision` (z `publication.revision`). Platia
   pravidlá inboxu: patrí worktree, zdieľa sa len commitom. `ctx_prepare_commit` ponúkne
   vyradenie (`drafts.retire`, nové: stage starého súboru so `status: deprecated` a dôvodom)
   alebo náhradu (zatiaľ ručne, generovanie v 3.2).
2. **Peer Hub:** riadok `- [against <id>] prečo` pod „For the team“; Hub počíta nezávislých
   namietateľov rovnakým pravidlom ako pri kandidátoch. Stavy: `open`, `dismissed`,
   `resolving` (náhrada alebo vyradenie je staged), `resolved` (Peer overil na knihe projektu, že
   starý záznam je `superseded`/`deprecated` a `publication.state` je committed alebo
   reviewed_ref). Námietka nesie `entry_revision`; UI ukáže „záznam sa odvtedy zmenil“.
   Limit 100 sa týka len vybavených námietok; otvorené sa nikdy nemažú. Celé je v
   `hotovo-ai-hub/openspec/changes/add-knowledge-challenges/`.
3. Agent, ktorému Peer námietku ukázal, je „počul“ (1.3): jeho riadok sa pridá ako zdroj, ale
   nezávislý hlas to nie je.

**Hotové, keď:** podľa tasks v openspec zmene; v kontexte test: námietka v inboxe → brief ju
ukáže → `prepare_commit` ponúkne retire → po stage má starý súbor `deprecated`.

**Veľkosť:** M (kontext) + L (Hub a Peer).

---

### 2.4 Čerstvosť fronty kandidátov (13)

**Čo:** Fronta „Knowledge to keep“ radí podľa nálezcov za posledných 30 dní, kandidát bez
nového zdroja 60 dní ide do zbaleného zoznamu.

**Princíp:** Čerstvosť pomáha triáži. Nerozhoduje o pravdivosti a nič neskrýva: zbalený
kandidát sa nájdený znova vráti.

**Ako:** iba triedenie a zobrazenie v `api.rs::list` a vo web UI; `sources[].at` už existuje.

**Hotové, keď:** test poradia s fixným časom ukáže čerstvého kandidáta pred starým s viac
nálezcami; zbalený kandidát sa po novom náleze vráti.

**Veľkosť:** S.

---

### 2.5 Čo sa zmenilo od posledného briefu (6, upravené)

**Čo:** Brief ukáže záznamy pridané alebo nahradené od poslednej návštevy tohto worktree.

**Princíp:** Doručovať zmeny od posledného doručenia, nie kalendárne okno. Kalendárne okno
opakuje ten istý obsah každý deň.

**Prečo:** Agent vidí rozhodnutia „najrelevantnejšie najskôr“, nie čo je nové. Peer vie
„od posledného povedania“ (1.3), kontext samotný je bez stavu.

**Ako:**
1. Pod `.git/kontext/` si worktree uloží HEAD úložiska znalostí pri poslednom briefe. Je to
   cache: keď chýba, použije sa okno 14 dní.
2. Sekcia `## Changed since <rev>` s najviac 5 riadkami, pri nahradených „nahrádza X“. Má
   vlastný malý rozpočet a pri plnom rozpočte vypadne prvá.
3. V Peeri sa táto sekcia nepoužije; Peer doručuje podľa 1.3.

**Hotové, keď:** druhý brief po novom rozhodnutí ho ukáže, tretí bez zmien už nie; po zmazaní
cache sa ukáže okno 14 dní.

**Veľkosť:** S.

---

## Etapa 3 — voliteľná asistencia modelom

Bez modelu musí všetko fungovať. Model sa volá na vyžiadanie, raz, a výsledok sa viaže na
obsah a verzie porovnávaných záznamov, aby sa nevolal znova v `capture`, `formalize`, `stage`
a pri otvorení karty.

### 3.1 Vzťah nového záznamu k existujúcim (1, 10)

**Čo:** Pri novom zázname alebo kandidátovi kontext povie: „súvisí s X (možný konflikt /
rozširuje / rovnaké)“, s odkazom na zdroje. Rozhodne človek.

**Princíp:** Dva kroky. Deterministický výber podobných záznamov (text + prekryv `paths`) vždy;
model na vyžiadanie s jednou otázkou, v ktorej je akcia aj cieľ spolu. Spojenie zabráni
odpovedi „nahraď“ ukazujúcej na zlý záznam, ale nezaručí správny výber. Preto výstup znie
„možný konflikt, skontroluj“, nikdy „nahraď“.

**Prečo:** Dnes sa porovnávajú len názvy (`title_similarity >= 0.6`, `src/ops.rs:464`). Peer
volá `entries.search` až pri `Keep`, človek pri rozhodovaní nevidí, že podobný záznam existuje.

**Ako:**
1. Operácia `entries.related {text, paths}`: BM25 + prekryv `paths`, najviac 5, s dôvodom.
2. Voliteľne `llm: true`: jedna otázka s možnosťami `new`, `same:<id>`, `extends:<id>`,
   `conflicts:<id>`, `uncertain`; viac vzťahov naraz je dovolené. Výsledok cache podľa hashu
   textu a revízií porovnaných záznamov.
3. Výstup v `ctx_capture`, `drafts.*` a na karte kandidáta v Peeri ako štítok s odkazom.
   `conflicts` ponúkne (nepredvyplní) `supersedes`; `extends` ho neponúkne, rozšírenie nie je
   náhrada; `same` ponúkne „Dismiss: už máme X“ a dôvod ide do `guidance.propose`.

**Hotové, keď:** v evaluačnej sade scenár „protirečenie“ dá `conflicts` so správnym id a
scenár „podobné názvy, iný obsah“ dá `uncertain` alebo `new`; bez modelu vráti kandidátov s
dôvodom.

**Veľkosť:** M.

---

### 3.2 Návrh náhrady podľa námietok — `drafts.revise` (4)

**Čo:** Z napadnutého záznamu a námietok pripraví návrh nového záznamu so
`supersedes: [starý]`. Človek upraví, Peer stage-ne.

**Princíp:** Oprava je nový záznam, nie prepis starého. Skontroluje sa, že záznam je stále v
revízii, ktorú námietka napadla.

**Ako:** podľa design.md D7 v openspec zmene; vstup `entry_id`, `entry_revision`,
`objections`, `llm`; bez modelu `model_unavailable`; nikdy nevráti staré telo ako nové.

**Veľkosť:** S. Až po 2.3.

---

### 3.3 Frázový bonus vo vyhľadávaní (7, opravené)

**Čo:** `Should` klauzula s frázou celého dotazu, aby presná zhoda vyhrala.

**Čo odpadlo:** náhradný OR dotaz. Tantivy 0.26 `QueryParser` má OR predvolene a kód
`set_conjunction_by_default` nevolá, takže navrhované „uvoľnenie“ by nič nezmenilo.

**Ako:** zaviesť až po meraní v 1.5 na scenároch „vlastné slová“ a „presná fráza“.

**Veľkosť:** S.

---

### 3.4 Odložené, kým sa neukáže potreba

- **Prijatie dokumentu ako kandidátov (8).** Rozširuje vstup skôr, než je overená kvalita
  výberu a údržby. Spustiť, keď budú 2.1 a 3.1 hotové a tím to bude chcieť.
- **Sémantické zlučovanie kandidátov (12).** Najprv v Hube zmerať, koľko skutočných duplicít
  uniká dnešnému porovnaniu po písmenách. Prah bez dát nezavádzať.
- **Projekt → firma (14).** Vyžaduje rozsah platnosti, prístupy a očistenie od projektových
  detailov. Až pri reálnej potrebe.

---

## Čomu sa vyhnúť

- Prepisovať na mieste. Vždy `supersedes` alebo `deprecated` s dôvodom.
- Hlas, ktorý platí len pre hlasujúceho. Tímu nepovie nič.
- Mlčky vyraďovať. Označiť a ukázať.
- Ukladať všetko automaticky. Kontrola človekom je zámer.
- Vektorová databáza alebo model v jadre. Ostávajú adaptérmi.
- Model volať opakovane na to isté. Raz, viazané na obsah a verzie.
- Samostatné rozpočty na každej vrstve. Jeden spoločný.

## Čo sa zmenilo po druhom posudku

- **Bod 7 mal technický omyl**: OR fallback predpokladal povinnú zhodu všetkých slov, ktorú
  parser nemá. Odstránené.
- **Námietka sa uzatvárala priskoro** (po stage). Pridané stavy `resolving` a overené
  `resolved`, verzia napadnutého obsahu, vyradenie bez náhrady. Limit sa netýka otvorených.
- **`contradicts`/`extends` nepredvyplňujú náhradu.** Výstup je „možný konflikt“, pridané
  `uncertain` a viac vzťahov.
- **„Nové za 14 dní“** nahradené „od posledného briefu“ s cache; v Peeri doručovanie podľa
  verzie.
- **Pridané**: autorita v briefe, bezpečná deduplikácia, doručovanie podľa verzie, jeden
  rozpočet, evaluačná sada.
- **Odložené**: ingest, sémantické zlučovanie, projekt → firma.
- **Evidencia** posunutá dopredu a začína cez existujúce `extra.external_refs`, odkazy na kód
  nesú repozitár a revíziu, citát sa overuje.
