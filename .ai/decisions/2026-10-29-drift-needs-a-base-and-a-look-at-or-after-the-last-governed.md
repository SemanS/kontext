---
id: 2026-10-29-drift-needs-a-base-and-a-look-at-or-after-the-last-governed
kind: decision
title: Drift needs a base and a look at or after the last governed change
status: accepted
date: 2026-10-29
summary: "entries.drift lists an active entry with paths when a non-knowledge file it governs changed in base..head and there is no look (edit, retirement, read or passed verify) at or after the last such change; without a base drift says why and --fail exits non-zero; capability drift.ordered."
tags: [drift, ci, verify]
paths: [src/drift.rs, src/api_read.rs, src/cli.rs, src/tools.rs]
supersedes: [2026-10-20-drift-is-the-knowledge-a-range-changed-the-code-of-without]
author: SemanS
origin: agent
---

## Decision

- **A base is required.** Base defaults as before (the merge base of HEAD with origin/HEAD, else origin/main, else origin/master). When none resolves, drift says why instead of comparing HEAD with itself, and `--fail` returns non-zero.
- **Coverage needs a look in order.** An entry is listed when it is active, has paths, a non-knowledge file changed in the range matches them, and there is no look at or after the last governed change in the range. A look is an edit of the entry's file, a retirement, a person's read, or a passed verify. A change to the entry's file before the last governed change no longer covers it.
- Kept from the superseded entry: head defaults to the working tree (jj working copies included); kinds default to decision, convention, learning and incident, architecture only when asked; deterministic, no model; ordered by id with full commit ids; `kontext ci` runs check plus drift; a listing is a prompt to look, not a verdict; a warning in CI unless `--strict`.
- Capability string `drift.ordered`.

## Consequences

An entry edited early in a branch and then left behind by later commits on its paths is reported. A missing base is visible instead of reading as "no drift".
