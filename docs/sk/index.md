---
layout: home

hero:
  name: kontext
  text: Tímový kontext pre coding agentov
  tagline: Krátka, posúdená pamäť toho, prečo je tvoj kód taký, aký je. Uložená v gite, sprístupnená cez MCP každému agentovi, ktorého používaš, a s ľubovoľným systémom pamäte, histórie či kódu zapojeným ako nakonfigurovaný adaptér.
  image:
    src: /logo.svg
    alt: kontext
  actions:
    - theme: brand
      text: Začíname
      link: /sk/getting-started/01-introduction
    - theme: alt
      text: Rýchly štart
      link: /sk/getting-started/03-quickstart
    - theme: alt
      text: GitHub
      link: https://github.com/SemanS/kontext

features:
  - icon: 🧭
    title: Jeden brief pre každého agenta
    details: ctx_brief dá Claude Code, Codexu, Cursoru či OpenCode aktívne rozhodnutia, konvencie, úskalia a mapu modulov v ~1–2k tokenoch, zoradené podľa súborov, na ktoré sa agent chystá siahnuť.
  - icon: 🌿
    title: Zdrojom pravdy je git
    details: Rozhodnutia a poznatky sú krátke Markdown súbory v repozitári. Menia sa cez commity a pull requesty, takže tím posudzuje znalosti spolu s kódom, ktorý vysvetľujú.
  - icon: 📥
    title: Zachyť, potom povýš
    details: Agenti zachytávajú kandidátov do lokálneho inboxu. ctx_prepare_commit povýši tie relevantné do commitu, kam patria, a hook doplní trailery Decision:.
  - icon: 🔌
    title: Adaptéry, nie integrácie
    details: "OpenViking, CodeGraph, Serena, Agent LCM, sessions, LLM CLI: každý je pár riadkov TOML nad generickými drivermi MCP, HTTP alebo command. Jadro nepozná žiadny produkt."
  - icon: 🪜
    title: Zavedenie krok za krokom
    details: kontext init prejde repozitár, vyťaží z histórie commity v tvare rozhodnutí a za pár sekúnd zapíše fakty; agenti ho potom prehlbujú po malých úlohách, ktoré sa dajú kedykoľvek prerušiť.
  - icon: ⚡
    title: Rýchly, lokálny, súkromný
    details: Jedna Rust binárka so zabudovaným indexom Tantivy. Brief za ~50 ms, hooky za ~0,1 s, skenovanie tajných údajov pri každom commite. Nič neopustí tvoj počítač, kým to nepošle adaptér.
---
