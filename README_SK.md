<div align="center">

<a href="https://semans.github.io/kontext/sk/" target="_blank">
  <img alt="kontext" src="docs/images/logo.svg" width="128" height="128">
</a>

### kontext: tímový kontext pre coding agentov

[English](README.md) / Slovenčina

<a href="https://semans.github.io/kontext/sk/">Dokumentácia</a> · <a href="https://semans.github.io/kontext/sk/getting-started/03-quickstart">Rýchly štart</a> · <a href="https://github.com/SemanS/kontext/releases">Vydania</a> · <a href="https://github.com/SemanS/kontext/issues">Issues</a> · <a href="https://github.com/SemanS/kontext/discussions">Diskusie</a>

<p>
  <a href="https://github.com/SemanS/kontext/releases"><img src="https://img.shields.io/github/v/release/SemanS/kontext?color=4f46e5&labelColor=black&logo=github&style=flat-square" alt="release"></a>
  <a href="https://github.com/SemanS/kontext/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/SemanS/kontext/ci.yml?branch=main&label=ci&labelColor=black&style=flat-square" alt="ci"></a>
  <a href="#licencia"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-white?labelColor=black&style=flat-square" alt="license"></a>
  <a href="https://github.com/SemanS/kontext"><img src="https://img.shields.io/github/stars/SemanS/kontext?labelColor=black&style=flat-square&color=ffcb47" alt="stars"></a>
  <a href="https://github.com/SemanS/kontext/issues"><img src="https://img.shields.io/github/issues/SemanS/kontext?labelColor=black&style=flat-square&color=ff80eb" alt="issues"></a>
  <a href="https://github.com/SemanS/kontext/commits/main"><img src="https://img.shields.io/github/last-commit/SemanS/kontext?color=0d9488&labelColor=black&style=flat-square" alt="last commit"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-orange?labelColor=black&style=flat-square&logo=rust" alt="rust 1.88+">
  <img src="https://img.shields.io/badge/MCP-stdio-0d9488?labelColor=black&style=flat-square" alt="MCP">
</p>

</div>

***

## Čo je kontext

kontext dá každému coding agentovi, ktorého používaš – Claude Code, Codex, Cursor, OpenCode, čomukoľvek, čo hovorí MCP – rovnakú krátku, **posúdenú** pamäť toho, *prečo je kód taký, aký je*, a cez bežné commity premení „agent na niečo prišiel“ na „tím to vie“.

Je to jedna rýchla Rust binárka s tromi tvárami: **MCP server** (`kontext mcp`), **CLI** a sada **git hookov**. Znalosti tímu žijú v repozitári ako krátke Markdown súbory. Všetko ostatné – sémantická pamäť, archívy sessions, inteligencia kódu, LLM – sa zapája ako **nakonfigurovaný adaptér**; kód kontextu nikdy nemenuje žiadny produkt.

```text
                 Claude Code · Codex · Cursor · OpenCode · any MCP client
                                         │  MCP (stdio)
                                ┌────────┴────────┐
                                │     kontext     │  ctx_brief · ctx_search · ctx_read · ctx_why
                                │  (Rust, 1 bin)  │  ctx_capture · ctx_prepare_commit · ctx_init …
                                └──┬─────┬─────┬──┘
             adapters (config) ────┘     │     └──── git hooks
   ┌──────────────┬──────────────┐       │       pre-commit: validate + secret-scan + index
   │ mcp driver   │ http driver  │       │       prepare-commit-msg: `Decision: <id>` trailers
   │ CodeGraph    │ OpenViking   │       │       post-commit/merge/rewrite: `sync` events
   │ Serena       │ Ollama       │       │
   │ Agent LCM    │ Slack hook   │       ▼
   │ sessions     │ …            │   TEAM TRUTH (git)                LOCAL, DERIVED, PRIVATE
   │ command drv  │              │   .ai/decisions/*.md   ◄─ PR ──   .git/kontext/inbox (candidates)
   │ claude -p    │              │   .ai/conventions/…               .git/kontext/…/index (Tantivy)
   │ codex exec   │              │   .ai/learnings/…                 outbox (adapter deliveries)
   └──────────────┴──────────────┘   .ai/architecture/…              init cache, sync snapshots
```

## Prečo kontext

