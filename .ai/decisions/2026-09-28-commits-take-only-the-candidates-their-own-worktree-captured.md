---
id: 2026-09-28-commits-take-only-the-candidates-their-own-worktree-captured
kind: decision
title: Commits take only the candidates their own worktree captured
status: accepted
date: 2026-09-28
tags: [inbox, worktrees]
paths: [src/inbox.rs, src/ops.rs, src/hooks.rs]
author: SemanS
origin: cli
---

## Context

The inbox lives in the git common dir, so all worktrees of a clone share it. With parallel agents in separate worktrees, ctx_prepare_commit in one worktree offered another session's candidates as related to the change, and an agent following "promote the ones that belong to this change" would ship them in the wrong commit.

## Decision

Candidates record the worktree and branch they were captured in. ctx_prepare_commit, the pre-commit reminder and the brief offer only the current worktree's own candidates (and entries that do not say); another live worktree's are listed apart as not for this commit; a removed worktree's may be adopted by anyone.

## Consequences

The inbox stays one place (kontext inbox lists everything with its origin). Promoting a foreign candidate remains possible when asked for by id.
