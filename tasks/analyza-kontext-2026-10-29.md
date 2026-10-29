# Hĺbková analýza kontext a návrh ďalších krokov (29. 10. 2026)

Stav repozitára pri písaní: `main` = v0.9.0 (0d3e277), vydané a pushnuté dnes počas analýzy. Čítači pracovali nad v0.8.0 a nad vtedy nevydanou vetvou 0.9.0, overovatelia nad oboma; kód briefu, vyhľadávania, indexu, úložiska a MCP servera sa medzi 0.8.0 a 0.9.0 nezmenil, takže zistenia platia pre dnešný `main`. Dôkazy ku každému zisteniu (reprodukčné príkazy, výstupy, riadky kódu) sú v prílohe [analyza-kontext-2026-10-29-zistenia.md](analyza-kontext-2026-10-29-zistenia.md); čísla `F…` odkazujú tam.

## 1. Ako analýza vznikla

Dva workflow na Opus 5.5 subagentoch, každý krok s overením kódom, nie názorom.

| fáza | agenti | čo robili |
| --- | --- | --- |
| čítanie | 9 | jeden čítač na subsystém (úložisko, brief/search, údržba, API, MCP/adaptéry, init/vlákna/CLI, testy/eval/CI, dokumentácia, tvoje vlastné vlákna cez `ctx_threads`); každé zistenie s `súbor:riadok` |
| rešerš | 3 | pamäť agentov 2026, MCP a platformy (Claude Code, Codex, Cursor, OpenCode), retrieval a ADR nástroje; 91 datovaných zdrojov, 59 techník |
| overenie | 24 | na každú skupinu dve nezávislé šošovky pre high/medium (A: reprodukcia v scratch klone s debug binárkou alebo trasovanie kódu; B: či to už niečo rieši, či docs naozaj sľubujú to, čo zistenie tvrdí, či závažnosť sedí) a jedna rýchla pre low |
| návrh | 4 + 3 + 1 + 1 | štyria návrhári z rôznych uhlov, traja porotcovia, syntéza, kritik úplnosti |

Výsledok čítania: 199 zistení, po zlúčení duplicít 185. Výsledok overenia: 185 potvrdených aspoň čiastočne, 0 vyvrátených. Šošovka B pri 61 zisteniach znížila závažnosť ako nadhodnotenú; konečná závažnosť je vždy tá miernejšia z A a B.

| konečná závažnosť | počet |
| --- | --- |
| high | 4 |
| medium | 41 |
| low | 140 |

Východiskový stav: `cargo test` zelený (128 unit, 26 integračných), clippy a fmt čisté, eval 33/33 scenárov a 69/69 očakávaní. Všetky štyri high zistenia boli reprodukované príkazom, nie len prečítané. To, že nič nebolo vyvrátené, ber aj ako upozornenie: skeptici súhlasili vo všetkom, takže závažnosť je dôveryhodnejšia než samotný počet.

Mimo záberu: reálny jj repozitár, Windows, úložisko s 250 záznamami (meralo sa na tomto repe a syntetických 12 konvenciách), skutočné volania Peeru (čítali sa len rozhodnutia v Hube), živý model pri `distill` a `related`. OpenViking MCP počas analýzy vypršal, pamäť som čítal z disku.

## 2. Záver v skratke

Tvar kontextu je správny a výskum z roku 2026 ho potvrdzuje: lexikálne vyhľadávanie riadené agentom poráža vektory (arXiv 2605.15184), kontextové súbory bez výberu zhoršujú výsledky agentov o viac ako 20 % (ICLR 2026, „Evaluating AGENTS.md“), Copilot Memory prešiel na citácie overované na aktuálnej vetve, Cursor zrušil UI pamätí v prospech súborových pravidiel. Nikto z konkurencie nemá review pred zdieľaním a supersede namiesto prepisu; to je tvoja výhoda a treba ju brániť výsledkami, nie princípom.

Štyri sľuby však dnes neplatia:

1. **Bezpečnostný sľub** „klonovaný repozitár nespustí príkazy bez `kontext trust`“ (08-security.md:16). Pole `verify:` z ľubovoľného záznamu, aj nezacommitovaného, beží cez `sh -c` s celým prostredím, bez trust pinu, `ctx_verify` má `destructiveHint:false` a Codex má od `kontext connect codex` `default_tools_approval_mode = "approve"`. Reprodukované: záznam s `verify: echo PWNED-$(whoami) > pwned.txt` prešiel (F7).
2. **Sľub briefu** „rozhodnutia pre cesty, ktorých sa agent dotkne“. Zameraný brief na `src/ops.rs` ukáže pri rozpočte 1 400 tokenov 9 z 10 riadiacich rozhodnutí, pri 600 dve z desiatich, a pritom nechá nerelevantnú konvenciu o docs a learning o `src/repo.rs`. Riadok „… 17 more“ nepovie, že medzi vypadnutými je riadiace. Dvanásť riadiacich konvencií zmizlo celé bez stopy (F15, F90).
3. **Sľub overenia** „len prečítanie človekom alebo prejdený verify je overenie“. Brief píše `verified agent at …` a `verified bulk` (F35), holé `checked:` bez riadku v ledgeri sa hlási ako `verified read` (F8), a agent cez Bash zapisuje `read` (F9). Počítanie „N commitov od pohľadu“ ide podľa pozície v `git log`, nie podľa ancestry, takže zmena vyvinutá pred potvrdením a zlúčená po ňom (bežný merge commit na GitHube) sa nikdy nezaráta (F10).
4. **Správna odpoveď** z dlhobežiaceho MCP servera: keď index obnoví iný proces (`kontext status`, CLI capture, druhá agentová session v tom istom worktree), server servíruje zastaraný index donekonečna bez varovania (F16).

Ostatné zistenia sú menšie, ale je ich veľa a skladajú sa: supersede zapísané len na starom súbore nechá dve aktívne protirečiace rozhodnutia (F19), krátke prefixy z briefu sa ukladajú a zostarnú (F21), slovenská história commitov dá nula „decision-shaped“ commitov (F45), eval set od 0.3.0 nemeria nič z 0.4 až 0.9 (F107), MSRV 1.88 nejde zbuildovať s `--locked` (F22).

## 3. Čo platí dnes: overené zistenia podľa tém

### 3.1 Bezpečnosť a dôvera

- **F7 (high)** `verify:` bez trust gate, `ctx_verify` bez varovných anotácií, Codex auto-approve, `dir` smeruje na ľubovoľný repozitár; shipnutý skill kontext-audit agentom hovorí `kontext verify <id>` spustiť. Šesť čítačov to hlásilo nezávisle.
- **F12** Trust pin adaptérov je 48-bitový FNV-1a; overovateľ našiel kolíziu dvoch TOML tabuliek za 1,1 s. Dokumentácia sľubuje „každá zmena vyžaduje nový trust“.
- **F18** `[vars]` a `[project] name` zo zdieľaného `.ai/kontext.toml` prejdú do šablón dôveryhodných osobných adaptérov bez trustu; realisticky `project.name` presmeruje OpenViking peer namespace (preset používa `peer = "{{project.name}}"`).
- **F52** `ctx_threads thread=<cesta>` prečíta ľubovoľný súbor na stroji (reprodukované s SSH kľúčom), `thread="-"` blokuje volanie na stdin servera.
- **F53, F54, F55** Redakcia maskuje len `BEGIN` riadok privátneho kľúča, telo ide modelu; telefóny bez `+` a IPv6 prechádzajú; chybný `secrets.redact` vzor sa ticho zahodí.
- **F14** Nástroje re-exponovaných adaptérov strácajú anotácie a spadajú pod Codex approve.
- **F65** `verify` zapíše HEAD aj pri špinavom strome a vráti `output_tail` bez redakcie (Peer ho ukladá do Hubu).
- **F40** Codex a Claude Code dokumentácia nesprávne hovorí, ktoré nástroje sú bezpečné na auto-approve (`ctx_init` s bootstrapom zapisuje, `ctx_verify` spúšťa príkazy).
- low, ale lacné: F99 trust nepokrýva skripty, ktoré adaptér spúšťa z repa; F76 chybové hlášky adaptérov nesú celú URL aj s kľúčom v query; F98 repozitár prepíše používateľov súkromný `secrets.redact`; F176 `ctx_read file:` servíruje `.git/`; F187 šablóny dostanú raw remote URL aj s credentials.

