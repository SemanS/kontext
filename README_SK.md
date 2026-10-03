<div align="center">

<a href="https://semans.github.io/kontext/sk/" target="_blank">
  <img alt="kontext" src="docs/images/logo.svg" width="128" height="128">
</a>

### kontext: deklaratívne tímové znalosti pre coding agentov

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

kontext je deklaratívny most medzi tvojimi coding agentmi a gitom. Rozhodnutia, konvencie a úskalia, ktorými sa agenti riadia, sú krátke Markdown súbory v repozitári. Agenti počas práce navrhujú nové, ľudia ich schvaľujú v pull requeste danej zmeny a každý agent (Claude Code, Codex, Cursor, OpenCode, akýkoľvek MCP klient) dostane výsledok v krátkom briefe pre cesty, na ktoré sa chystá siahnuť.

Je to jedna Rust binárka s tromi tvárami: **MCP server** (`kontext mcp`), **CLI** a sada **git hookov**.

```text
          Claude Code · Codex · Cursor · OpenCode · any MCP client
                                 │  MCP (stdio) or CLI
                        ┌────────┴────────┐
                        │     kontext     │  ctx_brief · ctx_why · ctx_search · ctx_read
                        │  (Rust, 1 bin)  │  ctx_capture · ctx_prepare_commit · ctx_init …
                        └────────┬────────┘
                                 │  git hooks: validate + secret scan · Decision: trailers · index
                                 ▼
   TEAM KNOWLEDGE (git, reviewed)                   LOCAL, DERIVED (.git/kontext/)
   .ai/decisions/*.md   ◄── commit + pull request ── inbox: candidates agents captured
   .ai/conventions/ · learnings/ · incidents/       search index (Tantivy), caches
   .ai/architecture/                                 deleted any time, rebuilt from git
```

## Prečo kontext

