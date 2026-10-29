---
id: 2026-10-29-an-amendment-only-appends-a-dated-section-a-model-never
kind: decision
title: An amendment only appends a dated section; a model never writes into an entry
status: accepted
date: 2026-10-29
summary: "drafts.stage amends accepts a body only when the old body is its byte prefix and the rest is ## Added <YYYY-MM-DD> sections, else amend_not_additive; title and every field outside the body must parse unchanged. drafts.extend with llm appends the finding as written. Decisions change only by supersession; the only in-place edits are metadata lines."
tags: [api, amend, extend, review]
paths: [src/api_write.rs, src/store.rs, src/tools.rs, skills/**]
supersedes: [2026-10-29-a-decision-is-never-amended-in-place-an-extension-of-one-is]
verify: cargo test --lib stage_amend_refuses_rewritten_body
author: SemanS
origin: agent
---

## Context

0.9.0 let `drafts.stage amends` replace a body with any text and let `drafts.extend` with `llm` fold a finding into the body through a model. Both rewrote reviewed text in place, against 2026-10-27 ("append or supersede, never rewrite"). A fields-style body could also inject `**Status:**` lines or drop the title (F122, F186).

## Decision

- **Kept from the superseded entry.** A decision (or adr) is never amended: `decision_not_amended`, it changes only by supersession. A finding that builds on a decision is a new entry with `extends: [<decision id>]`. Amends re-check the entry's revision (`stale_entry`).
- **An amendment only appends.** `drafts.stage amends` accepts a body only when the old body is a byte prefix of it and the rest consists of `## Added <YYYY-MM-DD>` sections. Anything else is `amend_not_additive`, whose message points to supersede. After the splice the result is parsed again: the title and every field outside the body must be identical.
- **A model never writes into an entry.** `drafts.extend` still accepts `llm` but does not fold: it appends the finding as written and answers `as_written: true` with a note. The response shape is unchanged.
- **The only in-place edits are metadata lines:** `verify`, `checked`, `status` and `superseded_by` on retire and supersede, and `repair` filling in a missing side of a supersession.
- `extends` passes through `ctx_capture`, the inbox and promotion. Resuming a stage plan takes the kind from `kind_for_path`.
- One wording of the rule in `src/tools.rs`, concepts/02 and the kontext-verify skill: reviewed text is not rewritten; conventions, learnings and incidents grow by dated additions.

## Alternatives

- **Let the model phrase only the added section**: open (question 3 of the 2026-10-29 analysis); still additive and reviewed, but not adopted until decided.
- **Keep model folding behind review**: rejected, a reviewer sees a diff of rewritten prose and the record degrades with each fold.

## Consequences

Narrowing `amends` is allowed under the API v1 policy because the old behaviour contradicted a recorded decision; it is announced by a capability string and a migration note. Peer's keep flow still works because the response shape is the same.
