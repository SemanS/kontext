---
id: 2026-10-29-a-challenge-stays-resolving-until-its-resolution-lands-in
kind: decision
title: A challenge stays resolving until its resolution lands in HEAD
status: accepted
date: 2026-10-29
summary: "Staging a retirement or a superseding entry marks a challenge resolving, not resolved; the challenge leaves the inbox only when a new blob of the challenged entry (or its successor) lands in HEAD; cleanup is tied to the blob change, not to the existence of a file or to any later commit."
tags: [challenge, inbox, knowledge-maintenance]
paths: [src/ops.rs, src/api_write.rs, src/inbox.rs]
extends: [2026-10-19-challenges-resolve-through-retire-or-supersede-never-in]
author: SemanS
origin: agent
---

## Context

`resolve_challenges` removed a challenge when its resolution was staged, and `cleanup_committed` deleted it after any commit, even when the retirement had been reset. The challenge then disappeared with nothing resolving it.

## Decision

- Staging a retirement or a superseding entry moves the challenge to `resolving`; it stays in the inbox.
- The challenge is resolved and removed only when HEAD contains a new blob of the challenged entry (its retirement) or the successor that supersedes it.
- Cleanup compares blobs, not file existence. A reset, or a commit without the change, brings the challenge back to open.

## Consequences

A challenge cannot be lost by an aborted commit. The brief keeps marking the entry until the change is really committed.
