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

Check with `/mcp` inside Claude Code: the server `kontext` should list sixteen `ctx_*` tools. Claude Code defers MCP tools behind tool search and loads only names and server instructions; `ctx_brief` carries `alwaysLoad`, so it is loaded from the first turn.

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

## Hooks {#hooks}

The SessionStart hook above sends one unfocused brief. For governing knowledge at the moment a file is edited, install the full set (opt-in until the A/B report; see [Hooks](../guides/09-hooks.md)):

```sh
kontext connect claude --hooks --write
```

It writes into `.claude/settings.json`:

| Event | Matcher | What the agent gets |
| --- | --- | --- |
| `SessionStart` | `startup`, `resume`, `clear`, `compact` | the brief, again after a compaction |
| `SubagentStart` | | the brief for the subagent |
| `PreToolUse` | `Edit\|Write` | the entries governing `tool_input.file_path` not yet told in this session |

Every hook runs `kontext hook --host claude`, which reads the hook JSON on stdin and answers with `additionalContext` (at most 10 000 characters). Hooks share one `session_budget_tokens` budget with `ctx_brief`, never block a tool, never call a model, and exit 0 with no output on any error. `KONTEXT_HOOKS=off` or `[hooks] enabled = false` turns them off.

`kontext connect` also adds a deny rule for `kontext trust` run through Bash, so the agent cannot pin commands or adapters for you (see [Security](../concepts/08-security.md#verify)).

## Slash commands

| Command | Use |
| --- | --- |
| `/mcp__kontext__kontext-init` | bootstrap and deepen the repository's knowledge (optionally with a number of tasks) |
| `/mcp__kontext__kontext-commit` | prepare the current change for commit |
| `/mcp__kontext__kontext-reflect` | capture what this session decided or learned |

## Skills {#skills}

Four recipes for knowledge maintenance ship with the binary as `SKILL.md` files. Install them into the project (commit them, so the team's agents share them):

```sh
kontext connect skills --write        # .claude/skills/ and .agents/skills/kontext-*/SKILL.md
```

| Skill | Use |
| --- | --- |
| `kontext-audit` | look at the entries most likely stale within a budget; confirm, challenge or supersede each, never delete |
| `kontext-flows` | reorganise the store into [flows](../concepts/02-knowledge-store.md#flows): curated trails through the entries, one jj change or commit per flow |
| `kontext-verify` | give entries a `verify` command that proves the claim, so `kontext verify` re-runs it |
| `kontext-merge` | find entries that say the same thing or disagree (`entries.related`) and draft one superseding entry per group |

Each one ends in the local inbox or staged files for a person to review and commit; nothing in `.ai/` changes before that. Each file carries a hash marker of the shipped text: re-running `connect skills --write` after an upgrade updates files nobody edited and leaves files your team changed, with a note. The same files work in any client that reads the `SKILL.md` layout (`.agents/skills` for Codex, `.claude/skills` for Cursor and OpenCode).

When a verify command is not pinned, `kontext-audit` and `kontext-verify` show the command and stop; they never run `kontext trust`.

## Plugin

The repository ships a Claude Code plugin built from the same sources as `connect`: `.claude-plugin/plugin.json` with the MCP server, the hooks and the skills. Install it from the plugin marketplace instead of running the `connect` commands one by one; the hooks in the plugin follow the same opt-in and kill switch.

## CLAUDE.md

```sh
kontext connect agents-md --write
```

adds a marked block with the working rules to `AGENTS.md`. Claude Code reads `AGENTS.md` natively; when the repository also has a `CLAUDE.md`, kontext adds an `@AGENTS.md` import to it instead of a second copy. Re-running updates the block in place.

## Permissions

To stop Claude Code from asking before every kontext call, allow the read-only tools in `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["mcp__kontext__ctx_brief", "mcp__kontext__ctx_search", "mcp__kontext__ctx_read",
              "mcp__kontext__ctx_why", "mcp__kontext__ctx_log", "mcp__kontext__ctx_drift",
              "mcp__kontext__ctx_threads"]
  }
}
```

| Tool | Suggested | Why |
| --- | --- | --- |
| `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads` | allow | read only |
| `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init_submit` | ask, or allow if you review the diff anyway | write the inbox, the working tree or the index |
| `ctx_init` | ask | with `bootstrap=true` it writes the store skeleton |
| `ctx_confirm`, `ctx_challenge` | ask | write a ledger line or a challenge for review |
| `ctx_verify` | ask | runs a pinned shell command from the repository (`destructiveHint: true`) |
| `ctx_api` | ask | reaches every application operation, including writes |

## Headless (`claude -p`)

```sh
echo '{"mcpServers":{"kontext":{"command":"kontext","args":["mcp"]}}}' > /tmp/kontext-mcp.json
claude -p --mcp-config /tmp/kontext-mcp.json --allowedTools mcp__kontext__ctx_brief < prompt.txt
```

Claude Code in print mode is also available as an LLM for the autopilot: `kontext adapters add llm-claude`.
