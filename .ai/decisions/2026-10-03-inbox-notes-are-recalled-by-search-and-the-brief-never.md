---
id: 2026-10-03-inbox-notes-are-recalled-by-search-and-the-brief-never
kind: decision
title: Inbox notes are recalled by search and the brief, never promoted across worktrees
status: accepted
date: 2026-10-03
summary: ctx_search scans the inbox and the brief lists the notes matching its focus; a commit still takes only its own worktree's candidates.
paths: [src/ops.rs, src/inbox.rs]
author: SemanS
origin: cli
---

## Context

Private captures in a repository without a store were written and never found again: search covered the store, the docs and the history, and the brief only counted inbox items. On dog-ai, an incident about an unbounded livestream download stayed invisible even to a search for its exact title.

## Decision

`ctx_search` scans the inbox (team candidates and private notes, from every worktree of the clone) as part of the local source, and the brief lists the notes that match its focus under "Local notes". Promotion does not change: a commit takes only the candidates its own worktree captured.

## Consequences

Every agent working in the repository on this machine sees the notes, labelled as local and unreviewed. Nothing leaves the clone. The inbox is a handful of files, so it is scanned rather than indexed.
