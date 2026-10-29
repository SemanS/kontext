---
id: 2026-10-29-ctx-verify-is-a-destructive-open-world-tool
kind: decision
title: ctx_verify is a destructive, open-world tool
status: accepted
date: 2026-10-29
summary: "ctx_verify declares destructiveHint:true and openWorldHint:true and takes dir only inside this repository, its worktrees and submodules; under Codex approval_policy=never it is refused and the agent runs the command itself. Read tools that call adapters keep their decided annotations."
tags: [mcp, annotations, verify, security]
paths: [src/tools.rs, src/connect.rs]
extends: [2026-10-15-mcp-annotations-distinguish-the-application-apis-external]
author: SemanS
origin: agent
---

## Decision

- `ctx_verify` declares `readOnlyHint:false`, `destructiveHint:true`, `openWorldHint:true`. This replaces, for this one tool, the clause of 2026-10-15-mcp-annotations-distinguish-the-application-apis-external that local writes are non-destructive: a verify runs an arbitrary command, not a local knowledge write.
- Its `dir` is limited to this repository, its worktrees and its submodules.
- Every other tool keeps the annotations already decided. Read tools that call adapters (`ctx_brief`, `ctx_search`, `ctx_why`, `ctx_read`) keep theirs (F69) until a separate decision changes them.

## Consequences

- Clients that auto-approve non-destructive tools ask before `ctx_verify`.
- Codex under `approval_policy=never` (Superset sessions) refuses the tool. The agent then runs the command itself in its own sandbox and records the result as an `agent` stamp, which matches 2026-10-29-a-verify-command-is-code-from-the-repository-and-runs-only.
- `kontext connect codex` no longer leaves `ctx_verify` under a blanket approve.
