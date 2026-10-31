---
id: 2026-11-01-a-brief-with-more-governing-entries-than-fit-shows-readable
kind: decision
title: A brief with more governing entries than fit shows readable lines and a count, not a wall of ids
status: accepted
date: 2026-11-01
verify: cargo test --quiet --lib the_governing_block_never_exceeds_its_budget
summary: "Ids stay on the also-governing line while they fit; past that the closest entries print as compact lines, then `+N more governing: ctx_why <path>`. The structured result carries only the text and the ids it names."
paths: [src/ops.rs, src/tools.rs]
author: SemanS
origin: agent
extends: [2026-10-29-a-focused-brief-never-silently-drops-a-governing-entry]
---

## Context

A focused brief at 1 400 tokens on a broad path of this kind of repository governed by 106 entries printed nothing but ids, and the MCP result repeated them in `entries`, `governing[].entries`, `governing[].text` and one resource link each: over 60 KB for one call. An id tells an agent nothing about what it must keep to.

## Decision

The rule of 2026-10-29 stands: a focused brief never silently drops a governing entry. Where every id fits, it prints them as before. Where even the ids do not fit, it prints as many compact lines as fit, closest first, then the count of the rest and how to see them. `ctx_brief`'s structured result is `{text, entries}`, where entries are those the text names, so the budget is what the agent reads.

## Consequences

The budget is honest in tokens an agent pays for. `governing_lines` only served the removed `governing` field and is gone.
