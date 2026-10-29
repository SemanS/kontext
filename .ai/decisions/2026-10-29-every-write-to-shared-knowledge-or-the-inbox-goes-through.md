---
id: 2026-10-29-every-write-to-shared-knowledge-or-the-inbox-goes-through
kind: decision
title: Every write to shared knowledge or the inbox goes through the application service
status: accepted
date: 2026-10-29
summary: "ops::promote, ctx_prepare_commit promote and drop, ctx_inbox drop and capture --promote use the application service (api_write::stage_candidate, inbox.drop) with its ownership checks, locks, dismissal history and guards; there is no second write path."
tags: [api, inbox, promote, write-path]
paths: [src/api_write.rs, src/ops.rs, src/inbox.rs, src/tools.rs, src/cli.rs]
extends: [2026-10-15-git-is-shared-truth-inbox-history-and-receipts-preserve]
author: SemanS
origin: agent
---

## Context

Promotion through `ctx_prepare_commit` and `kontext promote` was a second write path with different semantics and leaked the `api_operation` marker into Git (F27). `ctx_inbox drop` and `prepare_commit drop` hard-deleted drafts without locks, the ownership check or dismissal history (F13).

## Decision

- `ops::promote` and `ctx_prepare_commit` promote through `api_write::stage_candidate`: the `api_operation` marker is stripped, uncommitted edits of superseded files are guarded, an unknown `supersedes` target is refused.
- `ctx_inbox drop` and `prepare_commit drop` go through `inbox.drop`: ownership, locks and `.dismissed`; an id is exact or a unique prefix of the caller's own candidates.
- `capture --promote` resolves challenges like `promote()`. `prepare_commit` validates every id before its first side effect. New inbox ids avoid `.promoted/` and `.dismissed/`. `retire_challenged` takes `write_guard` before its check. `amends` is routed by the parent's repository.

## Consequences

CLI, MCP and application clients get the same semantics, receipts and guards for every write. A fix to the write path is made once.