### 3.2 Brief a vyhľadávanie (čo agent dostane)

- **F15 (high)** riadiace záznamy vypadnú skôr než nerelevantné; tie-break podľa dátumu vyhadzuje najstaršie obmedzenia ako prvé, hoci `READ_BEFORE_CHANGING` tvrdí opak.
- **F90** sekcie konvencií, learningov a incidentov sa režú na 8/6/3 bez riadku „N more“ a celá sekcia zmizne, keď sa nezmestí.
- **F16 (high)** zastaraný index v MCP serveri po obnove iným procesom (`ReloadPolicy::Manual` + skorý návrat pri aktuálnom manifeste).
- **F83** focus na neexistujúcu cestu, glob alebo nový súbor sa rozpadne na slová; agent, ktorý vytvára nový súbor v riadenom adresári, dostane brief zoradený podľa zhody slov.
- **F97** `path_overlaps` minie adresárový focus pri vzoroch so skorým wildcardom (`**/migrations/*.sql` vs focus `src/db`) a vedúcim lomítkom.
- **F96** `[slug]` v Next.js cestách sa berie ako trieda znakov; záznam nikdy nematchuje vlastný súbor.
- **F93** `paths:` zalomené na viac riadkov (prettier) dajú `["[src/a.rs"]`; záznam neriadi nič.
- **F86** `ctx_read` na úrovni 0 a 1 neukáže `superseded_by` ani značky (local, challenged, review due).
- **F84** `search adapters` vráti 9 doc chunkov a 1 modulový doc; rozhodnutia sú 13. a 15.; superseded rozhodnutia bez odkazu na náhradu.
- **F85** bez stemmingu a bez folding diakritiky: `namietka` 0 hitov, `námietka` 6; `adapter` minie „Adapters are configuration“.
- **F82** riadok briefu ukazuje `summary`, nie `rule_text`; pravidlo sa agent dozvie až pri `prepare-commit`.
- low: F170 phrase bonus nikdy nematchuje pri identifikátoroch v dotaze; F101 dedup zahodí aktívnu náhradu a nechá superseded dvojča; F102 hity adaptéra zoradené podľa abecedy bucketov (`result.*[*]` v OpenViking presete), nie podľa skóre; F174 päta briefu mimo rozpočtu; F180 brief nemá strojovú formu.

### 3.3 Životný cyklus znalostí (ledger, freshness, drift, supersede)

- **F10 (high)** `since` a freshness podľa pozície v logu alebo času, nie ancestry; reprodukované s `--no-ff` merge.
- **F19** stav „superseded“ sa odvodzuje len zo starého súboru; `apply_supersedes` pri nerozlíšiteľnom id iba vypíše na stderr a `kontext check` je zelený.
- **F20** adr-tools štýl (`## Status` sekcia) sa číta bez stavu: superseded ADR sú aktívne a supersede na ne nič neurobí; `init` pritom takéto adresáre adoptuje.
- **F21** brief tlačí najkratší jedinečný prefix, agenti ho kopírujú do `supersedes` a `[[odkazov]]`; po pridaní záznamu s dlhším id prestane rozlišovať, a pravidlo `starts_with("{s}-")` matchuje aj `-2` prípony.
- **F8, F9, F35** semantika „verified“ (vyššie).
- **F58 až F63** signály nesúhlasia: prejdený `verify` nie je pohľad pre drift ani freshness (F60), bulk stamp alebo `--repair` umlčí drift a freshness (F59), najnovší riadok vyhrá bez ohľadu na silu a zlyhaný verify vynuluje `since` (F58), drift ignoruje poradie v rozsahu (F63), drift bez základu ticho hlási „No knowledge drift“ a `--fail` vráti 0 (F62). Šošovka B ich znížila na low, lebo sú to rozhodnuté zjednodušenia; opravy potrebujú superseding rozhodnutia k 2026-10-20.
- **F61** potvrdenie toho istého záznamu na dvoch vetvách dá konflikt v súbore záznamu (`checked:`), nie v ledgeri.
- **F64** verify runner visí, keď príkaz nechá bežať proces so stdout (`sleep 12 &` prešiel po 12 s pri timeoute 2 s).
- **F66** integrita ledgera sa nekontroluje; budúci dátum vyhrá `latest()` a `--repair` potom zapíše `checked`, ktoré validácia odmietne.
- low: F67 `code:` evidencia prijme pohyblivú revíziu a kontroluje sa len raz; F68 secret scan po prvom matchi na riadku; F153 union merge ledgera pod jj neplatí a konfliktné riadky sa ticho preskočia.

### 3.4 API 0.9.0 a dve zapisovacie cesty

- **F30** `drafts.extend` s modelom a `drafts.stage amends` prepisujú telo na mieste; rozhodnutie 2026-10-27 hovorí „append or supersede, never rewrite“ a 2026-10-29 ho nesupersedne ani nezúži. Bez modelu je cesta `## Added <dátum>` v poriadku; model fold je presne tá konsolidácia, ktorú 2026-10-27 odmietlo.
- **F31** amendovaný záznam si nechá `verified: true` z čítania pred zmenou textu.
- **F32** `extends` sa cez `ctx_capture` → `ctx_prepare_commit` nedá zapísať a `drafts.capture` ho ticho zahodí.
- **F28, F29** revízny guard prijme `HEAD`; `related` dáva 48-bit hash, `retire` a `revise` chcú blob a vracajú iný kód chyby.
- **F27** `ops::promote` (CLI, `ctx_prepare_commit`) je druhá cesta vedľa `drafts.stage`: nechá v tímovom súbore `api_operation`, stage-ne používateľove necommitnuté úpravy supersedovaného súboru, neznáme `supersedes` len vypíše na stderr. Rozhodnutie 2026-10-15 hovorí o jednej kanonickej ceste.
- **F13** `ctx_inbox drop` a `prepare_commit drop` mažú cudzie worktree kandidátov bez ownership, zámku a `.dismissed` záznamu (reprodukované cez suffix id).
- **F1** resume prerušeného `drafts.stage` parsuje plán ako `learning`, takže fields-style ADR sa nikdy neobnoví (`operation_conflict`).
- low: F24 až F26 receipty po routingu, po neskoršom confirm a po cleanup; F122 amend splice do fields-style záznamu môže podsunúť polia; F186 amend zahodí H1 pri preambule; F2 `CommittedEntry` nenesie `supersedes`, `date`, `verify`.

### 3.5 Init, vlákna, distill

- **F5** `kontext init skip mod:<modul>` nemá efekt; úloha ostáva prvá vo fronte (reprodukované).
- **F6** dokončené `dec:` úlohy sú len v `.git/kontext/init-done.json`; kolega po pulle ich dostane znova.
- **F42** id klastrov sú pozičné a výber modulu cez `HashMap::max_by_key` je nedeterministický: tri behy `init status` nad tým istým HEAD dali tri rôzne klastrovania.
- **F45** skórovanie je anglické: šesť rovnakých rozhodovacích commitov po slovensky dalo „0 decision-shaped“, po anglicky 6.
- **F46** bot commity (dependabot s release notes) skórujú 6,25, nad prahom 3,5.
- **F47** commity s `Decision:` trailermi sa ťažia znova; na tomto repe `init next` ponúka `dec:docs-2` s commitmi, ktorých rozhodnutia už existujú.
- **F48** opakovaný `init` prepíše ručne upravené `paths`, `tags` a `summary` modulových docov (docs sľubujú opak).
- **F43** bootstrap cez `ctx_init` alebo API `setup` natrvalo zaznamená „hooks declined“.
- low: F44 fakty v overview sú z 28. 9. (31 .rs súborov, dnes 57) a nič ich neoznačí; F56 distill nemá pamäť spracovaných vlákien ani zahodených kandidátov; F135 bash výstupy z Claude Code idú modelu ako text používateľa; F147 prompt nesie host/org repozitára.

### 3.6 MCP server a adaptéry