- **Deklarované, nie zapamätané.** Rozhodnutie je súbor, ktorý hovorí, čo platí, odkedy a pre ktoré cesty. Brief, vyhľadávanie a `kontext why` sa odvodzujú z týchto súborov a z histórie gitu, takže agenti vedia to, čo hovorí repozitár. Checkoutni minuloročné vydanie a uvidia rozhodnutia, ktoré platili vtedy. → [Úložisko znalostí](https://semans.github.io/kontext/sk/concepts/02-knowledge-store)
- **Agenti navrhujú, ľudia schvaľujú.** Zachytenie čaká v lokálnom inboxe, kým sa nepovýši do commitu a nezlúči cez pull request, pod tými istými CODEOWNERS a CI ako kód. → [Zachytávanie a review](https://semans.github.io/kontext/sk/concepts/03-capture-and-review)
- **Dohľadateľné ku commitom.** Commit, ktorý rozhodnutie prináša, nesie trailer `Decision:` a `kontext why src/billing/round.ts:40` spojí rozhodnutia, ktoré riadok pokrývajú, s jeho blame a históriou. Zrušené rozhodnutie nahradí nové, nezmaže sa. → [Integrácia s gitom](https://semans.github.io/kontext/sk/concepts/07-git-integration)
- **Jeden brief pre každého agenta.** `ctx_brief` vráti rozhodnutia, konvencie a úskalia pre cesty, na ktoré sa agent chystá siahnuť, v ~1–2k tokenoch, s detailmi na požiadanie. → [Vrstvy kontextu](https://semans.github.io/kontext/sk/concepts/04-context-layers)
- **Neaktuálne rozhodnutia vyplávajú.** Rozhodnutie, ktorého cesty od prijatia zasiahlo 20+ commitov, sa označí v briefe aj v `kontext status`. → [Aktuálnosť](https://semans.github.io/kontext/sk/concepts/04-context-layers#freshness)
- **Zavedenie krok za krokom.** `kontext init` za pár sekúnd prejde repozitár a vyťaží z histórie commity v tvare rozhodnutí; tvoj agent ho potom prehlbuje po malých úlohách, ktoré sa dajú prerušiť a obnoviť. → [Inicializačný pipeline](https://semans.github.io/kontext/sk/concepts/06-init-pipeline)

## Prečo nie pamäť Claude Code alebo CLAUDE.md?

| | CLAUDE.md, AGENTS.md | Automatická pamäť Claude Code | Služby pamäte | kontext |
| --- | --- | --- | --- | --- |
| Píše ju | ľudia, ako prózu | model, sám od seba | model, zo sessions | agenti navrhujú, ľudia schvaľujú |
| Zdieľaná a posúdená | v pull requestoch | nie: jeden používateľ, jeden stroj | podľa nasadenia, bez review | v pull requeste danej zmeny |
| K agentovi sa dostane | celý súbor, každú session | prvých 200 riadkov indexu, každú session | podľa podobnosti s dopytom | podľa ciest, ktoré mení |
| Záznam nesie | žiadny stav, dátum ani vlastníka | čas zápisu | podľa úložiska | cesty, stav, dátum, autora, commity |
| Keď zastará | zostane, kým si to niekto nevšimne | model ho môže prepísať | model ho môže prepísať | označí sa po 20 commitoch na jeho cestách |
| Funguje s | Claude Code; ostatní čítajú AGENTS.md | Claude Code | ich pluginom, SDK alebo MCP serverom | každým MCP klientom a CLI |

kontext CLAUDE.md nenahrádza: nechaj v ňom pár pokynov, ktoré potrebuje každá session. Rozhodnutí je priveľa na to, aby sa načítavali celé, a sú pridôležité na to, aby zostali neposúdené. → [Prečo nie niečo iné?](https://semans.github.io/kontext/sk/getting-started/01-introduction#why-not)

## Pod tvojou kontrolou

- **Schvaľovanie.** Znalosti sa menia len cez commity. S `/.ai/ @acme/architects` v CODEOWNERS a povinným review od vlastníkov kódu žiadny agent nezmení to, čím sa riadia všetci agenti, bez súhlasu tohto tímu.
- **Auditná stopa.** `git log -- .ai` a `kontext log` ukážu, kto čo rozhodol a kedy. `git log --grep "Decision: <id>"` nájde commit, ktorý rozhodnutie priniesol.
- **Pravidlá v CI.** `kontext check` zastaví pull request s chybným záznamom, duplicitným id alebo uniknutým tajným údajom. → [Validácia znalostí v CI](https://semans.github.io/kontext/sk/guides/07-ci)
- **Lokálny a súkromný.** Žiadny účet ani server; brief za ~50 ms, hooky za ~0,1 s. Nič neopustí počítač, kým to nenastavíš. → [Bezpečnosť](https://semans.github.io/kontext/sk/concepts/08-security)
- **Bez uzamknutia.** Úložisko je obyčajný Markdown a dá sa čítať aj bez kontextu.

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
| **Codex** | `kontext connect codex --write` (+ `agents-md`: Codex sa riadi `AGENTS.md`) | [Codex](https://semans.github.io/kontext/sk/agent-integrations/03-codex) |
| **Cursor** | `kontext connect cursor --write` | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients) |
| **OpenCode** | `kontext connect opencode --write` | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients) |
| **Superset a orchestrátory** | nič navyše: worktree zdieľajú inbox aj postup | [Ďalší klienti](https://semans.github.io/kontext/sk/agent-integrations/04-other-clients#superset-and-other-orchestrators) |
| **Akýkoľvek MCP klient** | príkaz `kontext`, argumenty `["mcp"]` | [Referencia MCP](https://semans.github.io/kontext/sk/reference/02-mcp) |

Nástroje agenta: `ctx_brief` · `ctx_search` · `ctx_read` · `ctx_why` · `ctx_log` · `ctx_threads` · `ctx_capture` · `ctx_inbox` · `ctx_prepare_commit` · `ctx_init` · `ctx_init_submit`, plus prompty `kontext-init`, `kontext-commit`, `kontext-reflect`, `kontext-distill`.

## Voliteľné adaptéry

kontext funguje aj bez nich. Ak už používaš sémantickú pamäť (OpenViking), graf kódu (CodeGraph, Serena) alebo archív sessions, adaptér ich pripojí pár riadkami TOML a `claude -p`, `codex exec` či lokálny model v Ollame vedia navrhnúť zavedenie. → [Adaptéry](https://semans.github.io/kontext/sk/adapters/01-overview)

## Výkon

Release build na notebooku s Apple Silicon (init s teplou cache súborov; prvý studený beh monorepa trval 3,3 s):

| Repozitár | init (scan + history + render) | prvé vyhľadávanie (zostaví index) | ďalšie vyhľadávanie | brief | pre-commit hook |
| --- | --- | --- | --- | --- | --- |
| TypeScript Nx monorepo (3,2k súborov, 1,5k commitov) | 1,1 s | 0,47 s | 0,14 s | 0,04 s | 0,13 s |
| Rust/Python/TS moon workspace (640 súborov) | 1,1 s | 0,25 s | 0,11 s | 0,04 s | 0,11 s |

## Dokumentácia

**[semans.github.io/kontext/sk](https://semans.github.io/kontext/sk/)**, po slovensky a [anglicky](https://semans.github.io/kontext/).

- Začíname: [Úvod](https://semans.github.io/kontext/sk/getting-started/01-introduction) · [Inštalácia](https://semans.github.io/kontext/sk/getting-started/02-installation) · [Rýchly štart](https://semans.github.io/kontext/sk/getting-started/03-quickstart) · [Nastavenie agentov](https://semans.github.io/kontext/sk/getting-started/04-setup-for-agents)
- Koncepty: [Architektúra](https://semans.github.io/kontext/sk/concepts/01-architecture) · [Úložisko znalostí](https://semans.github.io/kontext/sk/concepts/02-knowledge-store) · [Integrácia s gitom](https://semans.github.io/kontext/sk/concepts/07-git-integration)
- Návody: [Zavedenie do existujúceho repozitára](https://semans.github.io/kontext/sk/guides/01-bootstrap-an-existing-repository) · [Tímový workflow](https://semans.github.io/kontext/sk/guides/02-team-workflow) · [OpenViking](https://semans.github.io/kontext/sk/guides/03-openviking)
- Referencia: [CLI](https://semans.github.io/kontext/sk/reference/01-cli) · [MCP](https://semans.github.io/kontext/sk/reference/02-mcp) · [Konfigurácia](https://semans.github.io/kontext/sk/reference/03-configuration)

Zdrojáky dokumentácie sú v [`docs/`](docs/). Tento repozitár zapisuje svoje vlastné rozhodnutia o návrhu kontextom. Pozri [`.ai/`](.ai/README.md).

## Komunita a prispievanie

- **Otázky a nápady**: [Diskusie](https://github.com/SemanS/kontext/discussions)
- **Chyby a požiadavky na funkcie**: [Issues](https://github.com/SemanS/kontext/issues)
- **Prispievanie**: opravy chýb, presety, dokumentácia aj preklady sú vítané. Pozri [CONTRIBUTING_SK.md](CONTRIBUTING_SK.md) ([EN](CONTRIBUTING.md)) a [Kódex správania](CODE_OF_CONDUCT.md)
- **Zoznam zmien**: [dokumentácia](https://semans.github.io/kontext/sk/about/02-changelog) · **Plán**: [dokumentácia](https://semans.github.io/kontext/sk/about/03-roadmap)

## Bezpečnosť

Zraniteľnosti, prosím, hlás súkromne. Pozri [SECURITY.md](SECURITY.md).

## Licencia

kontext je duálne licencovaný pod jednou z licencií

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

podľa tvojej voľby. Pokiaľ výslovne neuvedieš inak, každý príspevok, ktorý zámerne odovzdáš na zaradenie do kontextu, v zmysle licencie Apache-2.0, bude duálne licencovaný tak, ako je uvedené vyššie, bez akýchkoľvek ďalších podmienok.
