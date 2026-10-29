# Cursor, OpenCode and other MCP clients

## Cursor

```sh
kontext connect cursor --write     # writes .cursor/mcp.json
```

```json
{ "mcpServers": { "kontext": { "command": "kontext", "args": ["mcp"] } } }
```

Cursor also reads `.cursorrules`; kontext's brief lists it under *Rules for agents*. `kontext connect agents-md --write` adds the working rules to `AGENTS.md`.

**Hooks.** `kontext connect cursor --hooks --write` writes `.cursor/hooks.json`: a session-start hook and a hook after a file edit (`postToolUse`), both running `kontext hook --host cursor`. Cursor also reads Claude Code hooks from `.claude/settings.json`; install one set, not both. Opt-in until the A/B report; see [Hooks](../guides/09-hooks.md).

**Skills.** Cursor reads `SKILL.md` files from `.claude/skills/`, where `kontext connect skills --write` puts them.

## OpenCode

```sh
kontext connect opencode --write   # writes opencode.json
```

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": { "kontext": { "type": "local", "command": ["kontext", "mcp"], "enabled": true } }
}
```

OpenCode reads `AGENTS.md` and the skills in `.claude/skills/`. It has no hook protocol kontext writes to; the agent gets the brief by calling `ctx_brief`, as the `AGENTS.md` block tells it.

## Superset and other orchestrators

Orchestrators that run many agents in parallel worktrees need nothing special: each agent's harness starts its own `kontext mcp` in its worktree. All worktrees of a clone share the inbox, the outbox and bootstrap progress (they live in the git common directory); each worktree keeps its own search index. State of worktrees that no longer exist is pruned.

Two things to know with orchestrated agents:

- **Fresh clones have no hooks.** Run `kontext hooks install` once in a project the orchestrator cloned (worktrees share the hooks). Until then the brief warns, and `ctx_prepare_commit` says the trailers have to be added by hand.
- **Agents leave the session's directory** for other worktrees, submodules and sibling projects. See [Several repositories](../concepts/09-several-repositories.md).

A useful division of labour:

- one agent runs `/mcp__kontext__kontext-init` to deepen the bootstrap while others work,
- every agent calls `ctx_brief` with its focus paths,
- captures from all agents meet in the shared inbox, and each agent promotes the ones belonging to its own change with `ctx_prepare_commit`.

## Any other MCP client

Configure a stdio server with the command `kontext` and the argument `mcp`, started in the repository directory. If the client starts servers elsewhere, pass the repository explicitly:

```json
{ "command": "kontext", "args": ["-C", "/path/to/repo", "mcp"] }
```

or set `KONTEXT_DIR=/path/to/repo` in the server's environment.

## Clients without MCP

Use the CLI from any harness that can run shell commands:

```sh
kontext brief --focus src/api
kontext search "retry policy" --json
kontext call ctx_prepare_commit '{}'
```