- **F69** `ctx_brief`, `ctx_search`, `ctx_why`, `ctx_read` majú `openWorldHint:false`, hoci volajú adaptéry cez HTTP/MCP; docs to opakujú.
- **F73** outbox po 8 neúspešných flushoch eventy zahodí bez dead-letter; jeden výpadok adaptéra míňa pokusy všetkým; eventy pre adaptér preskočený cez `when` sa počítajú ako doručené (reprodukované).
- **F71, F72** `listChanged:false` hoci adaptérové nástroje pribúdajú; žiadne `notifications/cancelled`, verify proces prežije požiadavku aj server.
- **F80** HTTP MCP klient po 404 (expirovaná session) ostane rozbitý do reštartu procesu.
- low, ale podstatné pre 2026: F154 textové nástroje bez `structuredContent`, `resource_link`, completions; F155 `resources/list` orezané na 300 bez cursora; F159 neznámy tool ako `isError` namiesto `-32602`; F158 nejednotné názvy parametrov (`budget_tokens` vs `budget_chars`, `why` vs `because`, `kind` vs `kinds`); F78 `ctx_api` má 24 operácií za jedným netypovaným `input`; F166 `connect skills` prepíše tímom upravené skilly a zapisuje len do `.claude/skills`.

### 3.7 Testy, eval, CI, inštalácia

- **F22** MSRV 1.88 nejde: `ordered-float 5.5.0` v lockfile chce 1.90; žiadny MSRV job.
- **F91, F110, F107, F109** eval set nemeria, čo sa pokazilo: F15, F16 a F90 prejdú celou sadou; scenáre sú od 0.3.0, nič pre drift, ledger, verify, flows ani 0.9.0; žiadne skloňovanie, diakritika, väčší korpus, ranking metriky.
- **F112, F113** nula testov pre `mcp_client.rs`, `events.rs`, `inbox.rs`, `init/history.rs`, `tools.rs`, `mcp.rs`; trust gate (prvý bod SECURITY.md) nemá test, hoci dnes funguje.
- **F105** CI testuje len Linux; Windows iba `cargo check`; release beží bez testov.
- **F103, F104** `install.sh` neoveruje checksum, pri výpadku siete ticho builduje `main` HEAD; glibc floor ≥ 2.34 a pri zlyhaní `--version` skript vypíše „installed“ a skončí 0.
- **F111** testy dedia globálny git config; `end_to_end` môže nainštalovať hooky do globálneho `core.hooksPath`.
- low: F106 eval ignoruje exit status; F115 akcie bez SHA pinu, `contents: write` na build jobe; F195 projekt nespúšťa `kontext ci` nad vlastným `.ai/`; F198 floating toolchain bez `--locked`.

### 3.8 Dokumentácia a pozicionovanie

- **F38** porovnávacia tabuľka v README a na landing page vynecháva `.claude/rules` (path-scoped) a to, že Claude Code číta AGENTS.md natívne; introduction to má, README nie.
- **F39** docs hovoria o 11 nástrojoch, existuje 16; `ctx_challenge`, `ctx_confirm`, `ctx_verify`, `ctx_drift` chýbajú v README.
- **F41** jediné push doručenie je nezameraný SessionStart hook; changelog 0.2.1 opisuje presne zlyhanie, ktoré z toho plynie.
- low: F133 roadmap, `tasks/plan.md` a `tasks/knowledge-ideas.md` bez stavu (roadmap stále sľubuje Linux ARM, ktoré release.yml dávno builduje); F129 FAQ („len autopilot volá LLM“, „žiadny iný stav“); F130 skill kontext-verify hovorí editovať `verify` riadok na mieste proti rozhodnutiu 2026-10-27; F131 audit skill posiela nezmyselný `{"v":1}` a nestránkuje.

## 4. Čo hovorí výskum 2026 a čo z toho plynie

Plné zhrnutia so zdrojmi sú v prílohe pracovných súborov; tu je to, čo mení rozhodnutia.

**Pamäť agentov.** Dve vrstvy: ručne písané súbory (AGENTS.md pod Linux Foundation, `.claude/rules` s `paths:`, Cursor `.mdc`, Codex vnorené AGENTS.md, Agent Skills s trojstupňovým odhalením) a modelom písaná pamäť (Claude auto memory, Codex memories, Copilot Memory). Vendori sami hovoria, že pamäť je pomôcka na vybavenie, nie pravidlá. Copilot Memory overuje citácie na aktuálnej vetve pred použitím a exspiruje po 90 dňoch. Najbližší architektonický konkurent je Letta Context Repositories (git-verzovaný Markdown, reflexia v worktree), bez review. Útoky MINJA a MemoryGraft ukazujú, že pamäť sa dá otráviť bežnou interakciou; obrana je review pri zápise a provenance, čo je presne inbox → PR.

**MCP a platformy.** Revízia 2026-07-28 je bezstavová: `server/discover` je povinný, `_meta` na každej požiadavke, `resultType`, `ttlMs`/`cacheScope`, `ping` a `logging/setLevel` zrušené, Roots/Sampling/Logging deprecated, Tasks a Apps ako rozšírenia, resource-not-found `-32602`. Claude Code v2 to na stdio už vyjednáva; kontext pozná verzie len po 2025-11-25 a pri neznámej padá na 2025-06-18. Claude Code odkladá MCP nástroje za tool search: načíta len inštrukcie (max 4 096 znakov) a názvy; Codex chce prvých 512 znakov inštrukcií sebestačných. Hooky sú dnes hlavný kanál pre kontext, ktorý musí byť vždy: Claude Code SessionStart (vrátane `compact`, `resume`, `clear`), SubagentStart, PreToolUse/PostToolUse na Edit|Write s `additionalContext` do 10 000 znakov; Codex hooky zapnuté, `.codex/hooks.json`; Cursor `.cursor/hooks.json` a číta aj Claude hooky. Skilly: Codex `.agents/skills`, Cursor a OpenCode aj `.claude/skills`; `name` musí sedieť s adresárom, popis do 1 024 znakov.

**Retrieval a ADR.** BM25 ako jadro obstojí; zisk je na strane dotazu a indexu: viac formulácií od agenta zlúčených cez RRF, `questions:`/`aliases:` pri capture (doc2query robený agentom a schválený v PR), Tantivy 0.27 so stemmerom na tieňovom poli, breadcrumb hlavičky v chunkoch. Hodnotenie prešlo na qrels s nDCG@k a recall@k; LLM-značené qrels (UMBRELA) radia systémy ako ľudia. Kompilovanie rozhodnutí do natívnych formátov (`.github/instructions/*.instructions.md` s `applyTo`, `.cursor/rules/*.mdc`, `.claude/rules`, BUGBOT.md) dostane reviewované rozhodnutia aj k review botom a agentom bez MCP; `.ai/` ostáva jediná pravda. Packmind je najbližší konkurent pre distribúciu štandardov; git-natívna odpoveď je verzovaný balík v `.ai/org/` bumpovaný botím PR. `llms.txt` má nízku hodnotu.

Čo neprebrať (štyri techniky označené ako nevyhovujúce princípom): vektory v jadre, LLM-extrahovaný temporálny graf, automaticky aplikovaná pamäť bez review, hustý index v binárke. Výskum ich pre kódovacie dotazy nepodporuje a princíp „žiadny model v jadre“ ostáva správny.

## 5. Návrh: ako to riešiť správne

Štyri nezávislé návrhy (správnosť najprv, skúsenosť agenta najprv, najnovšie techniky najprv, kapacita maintainera najprv) hodnotili traja porotcovia; dvaja vybrali kapacitný návrh, jeden ten zameraný na správnosť (body 24 : 23 : 20 : 19). Syntéza prevzala víťaza a naštepila naň najlepšie časti ostatných. Kritik úplnosti potom našiel vecné diery a protirečenia, ktoré som do návrhu nižšie zapracoval; kde som sa od syntézy odchýlil, hovorím prečo. Celá syntéza a kritika sú v pracovných súboroch session, v repe ostáva tento upravený plán.

Zásady plánu: najprv to, čo agentovi hovorí nepravdu alebo porušuje sľub, až potom platforma a výskum. Každá oprava má test alebo eval scenár, ktorý najprv zlyhá, a očakávania evalu píše iná session než tá, ktorá implementuje. Horúce súbory (`src/ops.rs`, `src/api_write.rs`, `src/store.rs`, `src/verification.rs`, `src/config.rs`) mení naraz len jedna session; ostatné moduly sa dajú paralelizovať, ako to robíš. Čísla verzií sú návrh, vlny sú podstata.

