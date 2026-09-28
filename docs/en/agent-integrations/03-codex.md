# Codex

## Connect

Codex reads MCP servers from `~/.codex/config.toml`:

```toml
[mcp_servers.kontext]
command = "kontext"
args = ["mcp"]
```

```sh
kontext connect codex            # prints the snippet
kontext connect codex --write    # appends it (a timestamped backup of the file is kept)
```

Run `codex` inside the repository; the server discovers the repository from its working directory. If your setup starts MCP servers elsewhere, pin the repository with `args = ["-C", "/path/to/repo", "mcp"]` or `env = { KONTEXT_DIR = "/path/to/repo" }`.

## Instructions

Codex reads `AGENTS.md`. Add kontext's working rules to it:

```sh
kontext connect agents-md --write
```

## Codex as the autopilot LLM

```sh
kontext adapters add llm-codex
kontext init --deepen --llm llm-codex --jobs 2 --max 10
```

The preset runs `codex exec --skip-git-repo-check --sandbox read-only --ephemeral` and reads the final message from `--output-last-message`; the task, including the relevant file excerpts, is passed on stdin.

## Tips

- kontext's writing tools touch only the working tree, the git index and `.git/kontext/`; everything else is read-only.
- `kontext call ctx_brief '{"focus":["src/api"]}'` shows exactly what Codex receives.
