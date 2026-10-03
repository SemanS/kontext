# Codex

## Connect

Codex reads MCP servers from `~/.codex/config.toml` (`$CODEX_HOME/config.toml` when `CODEX_HOME` is set, as account-switching wrappers do):

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

## Approvals

Codex asks before calling an MCP tool unless it can tell the tool is harmless, and with `approval_policy = "never"` it refuses instead of asking: `MCP tool call requires approval, but approval policy is never`. Two things keep kontext working in that setup:

- every kontext tool carries MCP annotations: `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why` and `ctx_log` are `readOnlyHint: true`, the rest are marked non-destructive (they write only the local inbox, the working tree and the git index), which Codex runs without asking;
- `default_tools_approval_mode = "approve"` in the server entry approves kontext's tools explicitly, independent of how a Codex version reads the hints.

With `sandbox_mode = "workspace-write"` Codex keeps `.git` read-only for the agent's own shell commands, so `git add` / `git commit` fail there; kontext's server is not affected (`ctx_prepare_commit` still promotes and stages). Commit from a sandbox that allows it, or yourself.

Run `codex` inside the repository; the server discovers the repository from its working directory. If your setup starts MCP servers elsewhere, pin the repository with `args = ["-C", "/path/to/repo", "mcp"]` or `env = { KONTEXT_DIR = "/path/to/repo" }`.

## Instructions

Codex reads `AGENTS.md`. Add kontext's working rules to it:

```sh
kontext connect agents-md --write
```

Do this in every repository where Codex should use kontext. Codex follows `AGENTS.md`, not the MCP server's instructions: without the block it can finish a whole task without one kontext call.

## Codex as the autopilot LLM

```sh
kontext adapters add llm-codex
kontext init --deepen --llm llm-codex --jobs 2 --max 10
```

The preset runs `codex exec --skip-git-repo-check --sandbox read-only --ephemeral` and reads the final message from `--output-last-message`; the task, including the relevant file excerpts, is passed on stdin.

## Tips

- kontext's writing tools touch only the working tree, the git index and `.git/kontext/`; everything else is read-only.
- `kontext call ctx_brief '{"focus":["src/api"]}'` shows exactly what Codex receives.