### Vlna 0 (dnes, bez kódu): povedať, čo neplatí

- Do changelogu 0.9.0 (en aj sk) pridať „Known issues“: `verify:` beží cez `sh` bez trust gate (nevolať `ctx_verify`, `drafts.verify` ani `kontext verify` v repozitároch, ktorým neveríš; Peer ich volá automaticky), `drafts.extend` s `llm` a `amends` s ľubovoľným telom prepisujú záznam na mieste proti 2026-10-27.
- Opraviť `src/connect.rs:27` (komentár Codex snippetu) a `agent-integrations/03-codex.md`, `02-claude-code.md`: `ctx_verify` spúšťa príkazy, `ctx_init` s bootstrapom zapisuje, `ctx_drift` a `ctx_threads` sú read-only.
- Rust 1.88 → 1.90 v `Cargo.toml`, README badge, inštalačnej stránke a CONTRIBUTING (alebo downgrade `ordered-float`), kým nepribudne MSRV job.
- Kvalifikovať sľuby v `08-security.md` (48-bitový pin; trust pokrýva text tabuľky, nie skripty; verify je mimo gate), `08-distill-threads.md` (čo redakcia naozaj maskuje), `06-init-pipeline.md` (re-render prepisuje paths/tags/summary) a tabuľku porovnania s Claude Code (F38).

### Vlna 1 — 0.9.1, bezpečnosť (samostatne, čo najskôr; tri PR)

1. **`verify:` je kód z repozitára.** Pin `sha256(cesta záznamu, NUL, príkaz)` v `trust.toml` pod `[repos.<slug>.verify]`; bez pinu `untrusted_verify` s textom príkazu, jeho hashom a riadkom `kontext trust --verify <id>`, nič nebeží; zmena príkazu pin zruší. `kontext trust --verify [<id>|--all]` len z interaktívneho terminálu, vypíše, čo pinuje. Runner: allowlist prostredia (PATH, HOME, USER, LANG, LC_*, TMPDIR, TERM + `[verify] pass_env` len z používateľskej vrstvy), `output_tail` cez `secrets::redact`, špinavý strom zaznamenaný v ledgeri, po skončení shellu čítanie stdout najviac 2 s a potom kill process group, MCP server zabije bežiace verify pri zatvorení stdin. `ctx_verify` dostane `destructiveHint:true`, `openWorldHint:true` a `dir` obmedzený na tento repozitár, jeho worktrees a submoduly. Skilly kontext-audit a kontext-verify pri `untrusted_verify` ukážu príkaz a zastavia sa; nikdy nespúšťajú `kontext trust`. Pinuješ jediný príkaz vo vlastnom `.ai/`.
   Čo pin **nerieši**, a rozhodnutie to musí povedať: pin viaže súhlas na text príkazu, nie na kód, ktorý `cargo test` alebo `npm test` spustí zo stromu; HOME ostáva čitateľný a sieť otvorená; v Codexe beží `ctx_verify` v MCP serveri mimo sandboxu agenta. Preto cez MCP navrhujem variant: keď je strom špinavý alebo príkaz nepinnutý, `ctx_verify` príkaz iba vráti, agent ho spustí vo vlastnom sandboxe a výsledok sa zapíše ako `agent` stamp, nie ako `verify`. Tým sa `verify` riadok v ledgeri stane tým, čím má byť: dôkaz, ktorý spustil človek alebo CI.
2. **Jeden SHA-256 pin pre všetko zdieľané, čo sa dostane do adaptéra.** `adapters_hash` = `sha256:<hex>` nad kanonickým TOML zdieľaných `adapters`, `vars` a `project.name`; starý 12-hex FNV pin sa odmietne s hláškou, nikdy sa ticho nepovýši (to by legalizovalo podvrhnutý pin). Zdieľané `[vars]` nikdy neprepíšu vars adaptérov z používateľskej alebo lokálnej vrstvy; `project.name` zo zdieľanej vrstvy musí byť slug. `{{repo.remote}}` bez userinfo. `kontext trust` len z TTY, s tým, že rozhodnutie priznáva: proti procesu, ktorý vie editovať `trust.toml`, je gate honour-based (Codex aj Cursor vedia alokovať PTY). Preto `connect` nainštaluje aj deny pravidlo hostiteľa na `kontext trust` spúšťaný agentom (Claude Code a Codex PreToolUse na Bash).
3. **Amend iba pridáva, model nikdy nepíše do záznamu.** `drafts.stage amends` prijme telo len vtedy, keď je staré telo jeho bajtovým prefixom a zvyšok sú sekcie `## Added <YYYY-MM-DD>`; inak `amend_not_additive` s odkazom na supersede. Po spojení sa výsledok znova parsuje: názov a všetky polia mimo tela musia byť zhodné (zatvára F122 aj F186). `drafts.extend` ďalej prijíma `llm`, ale nefolduje: pridá nález tak, ako bol napísaný, `as_written:true` s poznámkou; tvar odpovede sa nemení, Peerov keep flow prejde. Resume plánu berie kind z `kind_for_path`. `extends` prejde cez `ctx_capture` → inbox → promote. Jedna formulácia pravidla v `tools.rs:180`, v `concepts/02` a v skille kontext-verify: reviewované texty sa neprepisujú, konvencie, learningy a incidenty rastú datovanými dodatkami, jediné in-place úpravy sú metadátové riadky (`verify`, `checked`, `status` pri retire a supersede). Otvorená otázka 3 nižšie: či chceš, aby model smel formulovať aspoň pridanú sekciu.

Overenie: `verify_refuses_unpinned_command` (scratch klon s `verify: touch $TMP/pwned`, súbor nevznikne, po pine áno), `verify_scrubs_environment`, `verify_returns_when_background_child_holds_stdout`, `trust_gate_skips_untrusted_and_revokes_on_edit` (F113), `legacy_fnv_pin_is_refused`, `shared_vars_do_not_reach_user_adapter`, `stage_amend_refuses_rewritten_body`, `extend_with_llm_appends_as_written`. Pred tagom pustiť Peerove integračné suity proti kandidátovi (skript `scripts/release-check.sh`, mimo binárky) a do migration note napísať, že Peerov `drift.ts` musí mapovať `untrusted_verify` na „neoverené, ukáž príkaz“.

### Vlna 2 — 0.9.2, pravdivé odpovede (šesť modulovo disjunktných PR, dajú sa rozdeliť medzi sessions)

