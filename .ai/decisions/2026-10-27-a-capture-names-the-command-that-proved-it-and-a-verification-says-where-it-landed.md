---
id: 2026-10-27-a-capture-names-the-command-that-proved-it-and-a-verification-says-where-it-landed
kind: decision
title: A capture names the command that proved it, an inbox draft can take one later, and a verification says which commit landed it
status: accepted
date: 2026-10-27
summary: ctx_capture and kontext capture take verify; ctx_inbox can set verify on a draft that was captured without one; verification carries landed, the commit in HEAD's history that first added the newest ledger line, so a consumer can tell a staged check from a committed one without parsing the ledger.
tags: [capture, inbox, verify, ledger, api]
paths: [src/tools.rs, src/cli.rs, src/api_write.rs, src/inbox.rs, src/verification.rs, src/ledger.rs, docs/en/concepts/03-capture-and-review.md, docs/sk/concepts/03-capture-and-review.md]
extends: [2026-10-27-a-verification-is-a-commit-in-a-ledger-and-the-brief-says-what-changed-since]
author: SemanS
origin: agent
---

## Context

The ledger decision gave entries a `verify` command and the machine API `verification` and `since`, but nothing fills `verify` on the way in: an agent that records a learning has usually just run the test that proves it, and the field stays empty unless a person edits frontmatter. A consumer that wants to resolve a question about an entry once its check passed and the pass is committed (Peer's drift challenges) can see `verification.commit`, the commit the check looked at, but not whether the ledger line itself is in a reviewed revision or only staged. Parsing the ledger in a second place would break the one-parser rule.

## Decision

- **Capture takes the proof.** `ctx_capture` and `kontext capture --verify "<command>"` accept `verify`, validated like the entry field (one line, at most 500 characters). The tool's description and the server's capture guidance say: a learning or decision you proved with a command carries that command as `verify`.
- **A draft can take it later.** `ctx_inbox` (and the machine operation `inbox.verify`) sets or replaces `verify` on an inbox draft by id; promotion keeps it. This is how a nudge after a capture without a check is answered without re-capturing.
- **A verification says where it landed.** `verification` gains `landed`: the full id of the commit in HEAD's history that first added the newest ledger line for the entry, or null while the line is only staged or in the working tree. Frontmatter-only verifications report the last commit that changed the entry's file. `since` already counts from the later of `commit` and `landed`.
- Nothing else changes: API version 1, kinds and the ledger format as decided.

## Consequences

An agent's `ctx_capture` carries the test it ran; a draft captured without one can be completed from the inbox; Peer can tell "this check passed and the pass is committed" from `verification` alone and resolve a drift challenge on it, and can show "passed, not yet committed" while it is staged. No second ledger parser exists.

## Alternatives

- **Infer `verify` from the commit that promotes the draft** (tests it touched): rejected, a touched test file is not the command that proves the entry.
- **Expose the ledger as an API operation for consumers to parse**: rejected, `landed` answers the one question consumers have and keeps parsing in kontext.
