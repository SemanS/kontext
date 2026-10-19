---
id: 2026-10-19-challenges-resolve-through-retire-or-supersede-never-in
kind: decision
title: Challenges resolve through retire or supersede, never in place
status: accepted
date: 2026-10-19
summary: "A challenge is a local inbox note against an entry and its revision; it is resolved only by a reviewed retirement (status deprecated with a ## Retired note) or by a replacement that supersedes the entry."
tags: [knowledge-maintenance]
paths: [src/ops.rs, src/api_write.rs, src/store.rs, src/tools.rs]
author: SemanS
origin: agent
---

## Context

Agents and people find recorded knowledge that no longer holds. Before 0.3 they had nowhere to say so: ctx_prepare_commit asked them to confirm a decision still holds, with no way to answer.

## Decision

`ctx_challenge <id> "why"` (and `kontext challenge`) writes an inbox candidate of kind `challenge` with `against: <id>`, `against_revision` (the commit it was read at) and `against_blob`. It changes no knowledge and cannot be promoted. It is resolved through review only: `ctx_prepare_commit retire=[id]` / `drafts.retire` stage the entry's own file with `status: deprecated` and a `## Retired` note (refusing a stale revision or a dirty file), or a new entry with `supersedes: [id]` replaces it. Either resolution takes the challenge out of the inbox with the change. Both work for every kind.

## Consequences

The old claim and the reason it was dropped stay readable in Git; nothing is edited in place. The brief marks a challenged entry `(challenged locally)` until the change is committed. Shared challenges with independent counting live in Peer Hub, not here.
