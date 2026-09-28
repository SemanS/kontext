---
id: 2026-09-28-git-markdown-is-the-team-s-source-of-truth-everything-else
kind: decision
title: Git + Markdown is the team's source of truth; everything else is derived
status: accepted
date: 2026-09-28
summary: Shared knowledge is short Markdown in the repository; the index, caches, snapshots and outbox under .git/kontext can be deleted anytime.
paths: [src/store.rs, src/index.rs, .ai/**]
commits: [8c35950]
origin: init
---

## Context

Agents need a shared memory that teammates can trust, diff and review. Vector stores and session archives are good at recall but are not reviewable and drift per machine.

## Decision

Team truth lives only in reviewed files (`.ai/` or an existing ADR directory). The Tantivy index, init caches, sync snapshots and the event outbox are derived from files and git and are rebuilt on demand.

## Consequences

No database to migrate or back up; knowledge travels with branches and PRs. Semantic recall must come from adapters, and the local index is rebuilt per worktree.