1. **`since` a freshness podľa ancestry** (`verification.rs`, `freshness.rs`): jedno čítanie `git log --format=%H %P --name-only HEAD` ohraničené `index.max_commits`, merge commity ostanú v grafe (bez `--no-merges` pri čítaní histórie), časové okno len bez kotvy; `since` = ne-merge commity na riadených cestách dosiahnuteľné z HEAD a nie z kotvy. Zlyhaný verify sa ukáže, ale nie je pohľad: nenahradí skoršie čítanie a nevynuluje počet. Agent/bulk stamp ostáva pohľadom pre freshness (rozhodnuté, zmena až po dátach). Príznaky `truncated` a `lower_bound` (shallow). Jedna mapa landingov na HEAD pre brief aj API namiesto dvoch `git log -p`. Výkonnostná brána: syntetická fixtúra 250 záznamov / 5 000 commitov, počet git volaní cez existujúci `GIT_TRACE2_EVENT` test a čas briefu do +20 %.
2. **Brief nikdy ticho nevyhodí riadiaci záznam** (`ops.rs`, `glob.rs`, `route.rs`): rezervovaná sekcia „Governs <focus> (N)“ pred všetkým ostatným, jeden kompaktný riadok na záznam (plné id, názov, značky, „· N commits since“), poradie podľa špecifickosti cesty (súbor > adresár > glob > `**`) a potom staršie prvé; **bez** anglických slov never/only/must (nemeraná, anglická heuristika, to isté, čo F45). Pretečenie na jednom vždy vytlačenom riadku „also governing: [a] [b]“; pri rozpočte pod veľkosť sekcie pravidlo dna: len id. Ostatné druhy riadok po riadku, každý orezaný druh končí „… N more (K govern your focus)“. Päta v rozpočte, superseded modulové docs vynechané. `currency_line` píše „stamped by agent“, „bulk stamp“, „verify FAILED“; „verified“ len kde `verification.verified` platí. `parse_focus`: položka s `/`, glob znakom alebo známou príponou je cesta aj keď súbor neexistuje; `path_overlaps` oboma smermi vrátane skorých wildcardov a vedúceho lomítka; `[slug]` literálne najprv, potom glob (ako git), nie podľa toho, či súbor existuje. Eval scenáre od inej session pri 200, 400, 600, 1 000 a 1 400 tokenoch (Peer posiela `max(200, 1500 − minuté)`, takže 200 až 1 000 je bežný prípad, nie okraj).
3. **Server vidí, čo zapísali iné procesy** (`index.rs`, `app.rs`, `repo.rs`, `mcp.rs`): `LocalIndex` si pamätá hash manifestu, `refresh()` reloaduje reader pri zmene ešte pred skorým návratom; nová schéma sa buduje v dočasnom adresári a premenuje (nikdy nemaže adresár, ktorý stavia iný proces); vetva sa číta na každú požiadavku; chyba parsovania configu sa cachuje podľa stampu a vráti ako varovanie v ďalšom výsledku. Nový `tests/mcp.rs` (stdio driver), ktorý 0.11 znova použije.
4. **Supersession je jedna pravda, referencie sú plné id** (`store.rs`): efektívny stav pri načítaní (záznam menovaný v `supersedes` aktívneho záznamu je superseded, `superseded_by` doplnené), `## Status` sekcia adr-tools sa číta aj zapisuje vo vlastnom štýle, fields záznam bez Status riadku ho dostane, `status`/`superseded_by` sa editujú ako jednotlivé riadky (nested YAML a komentáre prežijú), promote/capture/stage odmietnu nerozlíšiteľný cieľ, referencie sa pri zápise kanonizujú na plné id, pravidlo `starts_with("{s}-")` zmizne, duplicitné ADR číslo je chyba s menami súborov, id sa alokuje cez všetky druhy a staged check používa pravidlo `check_all`. `kontext check` hlási jednostranné supersede, `kontext repair --dry-run` ukáže úpravy.
5. **Nástroje agenta ostanú vo svojich hraniciach** (`tools.rs`, `threads.rs`, `ops.rs` read): `ctx_inbox drop` a `prepare_commit drop` cez `inbox.drop` API (ownership, zámky, `.dismissed`), id len presné alebo jedinečný prefix vlastných kandidátov; `ctx_threads` prijme len `claude:`/`codex:`/`superset:`/`last` a cesty pod známymi transcript rootmi, `-` cez MCP odmietne; `ctx_read file:` odmietne `.git/`, `.git-credentials`, `.ssh`. **Námietka ostáva „resolving“, kým v HEAD nepristane nový blob záznamu**; cleanup sa viaže na zmenu blobu, nie na existenciu súboru (dnes `resolve_challenges` námietku odstráni pri stage a `cleanup_committed` ju zmaže po ľubovoľnom commite, keď sa retirement resetne; to je vzor, ktorý tvoj druhý posudok vytkol Hubu).
6. **Redakcia dodrží sľuby a zlyháva zatvorene** (`secrets.rs`, `config.rs`): celý blok PEM, národné telefóny (oddeľovače, minimálny počet číslic), IPv6, každý match na riadku; nekompilovateľný vzor v `status` a `check`, distill/threads/autopilot odmietnu poslať modelu; súkromné `redact`/`allow` sa zjednocujú s repozitárovými, nie nahrádzajú; chybové hlášky adaptérov bez query stringu a userinfo a cez redakciu; bash-mode výstupy a compaction summary z Claude vlákien vypadnú.
7. **Kontext musí byť viditeľný už dnes** (`tools.rs`): v tejto session boli všetky `mcp__kontext__ctx_*` nástroje odložené za tool search a načítali sa len inštrukcie servera; `_meta["anthropic/alwaysLoad"]` na `ctx_brief`, prvých 512 znakov inštrukcií sebestačných (čo kontext drží, kedy hľadať nástroje, `ctx_brief` ako prvé), celok pod 4 096, `maxResultSizeChars` na `ctx_read` a `ctx_api`. Je to S a patrí sem, nie do 0.11.

Paralelne s vlnou 2 (jedna session, nedotýka sa `src/`): **brány**: macOS job, MSRV job čítajúci `rust-version`, `--locked` všade, build docs na PR bez deployu, test parity súborov `docs/en` ↔ `docs/sk`, `kontext ci` nad vlastným `.ai/` s `fetch-depth: 0`, release gated na testy a zhodu tagu s `Cargo.toml`, `contents: write` len publish jobu, akcie pinované SHA, `cargo deny`; spoločný test helper s `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`, dočasný HOME; eval zaznamenáva exit status; `tests/api_contract.rs` so snapshotom tvarov odpovedí každej operácie; jj colocated smoke test preskočený bez `jj` (jj je na stroji, nikdy sa nespustil, a nové dirty-tree guardy by mohli odmietať každé jj potvrdenie); do CONTRIBUTING pravidlo „prvý commit opravy je zlyhávajúci test a jeho výstup je v PR“.

### Vlna 3 — 0.10, význam na dôkazoch

