# Plán

Nápady, o ktorých uvažujeme, zhruba v poradí. Názory a pull requesty sú vítané. Otvor [diskusiu](https://github.com/SemanS/kontext/discussions) alebo [issue](https://github.com/SemanS/kontext/issues).

Hotové položky z tejto stránky odchádzajú; vymenúva ich [zoznam zmien](./02-changelog.md). Binárky pre Linux ARM, označovanie aktuálnosti, drift, kontroly v CI, hooky pre hostiteľov agentov a MCP rozhranie roku 2026 sú hotové.

## Ďalej

- **Distribúcia**: Homebrew tap, crates.io.
- **Viac presetov**: služby pamäte v štýle Mem0 / Zep, vyhľadávanie v issues GitHubu a GitLabu, Linear/Jira, Confluence/Notion ako zdroje znalostí len na čítanie.
- **Symboly cez tree-sitter** ako voliteľná funkcia pre presnejšie fakty o moduloch bez adaptéra kódu.
- **Aktuálnosť znalostí**: navrhnúť úlohy `refresh:` pre rozhodnutia, ktorých `paths` sa od rozhodnutia výrazne zmenili (brief a `kontext status` ich už označujú).

## Smerom k 1.0 {#toward-1-0}

Neplánované v čase; začne po A/B reporte o výsledkoch agentov.

- **A/B harness výsledkov**: páry issue → pull request s briefom a hookmi a bez nich, merajúce úspech, riadiace pravidlá zachytené pri commite, tokeny a cenu. Nastaví `session_budget_tokens` a rozhodne, či budú hooky predvolene zapnuté.
- **Kotvené tvrdenia**: `anchors:` (cesta a symbol, citovaný riadok, meno testu) vyriešené raz na `HEAD`; zlomená kotva označí záznam „evidence moved“ v briefe, drifte aj API, nikdy ho neskryje.
- **Verify v CI pre dotknuté cesty**: `kontext ci --verify-touched` spustí príkazy záznamov riadiacich zmenené súbory v znení base revízie, nové alebo zmenené príkazy vypíše ako nespustené, nepotrebuje lokálny trust a nikdy nezapisuje do denníka.
- **Schvaľovanie pre aplikácie**: človek schváli verify príkaz raz v UI klienta (MCP elicitation) namiesto terminálu.
- **Natívne exporty pravidiel** (opt-in): `kontext export rules --target claude-rules|cursor-rules|copilot-instructions|agents-md`, odvodené súbory tak označené, `--check` zlyhá pri zastaraní.
- **Organizačné balíky** vendorované v `.ai/org/` a bumpované botím pull requestom; **deterministická PR review akcia**, ktorá komentuje a nikdy neblokuje; **provenance pri zachytení**; zápis v **MCP Registry**; **dotazy k dátumu** z git objektov.

## Neskôr

- **Znalosti naprieč repozitármi**: konvencie celej organizácie zdieľané mnohými repozitármi, zapojené do každého briefu (pozri organizačné balíky vyššie).
- **Windows**: testy git hookov, hookov hostiteľov a verify príkazov na natívnom Windows.
- **Príklad adaptéra s embeddingami**: malá lokálna služba s embeddingami ako adaptér `search` pre tímy bez systému pamäte.

## Neplánujeme

- Vstavanú vektorovú databázu alebo LLM v jadre: tie zostanú adaptérmi.
- Hostovanú službu: kontext je lokálny nástroj; tvojím serverom je tvoj git hosting.
