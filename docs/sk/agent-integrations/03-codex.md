# Codex

## Pripojenie

Codex načítava MCP servery z `~/.codex/config.toml` (z `$CODEX_HOME/config.toml`, keď je `CODEX_HOME` nastavené, ako to robia obaly prepínajúce účty):

```toml
[mcp_servers.kontext]
command = "kontext"
args = ["mcp"]
# Read tools and the tools that write only this clone's inbox and working tree are approved
# below. ctx_verify runs the `verify:` command an entry names, ctx_init can bootstrap .ai/ and
# ctx_api reaches every operation, so those ask, as do tools kontext exposes from adapters.
default_tools_approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_brief]
approval_mode = "approve"

# … jedna tabuľka na každý nástroj kontextu: approve pre čítacie nástroje a inbox …

[mcp_servers.kontext.tools.ctx_verify]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_api]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_init]
approval_mode = "prompt"
```

```sh
kontext connect codex            # vypíše úryvok
kontext connect codex --write    # pripojí ho, alebo doplní chýbajúce tabuľky nástrojov do existujúcej položky (s časovo označenou zálohou)
```

Predvolené pre server je `prompt`, takže nástroj bez vlastnej tabuľky, napríklad ten, ktorý cez kontext vystavuje adaptér, sa pýta. Existujúca položka si ponechá svoje `default_tools_approval_mode` aj tabuľky nástrojov, ktoré si zmenil; keď je v nej `approve`, `connect codex` upozorní, že vystavené nástroje, a v Codexe bez `approval_mode` po nástrojoch aj `ctx_verify`, `ctx_api` a `ctx_init`, by potom bežali bez opýtania.

## Schvaľovanie

Codex sa pred volaním MCP nástroja pýta, pokiaľ nevie, že je neškodný, a pri `approval_policy = "never"` namiesto otázky volanie odmietne: `MCP tool call requires approval, but approval policy is never`.

| Nástroj | Anotácie | Schvaľovanie zapísané cez `connect codex` |
| --- | --- | --- |
| `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads` | `readOnlyHint: true` | approve |
| `ctx_capture`, `ctx_inbox`, `ctx_challenge`, `ctx_confirm`, `ctx_prepare_commit`, `ctx_init_submit` | nedeštruktívne zápisy do inboxu, pracovného stromu a indexu | approve |
| `ctx_init` | s `bootstrap=true` zapíše kostru úložiska | prompt |
| `ctx_verify` | `destructiveHint: true`, `openWorldHint: true`; spúšťa pinnutý shellový príkaz | prompt |
| `ctx_api` | `openWorldHint: true`; každá aplikačná operácia | prompt |
| nástroje vystavené z adaptérov | anotácie zo zdroja, inak `readOnlyHint: false`, `openWorldHint: true` | prompt |

Pri `approval_policy = "never"` (napríklad v Superset sessions) Codex `ctx_verify` odmietne. Je to zámer: agent namiesto toho spustí príkaz záznamu vo vlastnom sandboxe. `ctx_verify` beží v MCP serveri, mimo sandboxu agenta, a nepinnutý príkaz sa agentovi vráti, nespustí (pozri [Bezpečnosť](../concepts/08-security.md#verify)).

Pri `sandbox_mode = "workspace-write"` drží Codex `.git` pre vlastné shell príkazy agenta len na čítanie, takže `git add` / `git commit` tam zlyhajú; servera kontextu sa to netýka (`ctx_prepare_commit` stále povyšuje a stagne). Commituj zo sandboxu, ktorý to dovolí, alebo sám.

Spusti `codex` v repozitári; server zistí repozitár zo svojho pracovného adresára. Ak tvoje nastavenie spúšťa MCP servery inde, zafixuj repozitár cez `args = ["-C", "/path/to/repo", "mcp"]` alebo `env = { KONTEXT_DIR = "/path/to/repo" }`.

## Inštrukcie

Codex číta `AGENTS.md`. Pridaj doň pracovné pravidlá pre kontext:

```sh
kontext connect agents-md --write
```

Urob to v každom repozitári, kde má Codex kontext používať. Codex sa riadi `AGENTS.md` a z inštrukcií MCP servera číta len prvých 512 znakov, ktoré kontext drží samostatne zrozumiteľné; bez bloku dokáže aj tak dokončiť celú úlohu bez jediného volania kontextu.

## Hooky

Codex spúšťa hooky z `.codex/hooks.json`, a to len s `[features] codex_hooks = true` vo svojom `config.toml` (`connect` to vypíše a nezapíše). `kontext connect codex --hooks --write` nainštaluje `SessionStart` (matcher `startup|resume|clear|compact`) a `PreToolUse` s matcherom `apply_patch|Edit|Write`, oba spúšťajú `kontext hook --host codex`; odpoveď ostane v rámci približne 2 500 tokenov a rozpočtu session. Každé `--write` pridá aj hook `PreToolUse` na `Bash`, ktorý zamietne `kontext trust` spustený cez shell. Opt-in do A/B reportu; pozri [Hooky](../guides/09-hooks.md).

## Skills

`kontext connect skills --write` zapíše skills aj do `.agents/skills/`, kde ich Codex hľadá.

## Codex ako LLM pre autopilot

```sh
kontext adapters add llm-codex
kontext init --deepen --llm llm-codex --jobs 2 --max 10
```

Preset spúšťa `codex exec --skip-git-repo-check --sandbox read-only --ephemeral` a finálnu správu číta z `--output-last-message`; úloha vrátane relevantných úryvkov zo súborov sa odovzdáva cez stdin.

## Tipy

- Zapisovacie nástroje kontext sa dotýkajú iba pracovného stromu, git indexu a `.git/kontext/`. Výnimkou je `ctx_verify`: spúšťa pinnutý príkaz.
- `kontext call ctx_brief '{"focus":["src/api"]}'` ukáže presne to, čo dostane Codex.