1. **Graded eval set pred akoukoľvek zmenou rankingu**: `tests/eval/qrels.tsv` (TREC formát) nad fixtúrou shop a vlastným `.ai/`, rezy: presné identifikátory (tolerancia ±0,01), singulár/plurál, slovenčina s/bez diakritiky (označíš ty), parafráza, zameraný brief pri 200 až 1 400; nDCG@5, recall@10, MRR vedľa dnešnej tabuľky; baseline v repe, CI zlyhá pri poklese rezu; nové scenáre pre drift, confirm, ledger, verify, flows, extend/amends, `entries.changed`, kompaktáciu (backlog 1.5) a sumu rozpočtu za session (backlog 1.4, kritériom „štart session pod spoločným limitom“). Značkovanie: vývojársky skript mimo binárky, štítky od iného modelu, človek skontroluje, commitujú sa pred zmenou.
2. **Čo je „verified“ a čo je pohľad pre drift**: `checked:` iba vo front matter = kind `cache`, `verified:false` (pole sa ďalej zapisuje, Peer ho číta v `drift.ts`); `kontext confirm --migrate-legacy` napíše bulk riadky; API pridá `last_verified` a `since_verified` vedľa existujúcich polí; zlyhaný verify nastaví `failing:true`; `amended_since:true` po zmene tela po poslednom overujúcom pohľade. Kto zapisuje `read`: CLI len na TTY (inak `agent`, `--kind read` bez TTY s varovaním) a rozhodnutie priznáva, že TTY nie je dôkaz človeka; MCP nikdy `read`; `kontext api` ponechá `read` (aplikácia vlastní identitu), ale ledger nesie `by: application:<meno>`, aby sa to dalo rozlíšiť od terminálového čítania. Drift: prejdený verify je pohľad; pokrytie vyžaduje pohľad v alebo po poslednej riadenej zmene v rozsahu; bez základu drift povie prečo a `--fail` vráti nenulu. Capability reťazce `verification.last_verified`, `drift.ordered`.
3. **Hľadanie radí reviewované znalosti prvé a skladá jazyk** (`index.rs`, `ops.rs`): tieňové polia `title_f`/`body_f` s LowerCaser + AsciiFolding + anglický stemmer pri nižšom booste; **bez** upgradu na Tantivy 0.27 (0.26.2 má folding, používa ho už `api_read.rs:292`, aj stemmer; upgrade pridáva MSRV a schémové riziko bez potreby); `SCHEMA_VERSION` bump. Zdroje vážené na qrels (záznamy > docs chunky pri rovnakom skóre), superseded hity demotované s „→ superseded by [id]“, nikdy skryté; `entries.search` skratka len pri presnom id/ceste; frázový bonus z vyčistených slov bez expanzií; dedup nad celým normalizovaným textom, nikdy nenechá superseded dvojča nad aktívnym; adapter hity podľa mapovaného skóre; hity z vnorených stores dostanú značky. Default `[search] adapters` sa zmení len s nameraným časom a tokenmi cold startu v `tests/eval/README.md` (backlog 1.4).
4. **`ctx_read` a brief povedia celý stav**: L1 ukáže `superseded_by` s odkazom a značky, scheme-less argument uprednostní existujúcu cestu a povie, čo vyriešil, stránkovanie `offset`/`part`, orez podľa znakov; riadok briefu použije `rule_text`, keď summary pravidlo neobsahuje; `changed_since` briefu znova použije `entries.changed`.
5. **Jedna zapisovacia cesta**: `ops::promote` a `ctx_prepare_commit` promote cez `api_write::stage_candidate` (strip `api_operation`, guard na necommitnuté úpravy supersedovaných súborov, odmietnutie neznámeho `supersedes`), `capture --promote` rieši námietky ako `promote()`, `prepare_commit` validuje všetky id pred prvým vedľajším efektom a tlačí „… N more“ aj navrhnuté záznamy, nové inbox id obchádzajú `.promoted/` a `.dismissed/`, `retire_challenged` berie `write_guard` pred kontrolou, `amends` sa routuje podľa repozitára rodiča.
6. **API v1 povie, čo prijíma a čo sa smie meniť**: politika do `reference/05` (en, sk): v rámci verzie 1 môžu pribúdať operácie, voliteľné vstupy, polia, kódy chýb, enum hodnoty a capability reťazce a klienti neznáme ignorujú; odstránenie, premenovanie, zmena typu alebo významu bumpne verziu; zúženie vstupu je dovolené iba vtedy, keď staré správanie odporovalo zaznamenanému rozhodnutiu alebo bezpečnostnému sľubu, a oznamuje sa capability reťazcom a migration note (to zmieruje `amend_not_additive`, odmietnutie `HEAD` ako revízie a `ctx_threads` bez stdin s pravidlom „nebumpovať“). `api.describe` vráti JSON Schema vstupov každej operácie; jeden `model_unavailable`, jeden parser `llm`, jeden pojem revízie (`stale_entry` všade, starý kód ako alias), stale guard prijme len plný commit alebo blob hash; replay vráti uložený receipt aj po neskoršej legitímnej úprave; `operations.get` nájde routované receipty; `entries.changed` číta committed config každej revízie a nesie `status`; `CommittedEntry` nesie `date`, `verify`, `review_by`, `supersedes`, `commits`, `author`. Bez `#[non_exhaustive]`/typovaných chýb, kým ich niekto nepoužíva (Hub podľa overovateľov dnes crate závislosť zrušil).
7. **Init prestane opakovať a prepisovať prácu**: `skip mod:` platí; `dec:` klastre, ktorých commity cituje existujúci záznam alebo `Decision:` trailer, sa neplánujú; stav `dec:` úloh odvodený z gitu (kolega a re-clone sa zhodnú); re-render zachová ručné `paths`/`tags`/`summary`; výber modulu a id klastrov v zoradenom poradí (dnes `max_by_key` nad HashMap); bot autori mimo skórovania; jazykovo neutrálne signály a `[init] decision_words` so slovenskými defaultmi, default až s fixtúrou `sk-history`; iba explicitné `--no-hooks` zapíše opt-out; fakty označia untracked súbory.
8. **Outbox nič ticho nestratí**: pokus sa počíta len adaptéru, ktorý sa naozaj skúsil; po N zlyhaniach `outbox.dead.jsonl` viditeľný v `status`, `kontext outbox retry`; eventy pre adaptéry preskočené cez `when` ostanú vo fronte; sync hlási mazanie a premenovanie; logy adaptérov rotujú.
9. **Záznamy sa parsujú tak, ako ich ľudia a formátery píšu**: viacriadkové flow listy, blokové listy a koncové komentáre; neparsovateľné je chyba `check`, nie varovanie; BOM; status edit obmedzený na blok polí po H1 so zachovaním koncov riadkov; `retired`/`archived`/emoji statusy → deprecated s varovaním; ne-latinkové názvy dostanú krátky hash id; jedno zdokumentované kotvenie literálnych ciest; `check` hlási neparsovateľné a budúce riadky ledgera.
10. **Docs a CLI zodpovedajú binárke**: doc test porovná nástroje v README, `02-mcp` a integračných stránkach s `tools::tool_list()` a `TAKES_DIR` (`ctx_drift` dostane `dir`); `init.jobs` sa číta, `project.description` zmizne; `ctx_init_submit` schéma s `status`, `tags`, reťazcovými formami; FAQ, roadmap, `tasks/knowledge-ideas.md` so stavom shipnuté/otvorené, CONTRIBUTING tabuľky (en, sk), brief docs s riadkom overenia a jeho cenou, porovnanie s Claude Code; audit skill stránkuje a nepoužíva `v`; usage chyby CLI exit 64 (nie 2 ako „drift found“); príklad command adaptéra s `--`; varovania pri neznámych kľúčoch configu a tieňovaní.

### Vlna 4 — 0.11, prvotriedne na platformách 2026 (začína, keď je eval z 0.10 zelený; najprv prečítať zverejnenú špecifikáciu 2026-07-28 a hook docs hostiteľov, lebo plán stojí na rešeršných zhrnutiach)

1. **Dual-era MCP server**: `server/discover`, `_meta` na požiadavke bez `initialize`, `resultType`, `ttlMs`/`cacheScope`, deterministické poradie zoznamov, `-32022` pri neznámej verzii, legacy `initialize` s fallbackom na 2025-11-25 (nie 2025-06-18), zrušená logging capability, neznámy tool `-32602`, `resources/list` so stránkovaním; `listChanged:true` len pri `expose`.
2. **Riadiace znalosti prídu pri editácii cesty**: `kontext hook --host claude|codex|cursor` číta hook JSON zo stdin a píše tvar hostiteľa; SessionStart (startup, resume, clear, compact) a SubagentStart dostanú brief; PreToolUse na Edit|Write|MultiEdit (Codex apply_patch, Cursor postToolUse) dostane riadiacu sekciu pre `tool_input.file_path`, len záznamy v session ešte nepovedané alebo so zmeneným blob hashom; stav session v `.git/kontext/sessions/<id>.json`; **jeden** rozpočet `session_budget_tokens` zdieľaný s `ctx_brief` (je to ten istý rozpočet nastavený na jednom mieste, nie druhý rozpočet); limity hostiteľov (10 000 znakov Claude, ~2 500 tokenov Codex); iba fakty, iba committed accepted záznamy, záznam s lint nálezom (imperatívy pre agenta, `curl | sh`, verify riadky v tele) iba odkazom; fail-open (exit 0 bez výstupu), cieľ 150 ms na 250-záznamovom store, kill switch `KONTEXT_HOOKS=off` a `[hooks] enabled=false`; inštaluje sa len `connect --hooks`, default až po A/B. Voliteľný Stop hook iba `additionalContext`, nikdy block. Eval scenár „kompaktácia“.
3. **Štruktúrované výsledky**: `outputSchema` + `structuredContent` pre `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_inbox`, `ctx_drift` (text ostáva), `resource_link` bloky na `kontext://entry/{id}`, `brief --format json` so skutočnou štruktúrou (riadiaca sekcia, stav, počty, currency, čo bolo orezané).
4. **Poctivé schvaľovanie exponovaných nástrojov**: anotácie a title upstream nástrojov prejdú ďalej, chýbajúce default `readOnlyHint:false`, `openWorldHint:true`; `connect codex --write` zapíše per-tool approval (approve pre read, prompt pre `ctx_verify`, `ctx_api`, `ctx_init`, exponované), kde to Codex nevie, varovanie.
5. **Skilly, pluginy, objaviteľnosť**: skilly do `.agents/skills` aj `.claude/skills`, validované voči Agent Skills spec, hash marker chráni tímové úpravy; Claude Code plugin (`.claude-plugin/plugin.json` + marketplace: `.mcp.json`, hooks, skills) a Codex plugin z tých istých zdrojov ako `connect`; AGENTS.md kanonický, do existujúceho CLAUDE.md import `@AGENTS.md`; aliasy parametrov (`id`/`uri`, `focus`/`paths`, `limit`/`max`, `budget_tokens` všade, `reason` = `why` = `because`) namiesto premenovaní; strop veľkosti `tools/list` ako test.
6. **Robustnosť servera**: `catch_unwind` okolo požiadavky (`-32603`, `in_flight` vždy klesne, zámky prežijú poisoning), `notifications/cancelled` zabije verify process group, HTTP MCP klient po 404 re-inicializuje session, kontexty cez `dir` v ohraničenej LRU, mimo git repozitára to server povie a `dir` funguje.

