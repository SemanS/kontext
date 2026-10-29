# Codex

## Pripojenie

Codex načítava MCP servery z `~/.codex/config.toml` (z `$CODEX_HOME/config.toml`, keď je `CODEX_HOME` nastavené, ako to robia obaly prepínajúce účty):

```toml
[mcp_servers.kontext]
command = "kontext"
args = ["mcp"]
# read tools run without asking; ctx_verify runs a shell command, ctx_init can write the store
# skeleton, ctx_api reaches every operation: those ask
default_tools_approval_mode = "approve"

[mcp_servers.kontext.tools.ctx_verify]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_api]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_init]
approval_mode = "prompt"
```

```sh
kontext connect codex            # prints the snippet
kontext connect codex --write    # appends it, or adds the approval modes to an existing entry (a timestamped backup is kept)
```

Nástroje, ktoré cez kontext vystavujú adaptéry, dostanú tiež `approval_mode = "prompt"`. Kde nainštalovaný Codex nevie nastaviť schvaľovanie po nástrojoch, `connect codex` to povie a rozhoduješ pri každom volaní.

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

Codex spúšťa hooky z `.codex/hooks.json`. `kontext connect codex --hooks --write` nainštaluje SessionStart a hook pred úpravou cez `apply_patch`, oba spúšťajú `kontext hook --host codex`; odpoveď ostane v rámci približne 2 500 tokenov a delí si rozpočet session s `ctx_brief`. Pridá aj pravidlo PreToolUse, ktoré zakáže `kontext trust` spustený cez shell. Opt-in do A/B reportu; pozri [Hooky](../guides/09-hooks.md).

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
