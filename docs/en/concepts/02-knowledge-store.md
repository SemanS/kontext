# Knowledge store

The store is the team's shared, reviewed knowledge: short Markdown files in the repository, one per decision, convention, learning, incident or architecture note.

## Layout

```text
.ai/
├── README.md                 generated index (union-merged, refreshed by the pre-commit hook)
├── kontext.toml              shared configuration (committed)
├── .gitattributes            README.md merge=union
├── decisions/                2026-09-28-prices-are-integer-cents.md   (or your existing docs/adr)
├── conventions/
├── learnings/                pitfalls are learnings tagged `pitfall`
├── incidents/
└── architecture/
    ├── overview.md           purpose + generated stack, layout and history facts
    └── modules/*.md          one per module: an overview written by an agent + generated facts
```

The directory name and every kind's location are configurable ([configuration](../reference/03-configuration.md#store)). An existing ADR directory is reused as the home of decisions.

## Kinds

| Kind | Directory | Trailer | Use it for |
| --- | --- | --- | --- |
| `decision` | `decisions/` | `Decision:` | a direction taken, an alternative rejected, a rule introduced |
| `convention` | `conventions/` | `Convention:` | how the team does something (naming, layering, testing) |
| `learning` | `learnings/` | `Learning:` | non-obvious lessons and pitfalls ("gotchas") |
| `incident` | `incidents/` | `Incident:` | what broke, why, what changed |
| `architecture` | `architecture/` | – | the overview and module docs (mostly generated) |

Aliases are accepted when capturing: `adr` → decision, `pitfall`/`gotcha` → learning (tagged `pitfall`), `note`/`lesson` → learning, `rule`/`guideline` → convention, `postmortem`/`outage` → incident.

## Entry format

The default style is flat front matter plus a short body:

```markdown
---
id: 2026-09-28-prices-are-integer-cents
kind: decision
title: Prices are integer cents
status: accepted            # proposed | accepted | superseded | deprecated | rejected
date: 2026-09-28
review_by: 2027-03-31        # optional: when to look at it again (not an expiry)
summary: Every amount is an integer number of cents.   # optional; derived from the body otherwise
tags: [billing]
paths: [src/billing/**]      # what this governs — used by ctx_brief, ctx_why, prepare-commit
supersedes: []
commits: [a1b2c3d]           # optional: commits the decision came from
author: Jane Doe
---

## Context
Floats broke VAT rounding on invoices with many lines.

## Decision
Every amount is stored and computed as an integer number of cents.

## Consequences
Conversions happen at the edges (API, UI). Existing float columns are migrated.
```

Unknown front-matter keys are preserved when kontext rewrites a file.

### Classic ADRs (fields style)

Repositories that already keep ADRs usually write them like this, and kontext reads and writes them as they are:

```markdown
# 0007 — Orders live in PostgreSQL

**Status:** accepted
**Date:** 2026-08-04

## Context
…
```

Recognized fields: Status, Date, Tags, Paths, Supersedes, Superseded by, Summary, Commits, Author/Authors/Deciders/Owner. `Status: Superseded by 0009` sets both the status and the link.

## Ids and file names

| Numbering | File name | Typical for |
| --- | --- | --- |
| `date` (default for decisions) | `2026-09-28-prices-are-integer-cents.md` | new stores (no collisions between branches) |
| `sequential` | `0008-orders-live-in-postgresql.md` | existing ADR directories |
| `none` | `prices-are-integer-cents.md` | conventions, learnings |

The id is the file stem (or the front-matter `id`). Anywhere an id is expected you can use a unique prefix or, for sequential ADRs, the number (`0007`).

## Statuses and supersession

Entries are *active* unless `superseded`, `deprecated` or `rejected`. Capturing an entry with `supersedes: [0003]` marks the old entry `superseded` and links it back (`superseded_by`); for fields-style ADRs the `**Status:**` line is edited in place. `kontext log --all` shows the chains. This works for every kind, not only decisions.

An entry that no longer holds and has no replacement is *retired*: its status becomes `deprecated` and a `## Retired` section with the date and the reason is appended, so the claim and why it was dropped stay readable. Nothing is rewritten in place. Retirement goes through review like any change: an agent [challenges](./03-capture-and-review.md#challenges) the entry, and the retirement is staged with the commit (`ctx_prepare_commit retire=[id]`, or `drafts.retire` in the [application API](../reference/05-application-api.md#retire)).

## Review dates {#review-by}

`review_by: YYYY-MM-DD` says when someone should look at an entry again: a rule that holds while a migration runs, an incident's temporary workaround. It is a review date, not an expiry. Once it passes, the entry is marked *review due* in the brief (on its line and in *State*), in search and in `kontext status`; nothing is hidden. Confirm it (move the date), supersede it or retire it. A malformed date, or one before the entry's `date`, is a validation error. When in doubt, leave the field out.

A kind can give its new entries a default with `review_after` (`90d`, `12w`, `6m`, `1y`), for example for incidents:

```toml
[store.kinds.incident]
review_after = "90d"
```

It is off unless set.

## Evidence {#evidence}

An entry can say where it comes from. Its sources are references in `external_refs` (kept in the front matter, or in the *Kontext metadata* line of a fields-style ADR):

| Reference | Example |
| --- | --- |
| a pull request, an issue, a task | `pr:123`, `issue:45`, `task:krk-335` |
| lines of a file at a revision | `code:kontext@3f2a91c:src/x.rs#L10-20` |
| anything else that opens | `url:https://…` |

A file reference without a revision drifts as the file changes, so `code:` always names one. A capture can add a short quote (up to 200 characters) to a reference. A quote of a `code:` reference into this repository is checked with `git show <rev>:<path>` when it is captured or staged; anything else cannot be checked and is marked so, never refused. Quotes and their verdicts go into a `## Evidence` section of the body, where the reviewer reads them:

```markdown
## Evidence

- code:kontext@3f2a91c:src/payments/retry.rs#L4 — "one idempotency key per checkout attempt" (verified)
- url:https://provider.example/docs/keys — "keys expire after 24 hours" (not verified)
```

The brief says only how many sources an entry has (`(3 sources)`); `ctx_read` at level 1 lists them. E-mail addresses and session ids do not belong in references.

## Keep entries brief

Entries are meant to be read by agents inside a token budget. `kontext check` warns when a body exceeds `store.max_body_lines` (80 non-empty lines by default). Move details to regular docs and link them.

## The index file

`.ai/README.md` lists every entry by kind with its date, status and one-line summary. Text you write above or below the markers is kept; the part between `<!-- kontext:index:start … -->` and `<!-- kontext:index:end -->` is regenerated. Because the file is re-rendered on every commit that touches knowledge and is marked `merge=union`, parallel branches do not conflict on it.

## Validation

`kontext check` (all entries) and `kontext check --staged` (exactly what would be committed; this is what the pre-commit hook runs) report:

| Level | Check |
| --- | --- |
| error | missing title, unknown kind, duplicate id, high-confidence secret |
| warning | decision without status or date, unusual status, malformed date, long entry, `supersedes` pointing nowhere, a `paths` pattern matching no tracked file, medium-confidence secret |