### Horizont — 1.0 (neplánované, začína po 0.11 a po A/B reporte)

- **A/B harness výsledkov** (skript + `tests/ab/`, mimo binárky): aspoň 20 párov issue → PR na rameno, z tohto repa a jedného spotrebiteľského, s briefom a hookmi a bez nich; meria úspech, porušenia riadiacich pravidiel zachytené pri commite, tokeny, cenu. Report nastaví `session_budget_tokens` a rozhodne, či hooky budú default-on a či sa shipnú natívne exporty. ETH štúdia AGENTS.md ukazuje, že nevyberaný kontext zhoršuje úspech; qrels merajú retrieval, nie výsledok.
- **Kotvené tvrdenia** (`anchors:` cesta#symbol, citovaný riadok, `test:<meno>`, riešené `git grep` raz na HEAD, cache podľa blob id, len pre servírované záznamy; zlomená kotva označí „evidence moved“ v briefe, drift a API, nikdy neskryje; `code:` evidencia sa pri capture vyrieši na sha).
- **Verify v CI pre dotknuté cesty**: `kontext ci --verify-touched` spustí príkazy záznamov riadiacich zmenené súbory **v znení base revízie** (`git show <base>:<file>`), príkazy pridané alebo zmenené v rozsahu vypíše ako „new, not run in CI“, nikdy nepíše do ledgera, nepotrebuje lokálny trust. Toto je jediný mechanizmus, pri ktorom PR nemôže prepašovať príkaz bežiaci s CI credentials.
- **Schvaľovanie pre aplikácie** (Peer): nie podľa transportu (`kontext api` je obyčajný CLI podpríkaz, zavolá ho každý Bash agenta); cez MCP elicitation, na ktorú odpovedá človek v UI klienta, alebo Peer zapíše pin z vlastného UI po zobrazení príkazu. Do rozhodnutia o route ostávajú Peerove automatické kontroly na `untrusted_verify` (otvorená otázka 1).
- **Natívne exporty pravidiel** (opt-in, gated na A/B): `kontext export rules --target claude-rules|cursor-rules|copilot-instructions|agents-md [--write|--check]`, generované súbory označené ako derived, len L0 riadky a id aktívnych accepted záznamov, `check` zlyhá pri zastaraní.
- **Organizačné balíky** (vendorované v `.ai/org/`, bumpované botím PR, obsah nikdy nebeží), **deterministická PR review akcia** (komentuje, neblokuje), **provenance na capture** (agent, model, session, transport, príznak externého obsahu) a lint imperatívneho textu, **MCP Registry/MCPB**, **as-of dotazy z git objektov**, **ingest osobných pamätí agentov do súkromného inboxu**.
- **Kritickosť záznamu** ako signál oddelený od relevancie a autority: v tvojich vláknach ostala nerozhodnutá; literatúra (2604.12007) varuje pred statickým skóre pri zápise a navrhuje hodnotu aktualizovanú výsledkami, iný agent chcel ľudské schválenie. Patrí k A/B harnessu, nie pred neho.

### Ako to rozdeliť medzi sessions

| session | vlastní | môže bežať súbežne s |
| --- | --- | --- |
| S1 bezpečnosť (0.9.1) | `api_write.rs` (verify, amend), `verification.rs`, `config.rs`, `util.rs`, `tools.rs` (anotácie), `connect.rs`, skilly, docs | S2 |
| S2 brány | `.github/`, `tests/common`, `tests/api_contract.rs`, `tests/eval.rs` (exit status), `scripts/release-check.sh`, CONTRIBUTING | S1, potom S3 až S6 |
| S3 brief + focus | `ops.rs` (brief, currency), `glob.rs`, `route.rs`, eval scenáre | S4, S5, S6 |
| S4 index + server | `index.rs`, `app.rs`, `repo.rs`, `mcp.rs`, `tests/mcp.rs` | S3, S5, S6 |
| S5 store | `store.rs` (supersession, parser), `ops.rs` promote až po S3 | S3 (bez `ops.rs`), S4, S6 |
| S6 údržba | `freshness.rs`, `drift.rs`, `ledger.rs`, `secrets.rs`, `events.rs`, `threads.rs` | S3, S4, S5 |

Eval očakávania každej vlny píše iná session než implementačná. Dve sessions na tom istom horúcom súbore nie; na disjunktných moduloch áno, ako doteraz.

## 6. Rozhodnutia, ktoré treba zaznamenať do `.ai/`

Každé ako samostatný záznam s explicitným `supersedes` alebo `extends` a s `verify` príkazom, promované v tom istom commite ako kód.

1. **Trust piny sú SHA-256 nad všetkým zdieľaným, čo sa dostane do adaptéra, a pinuje len človek pri termináli** — supersedes 2026-09-28-repository-declared-adapters…; v texte: starý FNV pin sa odmieta, zdieľané vars neprepisujú používateľské, `project.name` je slug, gate je voči procesu editujúcemu `trust.toml` honour-based, trust pokrýva text tabuľky, nie skripty (F99). verify: `cargo test --test cli trust_gate_skips_untrusted_and_revokes_on_edit`.
2. **`verify:` je kód z repozitára: beží len po pine jeho textu** — nový záznam, extends 1 a 2026-10-27-a-capture-names-the-command…; v texte rozsah: pin viaže súhlas na text, nie na kód, ktorý príkaz spustí; cez MCP nepinnutý príkaz sa vráti agentovi; CI spúšťa len znenie base revízie; žiadna MCP operácia nepinuje. verify: `cargo test --test maintenance verify_refuses_unpinned_command`.
3. **`ctx_verify` je destruktívny a open-world nástroj** — supersedes klauzulu 2026-10-15-mcp-annotations… „lokálne zápisy sú nedeštruktívne“ pre tento jeden nástroj, s dôsledkom: Codex pod `approval_policy=never` (Superset sessions) ho odmietne a agent príkaz spustí sám. Read nástroje volajúce adaptéry si ponechajú rozhodnuté anotácie (F69), kým to niekto nezmení samostatným rozhodnutím.
4. **Dodatok iba pridáva datovanú sekciu, model nikdy nepíše do záznamu** — supersedes 2026-10-29-a-decision-is-never-amended…, zachová odmietnutie rozhodnutí a pravidlo `extends`, odstráni model fold; jediné in-place úpravy sú metadátové riadky (`verify`, `checked`, `status` a `superseded_by` pri retire/supersede, `repair` dopĺňajúci chýbajúcu stranu). verify: `cargo test --lib stage_amend_refuses_rewritten_body`.
5. **`since` a freshness počítajú commity podľa ancestry od commitu pohľadu, zlyhaný verify nie je pohľad** — supersedes 2026-10-20-freshness…, a výslovne mení klauzulu 2026-10-27 „overenie je najnovší z riadkov ledgera a front matter“ (inak ostane aktívna a kód jej odporuje). verify: `cargo test --test maintenance since_counts_branch_merged_after_the_look`.
6. **Zameraný brief nikdy ticho nevyhodí riadiaci záznam** — extends 2026-10-27-a-verification-is-a-commit…; poradie podľa špecifickosti a veku, pretečenie vymenované, orezané druhy spočítané, päta v rozpočte, cesty sú cesty aj pred vznikom súboru, stampy sa nikdy nepíšu ako „verified“. verify: eval scenár „every governing entry survives a focused brief“.
7. **Supersession sa odvodzuje od aktívnych nástupcov, referencie sú plné id** — extends 2026-09-28-reuse-existing-adr…; `## Status` adr-tools vo vlastnom štýle. verify: `cargo test --lib successor_alone_supersedes`.
8. **Redakcia zlyháva zatvorene, súkromné zoznamy sa pridávajú k repozitárovým** — nový. verify: `cargo test --lib distill_refuses_invalid_redact_pattern`.
9. **Námietka ostáva „resolving“, kým jej vyriešenie nepristane v HEAD** — extends 2026-10-19-challenges-resolve…; cleanup sa viaže na zmenu blobu.
10. **Čo je overené: čítanie človekom alebo prejdený verify; stampy sú pohľady, nikdy „verified“; `checked:` je cache** (0.10) — supersedes 2026-10-27 **so zachovaním** jeho klauzúl „append or supersede, never rewrite“, „superseding cascades as a question“ a kontraktu API polí (syntéza ich v zhrnutí vypustila, čo by rozhodnutie 4 pripravilo o oporu). TTY nie je dôkaz človeka; aplikačné čítania nesú `application:<meno>`.
11. **Drift potrebuje základ a pohľad v alebo po poslednej riadenej zmene** (0.10) — supersedes 2026-10-20-drift…
12. **Politika kompatibility API v1** (0.10) — extends 2026-10-15-applications-consume…; so zúžením dovoleným len pri rozpore so zaznamenaným rozhodnutím alebo bezpečnostným sľubom.
13. **Zmeny rankingu potrebujú graded dôkazy; skladané a stemované polia sú tiene s nižším boostom** (0.10) — extends 2026-09-28-local-recall-is-tantivy-bm25…
14. **Každý zápis do zdieľaných znalostí alebo inboxu ide cez aplikačnú službu** (0.9.2 a 0.10) — extends 2026-10-15-git-is-shared-truth…
15. **Hooky doručujú riadiace znalosti v jednom rozpočte session a ostávajú opt-in do A/B reportu** (0.11) — nový.
16. **Release brány a red-first regresné testy** — konvencia: tag publikuje až po fmt, clippy, testoch na Linuxe a macOS, MSRV, docs build, EN/SK parite, `kontext ci` nad vlastným store, API snapshot a Peerových suitách; prvý commit opravy je zlyhávajúci test; eval očakávania píše iná session. Bez predpisu kadencie: tvoje tempo je tvoja vec, brány ho majú udržať bezpečné.

## 7. Čo nerobiť

- Žiadne embeddingy, vektorové úložisko, reranker, LLM sudca ani LLM graf v jadre; husté a rerank signály len ako adaptéry, merané na qrels. Žiadny model nepíše do textu záznamu (fold v amends, konsolidácia, reflexia, auto-aplikované importy); model iba označuje.
- Nepovažovať príkaz na default vetve dôveryhodného repozitára za súhlas; žiadny MCP nástroj ani `ctx_api` operácia nepinuje; CI spúšťa len znenie base revízie. Nehlásať, že „pin plus allowlist uzatvára sľub“: neuzatvára, rozsah treba napísať.
- Netvrdiť, že transport je autorita (`kontext api` je CLI a volá ho každý agent) ani že TTY je človek.
- Starý FNV pin ticho nepovyšovať na SHA-256. Nehašovať skripty, ktoré adaptér spúšťa (F99), ale zdokumentovať rozsah.
- Nepodpisovať ani nereťaziť `confirmations.log` (F66); review TSV diffu je zvolený model integrity, `check` len varuje pri budúcich a neparsovateľných riadkoch.
- Nepoužívať OS sandboxy (sandbox-exec, bwrap, landlock) ako gate; sú to neskoršie, platformovo špecifické doplnky.
- Nerobiť capture idempotentný podľa obsahu a neautomergovať duplicitných kandidátov (F88): duplicity v súkromnom inboxe chytí človek pri review.
- Nemeniť rozhodnuté anotácie read nástrojov volajúcich adaptéry (F69) bez superseding rozhodnutia. Neprestať zapisovať `checked:`, kým Peer nečíta polia overenia. Nemeniť „agent/bulk stamp je pohľad pre freshness“ bez rozhodnutia a dát.
- Neskrývať superseded ani deprecated záznamy v search a `ctx_read`; demotovať a ukázať náhradu.
- Nepridávať samostatný rozpočet pre hooky alebo adaptéry; nerobiť hooky default-on pred A/B reportom; hooky nikdy neblokujú, nezachytávajú ani nevolajú model.
- Neupgradovať Tantivy na 0.27 kvôli foldingu a stemmingu; 0.26.2 to má.
- Neradiť riadiacu sekciu podľa anglických slov never/only/must; je to nemeraná a anglická heuristika (F45 v novom šate).
- Nerobiť literálne `[slug]` závislé od existencie súboru v pracovnom strome: význam záznamu nesmie závisieť od checkoutu.
- Nezačínať pred 0.11: MCP Registry/MCPB, natívne exporty, kotvy, as-of dotazy, organizačné balíky, PR review akciu, OPA bridge, import pamätí, Streamable HTTP, OAuth, MCP Apps, elicitation, Tasks, channels, `llms.txt`. Nebrať pred 0.11 tieto low zistenia, lebo nehovoria agentovi nepravdu ani neporušujú sľub: F44, F50, F51, F56, F57, F61, F136, F138, F141, F142, F144, F189, F192.
- Nebumpovať `api_version` pre žiadnu položku tohto plánu, nepremenúvať parametre (aliasy). Nesľubovať Windows nad rámec „binárka sa zbuilduje“, kým hooky a verify nemajú tam testy.
- Nenechať session, ktorá implementuje ranking, brief alebo overenie, písať vlastné eval očakávania či qrels štítky. Nepúšťať dve sessions na ten istý horúci súbor naraz.
- Nepomenúvať produkty v jadre a nepreberať ich čísla (exspirácie, prahy) bez dát.

## 8. Otvorené otázky pre teba

1. **Peerove automatické kontroly** (Hub rozhodnutie 2026-10-27-peer-runs-an-entrys-check-itself, `Knowledge.ts reviewChecks`) dostanú od 0.9.1 `untrusted_verify`. Chceš route cez elicitation/UI pin (človek vidí príkaz raz v Peeri), alebo má Peer príkaz len zobraziť s riadkom `kontext trust --verify`? Hubové rozhodnutie „slučka sa zatvorí bez kliku“ je s gate v rozpore a treba ho supersednúť alebo zúžiť.
2. Kým Peer nepinne 0.9.1: vypnúť v Peeri automatické verify na repozitároch, ktoré tím nevlastní, alebo stačí known-issue poznámka v 0.9.0?
3. `drafts.extend` s `llm`: po 0.9.1 pridá nález ako bol napísaný a model nepíše nič. Vyhovuje, alebo má model smieť formulovať iba pridanú sekciu (stále aditívne, stále reviewované)?
4. Stampy: ponechať „agent a bulk stamp resetujú freshness“ (2026-10-20, 2026-10-27) s poctivými štítkami a aditívnym `since_verified`, alebo má počet resetovať len čítanie človekom a prejdený verify?
5. Qrels: označíš slovenský rez sám? Môže jeden rez pochádzať z vocabulift store (213 až 227 záznamov) a je na stroji dostupný pre `release-check.sh`? (Pri analýze nebol: `~/.peer/worktrees` prázdne.)
6. Backlog 1.4: jeden rozpočet na session pokrýva aj Peerove vlastné injekcie a výsledky adaptérov, alebo len brief, nástroje a hooky kontextu?
7. Duplicitné id naprieč druhmi: chyba všade (ako `check_all`), alebo legálne (ako `Refs`)? Plán zjednocuje staged check s `check_all`.
8. Union merge ledgera na GitHube/GitLabe cez merge button nikto neoveril; jeden reálny PR merge to rozhodne. Ak server union nectí, treba lokálne merge alebo ledger rozdelený po súboroch.
9. `prune_worktree_state` maže receipty worktreeov, ktorých cesta zmizla; Peer worktrees vytvára a maže na každú session. Chceš receipty držať čas (napr. 30 dní) namiesto mazania podľa neprítomnosti cesty?
10. `ctx_threads` stráca obsah („…“ orezy, 186-ťahové vlákno v jednej časti) — tvoja vlastná bolesť z vlákien, ktorú čítači nezachytili ako zistenie. Pridať do 0.9.2 položku vernosti (bez orezania stredu, hlásenie, čo bolo orezané)?
11. Kritickosť záznamu ako samostatný signál: nechať pri A/B harnessi, alebo chceš skôr jednoduchý, ľudsky schvaľovaný príznak `critical: true` v front matter, ktorý brief nikdy nevyreže?
12. Kadencia: syntéza navrhla „najviac jeden minor za dva týždne“; kritik právom namietol, že to nemá oporu v dátach a odporuje tvojmu tempu (šesť minorov za desať dní, dva pretekajúce agenty zlúčené pri freshness). Môj návrh: tempo si urči sám, brány z vlny 2 ho majú udržať bezpečné.
