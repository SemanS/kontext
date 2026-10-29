---
id: 2026-10-29-a-decision-is-never-amended-in-place-an-extension-of-one-is
kind: decision
title: A decision is never amended in place; an extension of one is a new linked entry
status: accepted
date: 2026-10-29
verify: cargo test --lib extend_refuses_decisions
paths: [src/api_write.rs]
author: SemanS
origin: agent
---

## Decision

drafts.extend and drafts.stage amends change an entry's body in place only for conventions, learnings and incidents. A decision (or adr) is refused with decision_not_amended: it changes only by supersession. A finding that builds on a decision is staged as a new entry with `extends: [<decision id>]` in its front matter.

## Consequences

Decision history stays supersede-only and an addition never reads as if the decision changed. Smaller kinds grow by topic instead of one file per fact. An amend re-checks the entry's revision (stale_entry) and keeps everything before the body byte for byte.
