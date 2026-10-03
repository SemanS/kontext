# Cursor, OpenCode and other MCP clients

## Cursor

```sh
kontext connect cursor --write     # writes .cursor/mcp.json
```

```json
{ "mcpServers": { "kontext": { "command": "kontext", "args": ["mcp"] } } }
```

Cursor also reads `.cursorrules`; kontext's brief lists it under *Rules for agents*. `kontext connect agents-md --write` adds the working rules to `AGENTS.md`.

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

## Superset and other orchestrators

Orchestrators that run many agents in parallel worktrees need nothing special: each agent's harness starts its own `kontext mcp` in its worktree. All worktrees of a clone share the inbox, the outbox and bootstrap progress (they live in the git common directory); each worktree keeps its own search index. State of worktrees that no longer exist is pruned.

Things that come with orchestrated agents:

- **Fresh clones have no hooks.** A project an orchestrator clones (a Superset project, a CI checkout) runs none of kontext's commit-time checks until `kontext hooks install` runs once in it (worktrees share the hooks). The brief warns, and `ctx_prepare_commit` says when the trailers have to be added by hand.
- **Work outside the session's directory.** An agent often works in another worktree, a submodule or a sibling project. kontext answers from the repository that owns the paths it is given (focus paths, `ctx_why` targets, capture paths), and every tool takes `dir` to name another one. A brief in a branch without the team knowledge names a worktree that has it.

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
