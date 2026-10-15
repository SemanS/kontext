---
id: 2026-10-15-git-is-shared-truth-inbox-history-and-receipts-preserve
kind: decision
title: Git is shared truth; inbox history and receipts preserve local workflow identity
status: accepted
date: 2026-10-15
summary: Shared knowledge remains reviewed Git Markdown; local candidates, dismissal history and operation receipts are durable workflow state, while indexes/caches/snapshots are disposable.
tags: [git, workflow, idempotency]
paths: [src/api_write.rs, src/inbox.rs, src/application.rs, src/lib.rs, src/cli.rs, src/index.rs, src/repo.rs]
supersedes: [2026-09-28-git-markdown-is-the-team-s-source-of-truth-everything-else]
author: SemanS
origin: agent
---

## Context
The application API makes retried writes recoverable across clients, crashes and remote queue failures. Calling all local state derived would permit deleting the receipts that prevent duplicate writes. Candidate inbox state was already not reconstructible from committed knowledge.

## Decision
Git and reviewed Markdown remain the source of shared team truth. Local inbox candidates, dismissal history and operation receipts preserve private in-progress workflow identity; retain them while clients may retry or reconcile a write. Search indexes, cache and snapshots remain derived and rebuildable. The receipt does not grant publication or review authority; verifying services independently prove their Git source and use the same pure KnowledgePolicy parser/config/lifecycle facade as the standalone engine. CLI, MCP and embedded calls share the application service and canonical writes.

## Consequences
Clients can safely retry stable operation IDs without mistaking a cache miss for a new write. Deleting caches never changes worktree ownership or shared knowledge. Explicitly discarding local workflow state discards its retry identity too. No remote service becomes the source of reviewed team knowledge.
