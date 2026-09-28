---
id: 2026-09-28-reuse-existing-adr-directories-in-their-own-style
kind: decision
title: Reuse existing ADR directories in their own style
status: accepted
date: 2026-09-28
paths: [src/store.rs, src/init/scan.rs]
author: SemanS
origin: cli
---

## Context
Many teams already keep ADRs (`docs/adr/NNNN-*.md` with **Status:** fields). A parallel .ai/decisions would split the history.

## Decision
Init detects ADR directories, their style (front matter vs fields) and numbering (sequential, date, none); kontext reads and writes decisions there in the same format.

## Consequences
The decision log stays where the team already looks; sequential numbers can collide between parallel branches (as with any ADR practice).
