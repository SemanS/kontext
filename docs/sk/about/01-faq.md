# Časté otázky

## Je kontext systém pamäte ako OpenViking alebo Mem0?

Nie. Vlastní len malú, posúdenú časť (rozhodnutia, konvencie a poznatky tímu uložené v gite) a všetko ostatné pripája. Sémantická pamäť, archívy sessions a grafy kódu sa zapájajú ako adaptéry. Mnohé tímy používajú kontext *spolu so* systémom pamäte: pozri [návod pre OpenViking](../guides/03-openviking.md).

## Prečo neukladať všetko do vektorovej databázy?

Pretože to, na čo sa tím spolieha, musí byť posúditeľné, porovnateľné v diffe a verzované spolu s kódom, ktorý opisuje. Vektorové úložiská sú výborné vo vybavovaní a zlé v úlohe pravdy. kontext drží pravdu v Markdowne a sémantické vybavovanie dostáva od adaptérov.

## Volá kontext LLM?

V jadre nie. Brief, vyhľadávanie, drift, aktuálnosť, check aj hooky sú deterministické: žiadny model tam nič neradí, nepíše ani neposudzuje. Premýšľa agent, ktorého už používaš; kontext mu dáva malé úlohy a kontext s rozpočtom.

Model sa volá len tam, kde o to požiadaš, vždy cez adaptér `llm`, ktorý nakonfiguruješ (`llm-claude`, `llm-codex`, lokálna Ollama):

