# Claude Code

## Connect

**For the whole team** (recommended): commit a project-scoped `.mcp.json`.

```sh
kontext connect claude --write
```

```json
{
  "mcpServers": {
    "kontext": { "command": "kontext", "args": ["mcp"] }
  }
}
```

Claude Code asks each teammate once whether to trust the project's MCP servers. Teammates need the `kontext` binary on their `PATH`.

**Only for you**, in every project:

```sh
claude mcp add --scope user kontext -- kontext mcp
```

Check with `/mcp` inside Claude Code: the server `kontext` should list eleven `ctx_*` tools.

Registered for every project, kontext stays out of the way where nobody set it up: in a repository without a knowledge store it tells the agent so in its instructions, serves `ctx_brief`, `ctx_search`, `ctx_read` and `ctx_why` over docs and history, refuses team captures and promotions (private notes still work), and bootstraps only when asked (`ctx_init` with `bootstrap=true`, which the `kontext-init` prompt passes).

## Brief at session start

Add a `SessionStart` hook that injects the brief as additional context:

```sh
kontext connect claude-hooks --write
```

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "timeout": 15,
        "command": "command -v kontext >/dev/null 2>&1 && kontext brief --format claude-hook 2>/dev/null || true" } ] }
    ]
  }
}
```

`kontext brief --format claude-hook` prints `{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "…"}}`. The hook is a no-op on machines without kontext.

## Slash commands

| Command | Use |
| --- | --- |
| `/mcp__kontext__kontext-init` | bootstrap and deepen the repository's knowledge (optionally with a number of tasks) |
| `/mcp__kontext__kontext-commit` | prepare the current change for commit |
| `/mcp__kontext__kontext-reflect` | capture what this session decided or learned |

## CLAUDE.md

```sh
kontext connect agents-md --write
```

adds a marked block with the working rules to `AGENTS.md`, or to `CLAUDE.md` when that is the file your repository uses. Re-running updates the block in place.

## Permissions

To stop Claude Code from asking before every kontext call, allow the read-only tools in `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["mcp__kontext__ctx_brief", "mcp__kontext__ctx_search", "mcp__kontext__ctx_read",
              "mcp__kontext__ctx_why", "mcp__kontext__ctx_log", "mcp__kontext__ctx_init"]
  }
}
```

Keep the writing tools (`ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init_submit`) on "ask" if you want to confirm each capture.

## Headless (`claude -p`)

```sh
echo '{"mcpServers":{"kontext":{"command":"kontext","args":["mcp"]}}}' > /tmp/kontext-mcp.json
claude -p --mcp-config /tmp/kontext-mcp.json --allowedTools mcp__kontext__ctx_brief < prompt.txt
```

Claude Code in print mode is also available as an LLM for the autopilot: `kontext adapters add llm-claude`.
