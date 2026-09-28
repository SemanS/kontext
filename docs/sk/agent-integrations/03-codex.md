# Codex

## Pripojenie

Codex načítava MCP servery z `~/.codex/config.toml`:

```toml
[mcp_servers.kontext]
command = "kontext"
args = ["mcp"]
```

```sh
kontext connect codex            # prints the snippet
kontext connect codex --write    # appends it (a timestamped backup of the file is kept)
```

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
