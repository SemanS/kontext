# Plán

Nápady, o ktorých uvažujeme, zhruba v poradí. Názory a pull requesty sú vítané. Otvor [diskusiu](https://github.com/SemanS/kontext/discussions) alebo [issue](https://github.com/SemanS/kontext/issues).

## Ďalej

- **Distribúcia**: Homebrew tap, binárky pre Linux ARM, crates.io.
- **Viac presetov**: služby pamäte v štýle Mem0 / Zep, vyhľadávanie v issues GitHubu a GitLabu, Linear/Jira, Confluence/Notion ako zdroje znalostí len na čítanie.
- **Symboly cez tree-sitter** ako voliteľná funkcia pre presnejšie fakty o moduloch bez adaptéra kódu.
- **Úlohy na aktualizáciu rozhodnutí**: navrhnúť úlohy `refresh:` pre rozhodnutia označené [kontrolou aktuálnosti znalostí](../reference/03-configuration.md#freshness). Varovania o aktuálnosti sú dostupné v briefe a v `kontext status`.

## Neskôr

- **Pomocníci pri review**: GitHub Action, ktorá do pull requestov zapíše znalosti pokrývajúce zmenu (`ctx_prepare_commit` pre posudzovateľov).
- **Znalosti naprieč repozitármi**: konvencie celej organizácie zdieľané mnohými repozitármi, zapojené do každého briefu.
- Podpora hookov vo **Windows**.
- **Príklad adaptéra s embeddingami**: malá lokálna služba s embeddingami ako adaptér `search` pre tímy bez systému pamäte.

## Neplánujeme

- Vstavanú vektorovú databázu alebo LLM v jadre: tie zostanú adaptérmi.
- Hostovanú službu: kontext je lokálny nástroj; tvojím serverom je tvoj git hosting.