- `kontext init --deepen` ([autopilot](../guides/06-autopilot.md)) navrhne dokumenty modulov a rozhodnutia,
- `kontext distill` premení vlákna agentov na kandidátov v inboxe ([citlivé vlákna](../guides/08-distill-threads.md#sensitive-threads)),
- `entries.related` v aplikačnom API označí možné rozpory a `drafts.revise` navrhne náhradu, ktorú posúdi človek.

Model nikdy nepíše do existujúceho záznamu: `drafts.extend` pridá poznatok doslovne a každý návrh ide cez inbox a pull request.

## Koľko to stojí tokenov?

Brief má približne 1–2k tokenov (nastaviteľné). Vyhľadávanie vráti jeden riadok na výsledok v rámci rozpočtu. Detaily sa čítajú len na požiadanie (L1/L2).

## Posiela niekam môj kód?

Nie. Všetko je lokálne, pokiaľ nenakonfiguruješ adaptér, ktorý dáta posiela (a aj vtedy len to, čo deklarujú jeho operácie a udalosti). Autopilot posiela priložené úryvky LLM, ktorý si vybral. Súbory s tajnými údajmi sa nikdy nečítajú a zachytený text sa maskuje.

## Náš repozitár už má ADR.

kontext ich nájde a nové rozhodnutia zapisuje do toho istého adresára, v rovnakom štýle a číslovaní. Pozri [Zavedenie do existujúceho repozitára](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).

## Musia si kontext nainštalovať aj kolegovia?

Nie. `.ai/` je obyčajný Markdown a bloky v hookoch na strojoch bez binárky nerobia nič. Inštalácia im dá nástroje, hooky a synchronizáciu do ich vlastných adaptérov.

## Môžem ho používať v repozitári, ktorý nevlastním?

Áno, súkromne: vynechaj hooky, nedávaj `.ai/` do commitov alebo presuň úložisko do lokálne vylúčeného adresára. Pozri [Repozitáre, ktoré nevlastníš](../guides/01-bootstrap-an-existing-repository.md#repositories-you-do-not-own).

## A čo monorepá?

Workspaces (Nx, moon, Turborepo, pnpm/npm, Cargo, Go, uv) sa rozpoznajú; každý deklarovaný projekt sa stane modulom, veľké sa rozdelia na oblasti a brief sa zužuje cez `focus`.

## Windows?

Binárka sa pre Windows zbuilduje a brief, vyhľadávanie, MCP server ani CLI shell nepotrebujú. Git hooky, hooky hostiteľov a príkazy `verify` bežia cez `sh` a na natívnom Windows nie je otestované ani jedno; `sh` z Git for Windows použi na vlastné riziko, alebo WSL, ktoré funguje.

## Čo kontext zapisuje a kam? {#what-kontext-writes-where}

| Kam | Čo | Kto zapisuje |
| --- | --- | --- |
| `.ai/` (commitnuté) | záznamy, `kontext.toml`, `README.md`, `.gitattributes`, `confirmations.log`, postup initu | `init`, zachytenia, ktoré povýšiš, `confirm`, `verify`, `repair` |
| koreň repozitára (commitnuté) | označený blok v `AGENTS.md`, import `@AGENTS.md` v existujúcom `CLAUDE.md`, `.mcp.json`, `.cursor/mcp.json`, `opencode.json` | `kontext connect … --write` |
| `.claude/`, `.agents/`, `.codex/`, `.cursor/` (commitnuté) | hooky v `settings.json` a zákaz pre `kontext trust`, `skills/kontext-*/SKILL.md`, `hooks.json` | `kontext connect … --write`, `connect skills`, `--hooks` |
| git hooky (`.git/hooks` alebo `core.hooksPath`) | označený blok v `pre-commit`, `prepare-commit-msg`, `post-commit`, `post-merge`, `post-rewrite` | `init`, `kontext hooks install` |
| `.git/kontext/` (tento klon) | vyhľadávací index, inbox, outbox a `outbox.dead.jsonl`, receipty operácií, stav session hookov, cache, lokálny `config.toml` klonu | všetko počas behu |
| `~/.config/kontext/` (ty) | `config.toml`, `repos/<slug>.toml`, `trust.toml` (piny adaptérov a verify) | ty, `kontext trust`, `adapters add --scope user` |
| `~/.codex/config.toml` | záznam `[mcp_servers.kontext]` so zálohou s časovou pečiatkou | `kontext connect codex --write` |
| `~/.local/bin/kontext` | binárka | `install.sh` |

Odstránenie kontextu z repozitára:

1. `kontext hooks uninstall` (obnoví reťazené hooky),
2. zmaž záznamy kontextu z `.mcp.json`, `.claude/settings.json`, `.codex/hooks.json`, `.cursor/`, `opencode.json` a označený blok v `AGENTS.md`,
3. zmaž `.claude/skills/kontext-*` a `.agents/skills/kontext-*`,
4. zmaž `.ai/`, ak znalosti nechceš (ak ich necháš, ostanú čitateľným Markdownom),
5. zmaž `.git/kontext/`,
6. odstráň sekciu repozitára z `~/.config/kontext/trust.toml` a `repos/<slug>.toml`.

Odstránenie aj zo stroja: binárka `kontext`, `~/.config/kontext/` a záznam `kontext` v `~/.codex/config.toml`.

## Čím sa to líši od CLAUDE.md, AGENTS.md alebo pamäte Claude Code?

CLAUDE.md a AGENTS.md sú próza: koreňový súbor sa načíta každú session, vnorený CLAUDE.md, keď agent číta súbory pod ním, a Claude Code číta AGENTS.md natívne. Súbory `.claude/rules` sa dajú obmedziť na cesty, ale nemajú stav ani históriu a číta ich len Claude Code. Automatickú pamäť Claude Code píše model a zostáva na jednom stroji. kontext drží veľa malých záznamov s cestami, stavmi a autormi, schválených v pull requestoch, zoradených pre konkrétnu úlohu a prepojených s commitmi. Pár pokynov, ktoré potrebuje každá session, nechaj v CLAUDE.md; kontext navyše pridá do `AGENTS.md` krátky blok, ktorý agentom povie, aby ho používali. Pozri [Prečo nie niečo iné?](../getting-started/01-introduction.md#why-not)
