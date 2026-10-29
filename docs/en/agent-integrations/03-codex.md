# Codex

## Connect

Codex reads MCP servers from `~/.codex/config.toml` (`$CODEX_HOME/config.toml` when `CODEX_HOME` is set, as account-switching wrappers do):

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

# … one table per kontext tool: approve for the read and inbox tools …

[mcp_servers.kontext.tools.ctx_verify]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_api]
approval_mode = "prompt"

[mcp_servers.kontext.tools.ctx_init]
approval_mode = "prompt"
```

```sh
kontext connect codex            # prints the snippet
kontext connect codex --write    # appends it, or adds the missing tool tables to an existing entry (a timestamped backup is kept)
```

The server default is `prompt`, so a tool without its own table, such as one an adapter exposes through kontext, asks. An existing entry keeps its `default_tools_approval_mode` and the tool tables you changed; when it says `approve`, `connect codex` warns that exposed tools, and a Codex without per-tool `approval_mode`, would then run `ctx_verify`, `ctx_api` and `ctx_init` without asking.

## Approvals

Codex asks before calling an MCP tool unless it can tell the tool is harmless, and with `approval_policy = "never"` it refuses instead of asking: `MCP tool call requires approval, but approval policy is never`.

| Tool | Annotations | Approval written by `connect codex` |
| --- | --- | --- |
| `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads` | `readOnlyHint: true` | approve |
| `ctx_capture`, `ctx_inbox`, `ctx_challenge`, `ctx_confirm`, `ctx_prepare_commit`, `ctx_init_submit` | non-destructive writes to the inbox, the working tree and the index | approve |
| `ctx_init` | writes the store skeleton with `bootstrap=true` | prompt |
| `ctx_verify` | `destructiveHint: true`, `openWorldHint: true`; runs a pinned shell command | prompt |
| `ctx_api` | `openWorldHint: true`; every application operation | prompt |
| tools exposed from adapters | upstream annotations, or `readOnlyHint: false`, `openWorldHint: true` | prompt |

Under `approval_policy = "never"` (for example in Superset sessions) Codex refuses `ctx_verify`. That is intended: the agent runs the entry's command in its own sandbox instead. `ctx_verify` runs inside the MCP server, outside the agent's sandbox, and an unpinned command is returned to the agent, not run (see [Security](../concepts/08-security.md#verify)).

With `sandbox_mode = "workspace-write"` Codex keeps `.git` read-only for the agent's own shell commands, so `git add` / `git commit` fail there; kontext's server is not affected (`ctx_prepare_commit` still promotes and stages). Commit from a sandbox that allows it, or yourself.

Run `codex` inside the repository; the server discovers the repository from its working directory. If your setup starts MCP servers elsewhere, pin the repository with `args = ["-C", "/path/to/repo", "mcp"]` or `env = { KONTEXT_DIR = "/path/to/repo" }`.

## Instructions

Codex reads `AGENTS.md`. Add kontext's working rules to it:

```sh
kontext connect agents-md --write
```

Do this in every repository where Codex should use kontext. Codex follows `AGENTS.md` and reads only the first 512 characters of the MCP server's instructions, which kontext keeps self-contained; without the block it can still finish a whole task without one kontext call.

## Hooks

Codex runs hooks from `.codex/hooks.json`, and only with `[features] codex_hooks = true` in its `config.toml` (`connect` prints this and does not write it). `kontext connect codex --hooks --write` installs `SessionStart` (matcher `startup|resume|clear|compact`) and `PreToolUse` with matcher `apply_patch|Edit|Write`, both running `kontext hook --host codex`; an answer stays within about 2 500 tokens and the session budget. Any `--write` also adds a `PreToolUse` hook on `Bash` that denies `kontext trust` run through the shell. Opt-in until the A/B report; see [Hooks](../guides/09-hooks.md).

## Skills

`kontext connect skills --write` also writes the skills to `.agents/skills/`, where Codex looks for them.

## Codex as the autopilot LLM

```sh
kontext adapters add llm-codex
kontext init --deepen --llm llm-codex --jobs 2 --max 10
```

The preset runs `codex exec --skip-git-repo-check --sandbox read-only --ephemeral` and reads the final message from `--output-last-message`; the task, including the relevant file excerpts, is passed on stdin.

## Tips

- kontext's writing tools touch only the working tree, the git index and `.git/kontext/`. `ctx_verify` is the exception: it runs a pinned command.
- `kontext call ctx_brief '{"focus":["src/api"]}'` shows exactly what Codex receives.
