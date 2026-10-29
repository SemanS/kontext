# Prispievanie do kontextu

[English](CONTRIBUTING.md) / Slovenčina

Vďaka, že pomáhaš! Vítané sú opravy chýb, nové presety, dokumentácia, preklady aj nápady.

## Skôr než začneš

- **Otázky a nápady** → [Diskusie](https://github.com/SemanS/kontext/discussions).
- **Chyby** → [issue](https://github.com/SemanS/kontext/issues/new/choose) s výstupom `kontext --version` a `kontext status` a krokmi na zopakovanie.
- **Väčšie zmeny** (nové schopnosti, zmeny konfigurácie alebo formátu záznamov) → najprv otvor issue alebo diskusiu, aby sme sa dohodli na návrhu. Vlastné rozhodnutia projektu sú v [`.ai/decisions/`](.ai/decisions/); zmena, ktorá niektorému odporuje, by mala prísť s rozhodnutím, ktoré ho nahradí.

## Príprava vývojového prostredia

Požiadavky: Rust 1.90+ (`rustup`, `rust-version` v `Cargo.toml`), git 2.31+ a Node 20+ pre dokumentačný web.

```sh
git clone https://github.com/SemanS/kontext && cd kontext
cargo build --locked
cargo test --locked                                   # unit tests + an end-to-end test on a throwaway repository
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# try your build on a scratch clone of a real repository
cargo build --release
git clone --no-hardlinks <some-repo> /tmp/scratch && cd /tmp/scratch
KONTEXT_CONFIG_DIR=/tmp/kontext-config ~/path/to/kontext/target/release/kontext init
```

`KONTEXT_CONFIG_DIR` drží pokusy mimo tvojej osobnej `~/.config/kontext`.

### Dokumentácia

```sh
cd docs
npm ci
npm run docs:dev        # http://localhost:5173/kontext/
npm run docs:build      # what CI runs; fails on dead links
```

Anglické zdrojáky sú v `docs/en/`, slovenské v `docs/sk/`. Oba stromy si súbor po súbore zodpovedajú. Keď zmeníš stránku, uprav aj druhý jazyk, alebo to spomeň v pull requeste, aby mohol nadviazať prekladateľ.

## Štruktúra projektu

| Cesta | Čo |
| --- | --- |
| `src/main.rs`, `src/cli.rs` | tenký vstup binárky a CLI (chyby použitia končia kódom 64) |
| `src/lib.rs`, `src/application.rs` | súkromné moduly enginu a stabilná fasáda na vloženie |
| `src/app.rs` | kontext procesu: repozitár, konfigurácia, register adaptérov a vyhľadávací index |
| `src/api.rs`, `src/api_read.rs`, `src/api_write.rs` | verzovaná aplikačná služba zdieľaná CLI, MCP a knižnicou |
| `src/mcp.rs`, `src/tools.rs` | MCP server (obe éry protokolu) a nástroje pre agentov |
| `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| `src/model.rs` | výsledky z indexu a adaptérov, spojené a v rozpočte |
| `src/route.rs`, `src/repo.rs` | ktorému repozitáru patrí cesta (submoduly, worktree, `dir`); zisťovanie gitu a pomocné príkazy |
| `src/glob.rs` | globy v štýle gitu pre `sources`, `paths` záznamov a allowlisty |
| `src/store.rs`, `src/inbox.rs` | záznamy znalostí (oba štýly, nahrádzanie, parsovanie) a lokálni kandidáti |
| `src/index.rs` | index Tantivy vrátane skladaných tieňových polí |
| `src/freshness.rs` | záznamy, ktorých `paths` sa od posledného pohľadu na ne výrazne zmenili |
| `src/drift.rs` | záznamy, ktorých kód rozsah commitov (alebo pracovný strom) zmenil bez pohľadu na ne |
| `src/ledger.rs`, `src/verification.rs` | denník overení (`.ai/confirmations.log`); ako bol každý záznam overený, čo sa odvtedy zmenilo, zmenené predpoklady, príkazy `verify` a ich trust piny |
| `src/evidence.rs`, `src/related.rs` | odkiaľ záznam pochádza; ako nový text súvisí so zaznamenanými záznamami |
| `src/signals.rs` | heuristiky commitov v tvare rozhodnutia (`why`, história initu) |
| `src/secrets.rs` | detekcia a maskovanie tajných údajov |
| `src/threads.rs`, `src/distill.rs` | prepisy agentov a destilácia znalostí z nich |
| `src/connect.rs`, `src/skills.rs` | pripájanie klientov agentov, hooky hostiteľov a skills |
| `src/adapters/` | register, drivery, MCP klient, mapovanie výsledkov |
| `src/init/` | scan, history, render, deepen, symboly |
| `src/hooks.rs`, `src/events.rs` | git hooky a hooky hostiteľov agentov; outbox |
| `src/config.rs`, `src/template.rs`, `src/jpath.rs` | konfigurácia, šablóny, JSON cesty |
| `src/presets.rs`, `presets/` | presety adaptérov (obyčajné TOML) |
| `src/util.rs` | drobné spoločné pomocníky |
| `skills/` | recepty `SKILL.md` skompilované do binárky |
| `tests/` | integračné testy: `cli.rs` (end to end), `api.rs`, `drift.rs`, `maintenance.rs`, `eval.rs` s `tests/eval/` |
| `docs/` | dokumentačný web (VitePress) |

Tabuľku pregeneruj, keď pridáš alebo odstrániš modul: `ls src src/*/` a riadok `//!` na začiatku každého súboru.

## Pull requesty

- Nech sú sústredené; jedna téma na pull request.
- Pridaj alebo uprav testy: unit testy pri kóde, end-to-end test pre správanie naprieč celkom.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` a `cargo test` musia prejsť (spúšťa ich CI).
- Aktualizuj dokumentáciu (podľa možnosti v oboch jazykoch) a pri zmenách viditeľných pre používateľov `docs/*/about/02-changelog.md`.
- Správy commitov sa riadia [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:` …).
- Ak za zmenou stojí rozhodnutie o návrhu, zapíš ho: `kontext capture --kind decision …` a povýš ho do pull requestu. Hooky kontextu doplnia trailer.

## Oprava začína zlyhávajúcim testom {#fixes-start-with-a-failing-test}

- **Najprv červený.** Prvý commit opravy je test alebo eval scenár, ktorý na súčasnom kóde zlyhá; jeho zlyhávajúci výstup vlož do pull requestu. Oprava príde v ďalšom commite.
- **Očakávania evalu píše iná session.** Kto implementuje zmenu poradia, briefu alebo overenia, nepíše jej eval očakávania ani štítky qrels; napíše ich iná session (alebo človek), pred zmenou, a commitnú sa prvé.
- **Jeden vlastník na horúci súbor.** `src/ops.rs`, `src/api_write.rs`, `src/store.rs`, `src/verification.rs` a `src/config.rs` mení naraz len jedna session alebo pull request. Disjunktné moduly sa dajú robiť paralelne.
- **Brány vydania.** Tag sa publikuje až po prejdení fmt, clippy, testov na Linuxe a macOS, MSRV jobu (číta `rust-version`), buildu dokumentácie, parity EN/SK, `kontext ci` nad vlastným `.ai/` tohto repozitára a snapshotu API.

## Pridanie presetu

1. Vytvor `presets/<name>.toml` s hlavičkou v komentári (čo to je, ktoré operácie/udalosti, premenné).
2. Použi `when`, aby bol neaktívny tam, kde jeho nástroj chýba.
3. Pri MCP nástrojoch uprednostni naväzovanie argumentov zo schémy pred natvrdo zadanými `args`.
4. Zaregistruj ho v `src/presets.rs` (`BUNDLED`) a zdokumentuj v `docs/en/adapters/03-presets.md` (a `docs/sk/…`).
5. V pull requeste napíš, ako si ho otestoval (výstup `kontext adapters test <name>`).

## Štýl

- Rust: idiomaticky, malé funkcie, žiadny `unwrap` na vstupe, ktorý ovláda používateľ, chyby s kontextom (`anyhow`).
- Text pre agentov (popisy nástrojov, briefy, reporty): krátky, konkrétny, s ohľadom na rozpočet.
- Komentáre v kóde vysvetľujú *prečo*, nie *čo*.

## Licencia

Prispievaním súhlasíš s tým, že tvoje príspevky sú duálne licencované pod licenciami MIT a Apache-2.0, ako opisuje [README](README_SK.md#licencia).
