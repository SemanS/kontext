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
| `architecture` | `architecture/` | — | the overview and module docs (mostly generated) |

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
| `date` (default for decisions) | `2026-09-28-prices-are-integer-cents.md` | new stores — no collisions between branches |
| `sequential` | `0008-orders-live-in-postgresql.md` | existing ADR directories |
| `none` | `prices-are-integer-cents.md` | conventions, learnings |

The id is the file stem (or the front-matter `id`). Anywhere an id is expected you can use a unique prefix or, for sequential ADRs, the number (`0007`).

## Statuses and supersession

Decisions are *active* unless `superseded`, `deprecated` or `rejected`. Capturing a decision with `supersedes: [0003]` marks the old entry `superseded` and links it back (`superseded_by`); for fields-style ADRs the `**Status:**` line is edited in place. `kontext log --all` shows the chains.

## Keep entries brief

Entries are meant to be read by agents inside a token budget. `kontext check` warns when a body exceeds `store.max_body_lines` (80 non-empty lines by default) — move details to regular docs and link them.

## The index file

`.ai/README.md` lists every entry by kind with its date, status and one-line summary. Text you write above or below the markers is kept; the part between `<!-- kontext:index:start … -->` and `<!-- kontext:index:end -->` is regenerated. Because the file is re-rendered on every commit that touches knowledge and is marked `merge=union`, parallel branches do not conflict on it.

## Validation

`kontext check` (all entries) and `kontext check --staged` (exactly what would be committed; this is what the pre-commit hook runs) report:

| Level | Check |
| --- | --- |
| error | missing title, unknown kind, duplicate id, high-confidence secret |
| warning | decision without status or date, unusual status, malformed date, long entry, `supersedes` pointing nowhere, a `paths` pattern matching no tracked file, medium-confidence secret |