- **Zdrojom pravdy je git a agenti ho nikdy sami nemenia.** Zachytenie skončí v lokálnom inboxe; znalosťou tímu sa stane, až keď ho povýšiš *do commitu* a posúdi sa v pull requeste vedľa kódu, ktorý vysvetľuje. → [Zachytávanie a review](https://semans.github.io/kontext/sk/concepts/03-capture-and-review)
- **Jeden brief s rozpočtom.** `ctx_brief` vráti aktívne rozhodnutia, konvencie, úskalia a mapu modulov v ~1–2k tokenoch, zoradené podľa ciest, na ktoré sa agent chystá siahnuť; detaily prídu na požiadanie v úrovniach L0/L1/L2. → [Vrstvy kontextu](https://semans.github.io/kontext/sk/concepts/04-context-layers)
- **„Prečo je to takto?“ priamo z gitu.** `ctx_why path[:line]` spojí rozhodnutia, ktoré cestu pokrývajú, zhrnutie jej modulu, commity v tvare rozhodnutí, `git blame` a tvoje adaptéry histórie sessions; commity nesú trailery `Decision: <id>`. → [Vyhľadávanie](https://semans.github.io/kontext/sk/concepts/05-retrieval)
- **Zavedenie krok za krokom.** `kontext init` prejde repozitár, vyťaží z histórie commity v tvare rozhodnutí a výmeny závislostí a za pár sekúnd zapíše fakty; tvoj agent ho potom prehlbuje po malých úlohách, ktoré sa dajú prerušiť a obnoviť. → [Inicializačný pipeline](https://semans.github.io/kontext/sk/concepts/06-init-pipeline)
- **Adaptéry, nie integrácie.** OpenViking, CodeGraph, Serena, Agent LCM, sessions a LLM CLI sú každý pár riadkov TOML nad generickými drivermi MCP, HTTP a command – s naväzovaním argumentov zo schémy, mapovaním výsledkov a udalosťami. → [Adaptéry](https://semans.github.io/kontext/sk/adapters/01-overview)
- **Rýchly, lokálny, bezpečný.** Zabudovaný index Tantivy, brief za ~50 ms, hooky za ~0,1 s, skenovanie tajných údajov pri každom commite, dôvera naviazaná na hash pre adaptéry deklarované v repozitári. → [Bezpečnosť](https://semans.github.io/kontext/sk/concepts/08-security)

## Rýchly štart

```sh
curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh   # → ~/.local/bin/kontext

cd your-repo
kontext init                          # scan → history → render → wire (git hooks) → deepen plan
git switch -c kontext/bootstrap && git add .ai && git commit -m "docs: bootstrap team knowledge"

kontext connect claude --write        # .mcp.json — or: claude mcp add --scope user kontext -- kontext mcp
kontext connect agents-md --write     # a short "how to use kontext" block in AGENTS.md
```

Potom v Claude Code spusti **`/mcp__kontext__kontext-init`** (alebo požiadaj ktoréhokoľvek pripojeného agenta, nech pokračuje v zavádzaní kontextu), prípadne nech prvý prechod urobí LLM: `kontext init --deepen --llm llm-claude`.

Každodenné použitie:

```sh
kontext brief --focus src/billing       # what an agent sees first
kontext why src/billing/round.ts:40     # decisions, module, history, blame
kontext capture --kind decision --title "Prices are integer cents" --paths "src/billing/**" --body "…"
kontext prepare-commit --promote <id>   # ships the decision in the same commit
kontext log                             # the decision timeline
```

Celý postup: [Rýchly štart](https://semans.github.io/kontext/sk/getting-started/03-quickstart) · zostavenie zo zdrojákov: [Inštalácia](https://semans.github.io/kontext/sk/getting-started/02-installation).

## Použi ho so svojím agentom

| Agent | Pripojenie | Návod |
| --- | --- | --- |
| **Claude Code** | `kontext connect claude --write` (+ `claude-hooks` pre brief na začiatku session) | [Claude Code](https://semans.github.io/kontext/sk/agent-integrations/02-claude-code) |
| **Codex** | `kontext connect codex --write` | [Codex](https://semans.github.io/kontext/sk/agent-integrations/03-codex) |
| **Cursor** | `kontext connect cursor --write` | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients) |
| **OpenCode** | `kontext connect opencode --write` | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients) |
| **Superset a orchestrátory** | nič navyše – worktree zdieľajú inbox aj postup | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients#superset-and-other-orchestrators) |
| **Akýkoľvek MCP klient** | príkaz `kontext`, argumenty `["mcp"]` | [Referencia MCP](https://semans.github.io/kontext/sk/reference/02-mcp) |

Nástroje agenta: `ctx_brief` · `ctx_search` · `ctx_read` · `ctx_why` · `ctx_log` · `ctx_capture` · `ctx_inbox` · `ctx_prepare_commit` · `ctx_init` · `ctx_init_submit` – plus prompty `kontext-init`, `kontext-commit`, `kontext-reflect`.

## Adaptéry

| Preset | Driver | Prináša |
| --- | --- | --- |
| [OpenViking](https://github.com/volcengine/OpenViking) | http | sémantické vybavovanie v `ctx_search`, čítanie `viking://`, zlúčené rozhodnutia zrkadlené do `resources/`, súkromné poznámky do `memories/` |
| CodeGraph | mcp | vyhľadanie symbolu pre `ctx_why <symbol>`, voliteľné federovanie nástrojov |
| [Serena](https://github.com/oraios/serena) | mcp | vyhľadávanie symbolov cez LSP |
| [Agent LCM](https://github.com/Team-Volt/agent-lcm) | mcp | história sessions naprieč prostrediami agentov vo vyhľadávaní, why a briefe |
| [sessions](https://github.com/nicknisi/sessions) | mcp | `why_did_this_change`, vyhľadávanie v sessions, primery |
| `llm-claude` / `llm-codex` / `llm-ollama` | command / http | LLM na prehĺbenie autopilotom |
| `slack-webhook` | http | oznamuje zlúčené rozhodnutia |

```sh
kontext presets
kontext adapters add openviking --var user=alice
kontext adapters test
```

Vlastný adaptér je pár riadkov TOML: [Vlastný adaptér](https://semans.github.io/kontext/sk/adapters/04-writing-an-adapter).

## Výkon

Release build na notebooku s Apple Silicon (init s teplou cache súborov; prvý studený beh monorepa trval 3,3 s):

| Repozitár | init (scan + history + render) | prvé vyhľadávanie (zostaví index) | ďalšie vyhľadávanie | brief | pre-commit hook |
| --- | --- | --- | --- | --- | --- |
| TypeScript Nx monorepo – 3,2k súborov, 1,5k commitov | 1,1 s | 0,47 s | 0,14 s | 0,04 s | 0,13 s |
| Rust/Python/TS moon workspace – 640 súborov | 1,1 s | 0,25 s | 0,11 s | 0,04 s | 0,11 s |

## Dokumentácia

**[semans.github.io/kontext/sk](https://semans.github.io/kontext/sk/)** – po slovensky a [anglicky](https://semans.github.io/kontext/).

- Začíname: [Úvod](https://semans.github.io/kontext/sk/getting-started/01-introduction) · [Inštalácia](https://semans.github.io/kontext/sk/getting-started/02-installation) · [Rýchly štart](https://semans.github.io/kontext/sk/getting-started/03-quickstart) · [Nastavenie agentov](https://semans.github.io/kontext/sk/getting-started/04-setup-for-agents)
- Koncepty: [Architektúra](https://semans.github.io/kontext/sk/concepts/01-architecture) · [Úložisko znalostí](https://semans.github.io/kontext/sk/concepts/02-knowledge-store) · [Integrácia s gitom](https://semans.github.io/kontext/sk/concepts/07-git-integration)
- Návody: [Zavedenie do existujúceho repozitára](https://semans.github.io/kontext/sk/guides/01-bootstrap-an-existing-repository) · [Tímový workflow](https://semans.github.io/kontext/sk/guides/02-team-workflow) · [OpenViking](https://semans.github.io/kontext/sk/guides/03-openviking)
- Referencia: [CLI](https://semans.github.io/kontext/sk/reference/01-cli) · [MCP](https://semans.github.io/kontext/sk/reference/02-mcp) · [Konfigurácia](https://semans.github.io/kontext/sk/reference/03-configuration)

Zdrojáky dokumentácie sú v [`docs/`](docs/). Tento repozitár zapisuje svoje vlastné rozhodnutia o návrhu kontextom – pozri [`.ai/`](.ai/README.md).

## Komunita a prispievanie

- **Otázky a nápady**: [Diskusie](https://github.com/SemanS/kontext/discussions)
- **Chyby a požiadavky na funkcie**: [Issues](https://github.com/SemanS/kontext/issues)
- **Prispievanie**: opravy chýb, presety, dokumentácia aj preklady sú vítané – pozri [CONTRIBUTING_SK.md](CONTRIBUTING_SK.md) ([EN](CONTRIBUTING.md)) a [Kódex správania](CODE_OF_CONDUCT.md)
- **Zoznam zmien**: [dokumentácia](https://semans.github.io/kontext/sk/about/02-changelog) · **Plán**: [dokumentácia](https://semans.github.io/kontext/sk/about/03-roadmap)

## Bezpečnosť

Zraniteľnosti, prosím, hlás súkromne – pozri [SECURITY.md](SECURITY.md).

## Licencia

kontext je duálne licencovaný pod jednou z licencií

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

podľa tvojej voľby. Pokiaľ výslovne neuvedieš inak, každý príspevok, ktorý zámerne odovzdáš na zaradenie do kontextu, v zmysle licencie Apache-2.0, bude duálne licencovaný tak, ako je uvedené vyššie, bez akýchkoľvek ďalších podmienok.
