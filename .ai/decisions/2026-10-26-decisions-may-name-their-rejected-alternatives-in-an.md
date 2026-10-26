---
id: 2026-10-26-decisions-may-name-their-rejected-alternatives-in-an
kind: decision
title: "Decisions may name their rejected alternatives in an optional ## Alternatives section"
status: accepted
date: 2026-10-26
tags: [decisions, format]
paths: [src/init/deepen.rs, src/tools.rs, src/distill.rs, docs/en/concepts/02-knowledge-store.md, docs/sk/concepts/02-knowledge-store.md]
author: SemanS
origin: agent
---

## Context

A decision entry said what was chosen and why, but the options that were weighed and dropped lived only in prose, if at all. When a later challenge or drift reopens an entry, the first question is whether the proposed fix was already tried. agentworkforce/trajectories keeps `alternatives[]` with a reason on every decision; that part is worth having without its per-task narrative.

## Decision

A decision's body may carry `## Alternatives` between `## Decision` and `## Consequences`: each option considered and why it was rejected. It is optional and free Markdown, not front matter. `ctx_init_submit` takes an `alternatives` string for `dec:` tasks and writes the section; the `ctx_capture` body hint and the distill prompt mention it.

## Alternatives

A front-matter list `alternatives: [{option, reason}]`: structured, but reasons need sentences and every reader would have to render it. Making it required: most small decisions had no real alternative and would get filler.

## Consequences

Summaries and the action line still come from `## Decision`, so retrieval is unchanged. Entries without the section stay valid.
