---
id: 2026-10-29-verified-means-read-by-a-person-or-a-passed-verify-stamps
kind: decision
title: Verified means read by a person or a passed verify; stamps are looks, never verified, and checked is a cache
status: accepted
date: 2026-10-29
summary: "Only a person's read or a passed verify is verified; agent and bulk stamps are looks; a bare checked: is kind cache, verified:false. The CLI writes read only on a TTY, MCP never, kontext api records by: application:<name>. The API adds last_verified, since_verified, failing and amended_since. Retained from 2026-10-27: the ledger, append or supersede never rewrite, superseding cascades as a question, and the API field contract."
tags: [verify, ledger, freshness, drift, api]
paths: [src/verification.rs, src/ledger.rs, src/freshness.rs, src/drift.rs, src/api_write.rs, src/api_read.rs, src/ops.rs, src/store.rs, src/cli.rs, src/tools.rs, src/hooks.rs]
supersedes: [2026-10-27-a-verification-is-a-commit-in-a-ledger-and-the-brief-says-what-changed-since]
author: SemanS
origin: agent
---

## Context

The brief wrote `verified agent at …` and `verified bulk` (F35), a bare `checked:` with no ledger line was reported as `verified read` (F8), and an agent could write `read` through Bash (F9). 2026-10-27 stated the right intent, but its clause "verification is the latest of the ledger lines and the frontmatter" let a cache or a stamp pass as proof.

## Decision

- **What counts as verified.** A `read` by a person or a passed `verify`. `agent` and `bulk` lines are looks: kept, shown, used by freshness, never written as "verified". A failed verify sets `failing: true` and is not a look.
- **`checked:` is a cache.** A `checked:` in front matter with no ledger line has kind `cache` and `verified: false`. The field is still written, because Peer reads it in `drift.ts`. `kontext confirm --migrate-legacy` writes `bulk` lines for legacy stamps.
- **Who writes `read`.** The CLI writes `read` only on a TTY; without one it writes `agent`, and `--kind read` without a TTY warns. A TTY is not proof of a person, and this decision does not claim it is. MCP never writes `read`. `kontext api` keeps `read` (the application owns identity), but the ledger line carries `by: application:<name>` so it is distinguishable from a terminal read.
- **API, additive.** `last_verified` and `since_verified` next to the existing fields; `failing: true`; `amended_since: true` when the body changed after the last verifying look. Capability string `verification.last_verified`.
- **How a look is counted** follows 2026-10-29-since-and-freshness-count-commits-by-ancestry-from-the: by ancestry from the commit of the latest look, a failed verify excluded. This replaces the clause "the latest of its ledger lines and its frontmatter".

## Retained from 2026-10-27

- **The ledger.** `kontext confirm`, `ctx_confirm` and `kontext verify` append one line to `.ai/confirmations.log` (time, id, kind, HEAD commit, who, optional reason); union-merged; append-only. An entry may name a `verify:` command, run under 2026-10-29-a-verify-command-is-code-from-the-repository-and-runs-only.
- **Agents append or supersede, never rewrite.** An agent's change to an existing entry is a dated addition under its body or a superseding entry; `ctx_capture` with `supersedes` says an entry no longer holds.
- **Superseding cascades as a question.** Retiring or superseding an entry marks every active entry that links to it (`[[id]]`, `supersedes`, `extends`) as having a changed premise; the brief, drift and the API say so. It is a flag, never an automatic change of the dependent entry.
- **The API field contract.** `entries.list`, `entries.get`, `entries.governing` and `entries.drift` carry `verification` (`at`, `kind`, `commit`, `by`, `because`, `passed`, `landed`), `since` (`commits`, `head`, changed governed paths, at most twenty), `verify` and `premises`. `drafts.confirm` takes `because` and `kind`; `drafts.verify` exists. API version stays 1.
- **The brief carries derivation currency** for each entry it serves for the focus paths, with the line to read these before changing their paths.

## Alternatives

- **Treat recent agent and bulk stamps as verified**: rejected, a content-free stamp is the failure 2026-10-27 set out to correct.
- **Stop writing `checked:`**: rejected until Peer reads the verification fields.
- **Sign or chain the ledger**: rejected (F66); review of the TSV diff is the integrity model, `check` only warns on future-dated and unparsable lines.

## Consequences

"verified" means one thing in the brief and the API. Existing stores lose their implicit verified status until a person reads or a check passes, which is the truth. The append-only amendment rule (2026-10-29-an-amendment-only-appends-a-dated-section-a-model-never) keeps its footing in the retained clause.
