---
id: 2026-10-19-evidence-starts-in-extra-external-refs-with-quotes-checked
kind: decision
title: Evidence starts in extra.external_refs, with quotes checked in the body
status: accepted
date: 2026-10-19
summary: "Sources are references in external_refs with agreed shapes (pr:, issue:, task:, code:<repo>@<rev>:<path>#Lx-y, url:); quotes and their verdicts live in a ## Evidence body section; no schema field until the shape settles."
tags: [knowledge-maintenance]
paths: [src/evidence.rs, src/ops.rs, src/api_write.rs]
author: SemanS
origin: agent
---

## Context

A reviewer sees only an entry's text and cannot tell whether it rests on three independent findings or one sentence in a chat. Applications (Peer Hub) already pass `extra.external_refs` and `extra.evidence_hash` through drafts.stage.

## Decision

Start without a schema change. `evidence: [{ref, quote?}]` on any capture client is validated against the shapes `pr:<id>`, `issue:<id>`, `task:<id>`, `code:<repo>@<rev>:<path>[#L<a>[-<b>]]` and `url:<http(s)>` and stored in `extra.external_refs` (after references a client passes there itself). A quote (≤200 chars) of a `code:` reference into this repository is checked with `git show <rev>:<path>` at capture/stage; everything else is marked unverifiable, never refused. Quotes and verdicts are written into a `## Evidence` section of the body, where the pull request reviewer reads them. The brief shows only "N sources"; ctx_read L1 lists them.

## Consequences

Promote the list to a first-class `evidence:` field only once the shape has settled in use. Code references always carry a revision so they do not drift. E-mails and session ids stay out of references.
