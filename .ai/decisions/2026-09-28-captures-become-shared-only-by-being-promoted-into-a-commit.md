---
id: 2026-09-28-captures-become-shared-only-by-being-promoted-into-a-commit
kind: decision
title: Captures become shared only by being promoted into a commit
status: accepted
date: 2026-09-28
summary: Agents write candidates to a local inbox; promotion writes them into the store and stages them with the change, so review happens in the PR.
paths: [src/inbox.rs, src/ops.rs, src/hooks.rs]
commits: [8c35950]
origin: init
---

## Context

Letting agents write shared memory directly means unreviewed claims become everyone's truth.

## Decision

`ctx_capture` stores candidates in `<git-common-dir>/kontext/inbox` (shared by worktrees, never committed). `ctx_prepare_commit`/`kontext promote` move the relevant ones into the store and stage them; private captures never leave the clone except to personal adapters.

## Consequences

Every shared entry has a reviewer and a commit (trailer `Decision: <id>`). Candidates that nobody promotes stay local and can be dropped.

