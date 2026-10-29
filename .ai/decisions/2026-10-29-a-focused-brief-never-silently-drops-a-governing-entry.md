---
id: 2026-10-29-a-focused-brief-never-silently-drops-a-governing-entry
kind: decision
title: A focused brief never silently drops a governing entry
status: accepted
date: 2026-10-29
summary: "A reserved 'Governs <focus> (N)' section comes first, one compact line per entry ordered by path specificity then age; overflow is named on one always-printed 'also governing' line; every trimmed kind ends with '… N more (K govern your focus)'; focus items that look like paths are paths before the file exists; stamps are never written as verified."
tags: [brief, focus, budget]
paths: [src/ops.rs, src/glob.rs, src/route.rs]
extends: [2026-10-29-verified-means-read-by-a-person-or-a-passed-verify-stamps]
author: SemanS
origin: agent
---

## Context

A brief focused on `src/ops.rs` showed 9 of 10 governing decisions at 1 400 tokens and 2 of 10 at 600, while keeping unrelated entries; "… 17 more" did not say a governing entry was among them, and twelve governing conventions vanished without trace (F15, F90). Peer sends `max(200, 1500 − spent)`, so 200 to 1 000 tokens is the normal case.

This extends the brief's derivation currency, decided in 2026-10-27-a-verification-is-a-commit-in-a-ledger-and-the-brief-says-what-changed-since and retained by its successor.

## Decision

- **A reserved section first.** "Governs <focus> (N)" precedes everything else: one compact line per entry (full id, title, markers, "· N commits since"), ordered by path specificity (file > directory > glob > `**`), then older first. No ordering by English words such as never, only or must.
- **Overflow is named.** What does not fit goes on one always-printed line "also governing: [a] [b]". Below the size of the section the floor is ids only.
- **Other kinds are counted.** Every trimmed kind ends with "… N more (K govern your focus)". The footer is inside the budget; superseded module docs are left out.
- **Honest currency.** The currency line says "stamped by agent", "bulk stamp" or "verify FAILED"; "verified" only where `verification.verified` holds.
- **Paths before files.** A focus item with `/`, a glob character or a known extension is a path even when the file does not exist yet. `path_overlaps` works in both directions, including early wildcards and a leading slash. `[slug]` is matched literally first, then as a glob (as git does), never depending on whether a file exists.

## Consequences

An agent always sees which entries govern what it is about to change, at any budget. Verification is the eval scenario "every governing entry survives a focused brief" at 200, 400, 600, 1 000 and 1 400 tokens, written by a different session than the implementation.
