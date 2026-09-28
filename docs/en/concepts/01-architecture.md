# Architecture

```text
                 Claude Code · Codex · Cursor · OpenCode · any MCP client
                                         │  MCP (stdio, JSON-RPC)
                                ┌────────┴────────┐
                                │     kontext     │  tools · prompts · resources
                                │  (one binary)   │  CLI · git hooks
                                └──┬─────┬─────┬──┘
             adapters (config) ────┘     │     └──── git hooks
   ┌──────────────┬──────────────┐       │       pre-commit        validate · secret-scan · index
   │ mcp driver   │ http driver  │       │       prepare-commit-msg  Decision: trailers
   │ command drv  │              │       │       post-commit/merge/rewrite  sync events
   └──────┬───────┴──────┬───────┘       ▼
          │              │         TEAM TRUTH (in the repository)   LOCAL, DERIVED, PRIVATE
   CodeGraph, Serena,  OpenViking,  .ai/decisions/*.md   ◄─ PR ──  .git/kontext/inbox/      candidates
   Agent LCM, sessions Ollama, …    .ai/conventions/, learnings/   .git/kontext/worktrees/… search index
   claude -p, codex    Slack        .ai/architecture/…             .git/kontext/outbox.jsonl adapter deliveries
```

## Components

| Component | Source | Responsibility |
| --- | --- | --- |
| CLI | `src/main.rs` | every command; `kontext mcp` starts the server |
| MCP server | `src/mcp.rs`, `src/tools.rs` | JSON-RPC over stdio, concurrent requests, tool/prompt/resource surface |
| Operations | `src/ops.rs` | brief, search, read, why, log, capture, promote, prepare-commit, check |
| Store | `src/store.rs` | entries in the repository: parse, validate, allocate ids, render the index |
| Inbox | `src/inbox.rs` | local candidates in the git common dir |
| Index | `src/index.rs` | Tantivy BM25 over entries, docs and commits; incremental refresh |
| Adapters | `src/adapters/` | registry, drivers (MCP client, HTTP, command), result mapping, events, federated tools |
| Events | `src/events.rs` | outbox with retries; background flush |
| Hooks | `src/hooks.rs` | install/uninstall; pre-commit, prepare-commit-msg, post-* handlers |
| Init | `src/init/` | scan, history mining, rendering, the deepen task protocol |
| Config | `src/config.rs` | layered TOML, trust of repository-declared adapters |

## Where state lives

| What | Where | Committed | Shared by worktrees |
| --- | --- | --- | --- |
| Knowledge entries, index, shared config | `.ai/` (or your ADR directory) | yes | via git |
| Personal adapters | `~/.config/kontext/config.toml` | no | — |
| Per-repository personal overrides | `~/.config/kontext/repos/<slug>.toml` | no | — |
| Clone-local config | `<git-common-dir>/kontext/config.toml` | no | yes |
| Inbox (candidates) | `<git-common-dir>/kontext/inbox/` | no | yes |
| Outbox (adapter deliveries) | `<git-common-dir>/kontext/outbox.jsonl` | no | yes |
| Bootstrap progress | `<git-common-dir>/kontext/init-done.json` + the entries themselves | partly (entries) | yes |
| Search index, init cache, sync snapshot | `<git-common-dir>/kontext/worktrees/<worktree>/` | no | no (per worktree) |
| Adapter logs (MCP servers' stderr) | `<git-common-dir>/kontext/logs/` | no | yes |

`<git-common-dir>` is `$(git rev-parse --git-common-dir)`, normally `.git`. Everything under `.git/kontext/` is derived or local and can be deleted.

## Repository identity

kontext derives a stable identity from the `origin` remote: `git@github.com:Acme/Shop.git` and `https://github.com/acme/shop` both become `github.com/acme/shop` (slug `github.com-acme-shop`). Every worktree and clone of the same remote gets the same identity, which is what per-repository user config and adapter variables key on. Without a remote the identity is `local/<dir>-<hash>`.

## Request flow: `ctx_search`

1. The local index is refreshed if anything changed (entries, docs, new commits) — at most once per second per process.
2. Tantivy runs the query (BM25 over title, body and identifier-expanded fields).
3. In parallel, every adapter with a `search` op receives the query (12 s deadline; a failing adapter cools down for 30 s).
4. Scores are normalized per source, weighted, merged and de-duplicated by URI and by near-identical text.
5. The result is rendered as compact lines within the token budget, each with a URI for `ctx_read`.

## Request flow: a commit

1. `pre-commit` validates the staged knowledge files, scans them for secrets (blocking on errors), re-renders `.ai/README.md` from the staged tree and stages it, and reminds you of inbox candidates that touch the staged paths.
2. `prepare-commit-msg` adds `Decision: <id>` (and `Convention:`, `Learning:`, `Incident:`) trailers for staged entries.
3. `post-commit` compares knowledge at `HEAD` with the last synced snapshot, queues `sync` events and starts a detached `kontext outbox flush`.

## Design decisions

The project records its own decisions with kontext — see [`.ai/decisions/`](https://github.com/SemanS/kontext/tree/main/.ai/decisions).
