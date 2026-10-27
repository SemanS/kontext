---
id: 2026-10-27-a-verification-is-a-commit-in-a-ledger-and-the-brief-says-what-changed-since
kind: decision
title: A verification is a commit in a ledger, and the brief says what changed since; a date in the entry is only a cache
status: accepted
date: 2026-10-27
summary: Confirmations and executable checks are appended to .ai/confirmations.log (union-merged) with the commit they looked at and how they looked; entries carry an optional verify command; the brief and the machine API say for each governing entry how it was verified and how many commits touched its paths since. The checked date in the entry stays as a cache, never as the proof.
tags: [freshness, drift, confirm, ledger, verify, brief]
paths: [src/freshness.rs, src/drift.rs, src/api_write.rs, src/api_read.rs, src/ops.rs, src/store.rs, src/cli.rs, src/tools.rs, src/hooks.rs, docs/en/concepts/03-capture-and-review.md, docs/sk/concepts/03-capture-and-review.md]
author: SemanS
origin: agent
---

## Context

A pass over the 208 entries of one production store (vocabulift, 2026-10-27) found the content sound: three of eight candidates needed a correction, none had turned false with age. What failed was the proof. `checked:` in the entry's frontmatter is a stamp anyone can set, and it was set in bulk: one feature commit confirmed about forty entries, one merge-resolution commit confirmed fourteen. Two integration merges then resolved frontmatter conflicts by taking the older side, so at least eight entries show an older confirmation than they had. One learning written and confirmed by an agent was wrong on the day it was written (where a tool lives), and no stamp would have caught it; a test would have. A dependency bump that an entry depended on (calamine 0.36.1 to 0.37) touched no governed path, so drift was blind to it.

What the research says, read on 2026-10-27: a freshness cue alone does not change what an agent does with a settled-looking constraint (arXiv 2608.25553: re-verified in one episode in five, stale-consistent decisions in 75 %); what helped was putting the governing record on the agent's critical path with its provenance. Freshness of an observation is not currency of a derivation: the check must compare against the inputs the belief was derived from, which for us is the commit a verification looked at (2609.03340). Machine-observable facts are verified read-only, developer intent is asked (2610.05241). Agents detect an invalidated belief about half the time even with the evidence in front of them (2605.06527), and LLM rewriting of stored lessons degrades them, so raw records stay and consolidation is gated (2605.12978). In AI-generated pull requests, 63 % change code without touching documentation and the debt compounds (2610.27381). A six-week study of an AGENTS.md system found drift the largest failure class and a single LLM audit catching about half of known drift, so audits repeat and lessons get retired, not only confirmed (2610.32578).

## Decision

- **A ledger holds verifications.** `kontext confirm`, `ctx_confirm` and the new `kontext verify` append one line to `.ai/confirmations.log`: time (UTC, to the second), entry id, how it was looked at (`read` from the CLI, `agent` from the MCP tool, `bulk` when one call confirms more than five entries, `verify` when a command ran, with its outcome), the commit HEAD named at that moment, who (git `user.name`), and an optional one-line reason (`--because`). `kontext init` writes `.gitattributes` with `merge=union` for the log, and the first write ensures it. The log is append-only; nothing rewrites a line.
- **The date in the entry is a cache.** `checked:` keeps being written into the entry's file so that older readers and Peer's current revision work. On read, an entry's verification is the latest of its ledger lines and its frontmatter; `kontext check` reports an entry whose frontmatter is behind its ledger (a merge dropped it) and `kontext confirm --repair` rewrites the cache from the ledger.
- **An entry may name how to check it.** An optional `verify:` field holds one shell command run from the repository root (bounded by a timeout). `kontext verify <id>` and the machine operation run it and append a `verify` line with pass or fail; `ctx_verify` is the MCP tool. Only a `read` by a person or a passed `verify` counts as verified; `agent` and `bulk` lines are kept and shown, but they are stamps.
- **The brief carries derivation currency.** For each entry it serves for the focus paths, the brief adds how it was last verified (kind, short commit, date) and, when commits touched its paths since that commit, how many and the first subjects; once per brief, when any such entry is listed: read these before changing their paths, an entry that limits the direction you are about to take outranks a fresh one. Freshness keeps its threshold for the state section; this line has none, since one commit on a governed path is already a reason to read.
- **The machine API says the same.** `entries.list`, `entries.get`, `entries.governing` and `entries.drift` gain additive fields: `verification` (`at`, `kind`, `commit`, `by`, `because`, `passed`), `since` (`commits`, `head`, the changed governed paths, at most twenty), `verify` (the command) and `premises` (entries this one links to that are superseded, retired or deprecated). `drafts.confirm` takes `because` and `kind`; `drafts.verify` is new. API version stays 1: a reader that ignores the fields loses nothing.
- **Superseding cascades as a question.** Retiring or superseding an entry marks every active entry that links to it (`[[id]]`, `supersedes`, `extends`) as having a changed premise; the brief, drift and the API say so. It is a flag, never an automatic change of the dependent entry.
- **Agents append or supersede, never rewrite.** An agent's change to an existing entry is a dated addition under its body or a superseding entry. `ctx_capture` with `supersedes` stays the way to say an entry no longer holds.

## Consequences

A verification can be trusted by what it names: the commit it looked at and whether a command ran. A merge cannot lose it. Peer and the brief can show "verified by its test at a7450a5, three commits on its paths since" instead of a date, and the entry reaches the agent at the moment it touches those paths, which is the one place the research shows a provenance line changes behaviour. Bulk stamping is still possible and now visible as such. Entries whose dependencies are outside their paths still need a `deps:` field later; this decision does not add it.

## Alternatives

- **Re-verify on a calendar** (daily, weekly): rejected. Age is not the trigger; a change of the governed paths is, and a calendar pass would stamp in bulk, which is the failure this corrects.
- **Keep `checked:` in frontmatter as the only record**: rejected. It conflicts in merges and the losing side vanishes; it says nothing about what was looked at or how.
- **A verification badge with a date in the UI**: rejected. A content-free freshness cue did not redirect agents in the cited study; a commit and a kind did.
- **Let agents rewrite entries in place when code changes**: rejected. Consolidation by a model degrades the record it depends on; append or supersede keeps the evidence.
