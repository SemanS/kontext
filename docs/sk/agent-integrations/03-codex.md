# Codex

## Pripojenie

Codex načítava MCP servery z `~/.codex/config.toml`:

```toml
[mcp_servers.kontext]
command = "kontext"
args = ["mcp"]
# kontext's tools read the repository and write only the local inbox and the working tree;
# without this, approval_policy = "never" refuses every call
default_tools_approval_mode = "approve"
```

```sh
kontext connect codex            # prints the snippet
kontext connect codex --write    # appends it, or adds the approval mode to an existing entry (a timestamped backup is kept)
```

## Schvaľovanie

Codex sa pred volaním MCP nástroja pýta, pokiaľ nevie, že je neškodný, a pri `approval_policy = "never"` namiesto otázky volanie odmietne: `MCP tool call requires approval, but approval policy is never`. V takom nastavení kontext fungujú dve veci:

- každý nástroj kontextu nesie MCP anotácie – `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why` a `ctx_log` majú `readOnlyHint: true`, ostatné sú označené ako nedeštruktívne (zapisujú len do lokálneho inboxu, pracovného stromu a git indexu), takže ich Codex spustí bez pýtania;
- `default_tools_approval_mode = "approve"` v zázname servera schváli nástroje kontextu výslovne, nezávisle od toho, ako konkrétna verzia Codexu anotácie číta.

Pri `sandbox_mode = "workspace-write"` drží Codex `.git` pre vlastné shell príkazy agenta len na čítanie, takže `git add` / `git commit` tam zlyhajú; servera kontextu sa to netýka (`ctx_prepare_commit` stále povyšuje a stagne). Commituj zo sandboxu, ktorý to dovolí, alebo sám.

Spusti `codex` v repozitári; server zistí repozitár zo svojho pracovného adresára. Ak tvoje nastavenie spúšťa MCP servery inde, zafixuj repozitár cez `args = ["-C", "/path/to/repo", "mcp"]` alebo `env = { KONTEXT_DIR = "/path/to/repo" }`.

## Inštrukcie

Codex číta `AGENTS.md`. Pridaj doň pracovné pravidlá pre kontext:

```sh
kontext connect agents-md --write
```

## Codex ako LLM pre autopilot

```sh
kontext adapters add llm-codex
kontext init --deepen --llm llm-codex --jobs 2 --max 10
```

Preset spúšťa `codex exec --skip-git-repo-check --sandbox read-only --ephemeral` a finálnu správu číta z `--output-last-message`; úloha vrátane relevantných úryvkov zo súborov sa odovzdáva cez stdin.

## Tipy

- Zapisovacie nástroje kontext sa dotýkajú iba pracovného stromu, git indexu a `.git/kontext/`; všetko ostatné je len na čítanie.
- `kontext call ctx_brief '{"focus":["src/api"]}'` ukáže presne to, čo dostane Codex.
