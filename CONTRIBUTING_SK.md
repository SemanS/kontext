# Prispievanie do kontextu

[English](CONTRIBUTING.md) / Slovenčina

Vďaka, že pomáhaš! Vítané sú opravy chýb, nové presety, dokumentácia, preklady aj nápady.

## Skôr než začneš

- **Otázky a nápady** → [Diskusie](https://github.com/SemanS/kontext/discussions).
- **Chyby** → [issue](https://github.com/SemanS/kontext/issues/new/choose) s výstupom `kontext --version` a `kontext status` a krokmi na zopakovanie.
- **Väčšie zmeny** (nové schopnosti, zmeny konfigurácie alebo formátu záznamov) → najprv otvor issue alebo diskusiu, aby sme sa dohodli na návrhu. Vlastné rozhodnutia projektu sú v [`.ai/decisions/`](.ai/decisions/); zmena, ktorá niektorému odporuje, by mala prísť s rozhodnutím, ktoré ho nahradí.

## Príprava vývojového prostredia

Požiadavky: Rust 1.88+ (`rustup`), git 2.31+ a Node 20+ pre dokumentačný web.

```sh
git clone https://github.com/SemanS/kontext && cd kontext
cargo build
cargo test                                   # unit tests + an end-to-end test on a throwaway repository
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
| `src/main.rs` | CLI |
| `src/mcp.rs`, `src/tools.rs` | MCP server a nástroje pre agentov |
| `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| `src/store.rs`, `src/inbox.rs` | záznamy znalostí (oba štýly) a lokálni kandidáti |
| `src/index.rs` | index Tantivy |
| `src/freshness.rs` | rozhodnutia, ktorých `paths` sa od ich prijatia výrazne zmenili |
| `src/adapters/` | register, drivery, MCP klient, mapovanie výsledkov |
| `src/init/` | scan, history, render, deepen |
| `src/hooks.rs`, `src/events.rs` | git hooky, outbox |
| `src/config.rs`, `src/template.rs`, `src/jpath.rs` | konfigurácia, šablóny, JSON cesty |
| `presets/` | presety adaptérov (obyčajné TOML) |
| `tests/cli.rs` | end-to-end test |
| `docs/` | dokumentačný web (VitePress) |

## Pull requesty

- Nech sú sústredené; jedna téma na pull request.
- Pridaj alebo uprav testy: unit testy pri kóde, end-to-end test pre správanie naprieč celkom.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` a `cargo test` musia prejsť (spúšťa ich CI).
- Aktualizuj dokumentáciu (podľa možnosti v oboch jazykoch) a pri zmenách viditeľných pre používateľov `docs/*/about/02-changelog.md`.
- Správy commitov sa riadia [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:` …).
- Ak za zmenou stojí rozhodnutie o návrhu, zapíš ho: `kontext capture --kind decision …` a povýš ho do pull requestu. Hooky kontextu doplnia trailer.

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
