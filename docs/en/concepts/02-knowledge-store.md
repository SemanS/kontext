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

### Flows {#flows}

A store that grew by capture is a pile sorted by date and kind. A **flow** is an `architecture` entry that reads it as a process: `architecture/flows/flow-<name>.md`, id `flow-<name>` (or tagged `flow`), `paths` naming the modules the process passes through, and a `## Trail` — a numbered list of `[[entry-id]] — one sentence` in the order a reader meets the entries, with `###` headings for stages. A flow links entries and changes nothing about them: no copy, no rename, no retirement, so trailers, the ledger and other links keep resolving. A focused brief lists the flows through the focus paths with their step count; the `kontext-flows` [skill](../agent-integrations/02-claude-code.md#skills) writes them.

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
checked: 2026-10-20T14:05:09Z  # optional: when someone last confirmed it still holds (or a date)
verify: cargo test -p billing cents   # optional: one command that shows it still holds
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

## Alternatives
Decimal columns: exact, but every language on the path needs a decimal type.

## Consequences
Conversions happen at the edges (API, UI). Existing float columns are migrated.
```

`## Alternatives` is optional: the options considered and why each was rejected, so a later challenge does not reopen one that already failed.

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

Recognized fields: Status, Date, Review by, Checked, Tags, Paths, Supersedes, Superseded by, Summary, Commits, Author/Authors/Deciders/Owner. `Status: Superseded by 0009` sets both the status and the link.

ADRs written with adr-tools keep the status in a `## Status` section (`Accepted`, `Superseded by [9. …](0009-….md)`). kontext reads that section and, on supersede or retire, writes it in the same style. A fields-style entry without a Status line gets one when its status changes.

## Ids and file names

| Numbering | File name | Typical for |
| --- | --- | --- |
| `date` (default for decisions) | `2026-09-28-prices-are-integer-cents.md` | new stores (no collisions between branches) |
| `sequential` | `0008-orders-live-in-postgresql.md` | existing ADR directories |
| `none` | `prices-are-integer-cents.md` | conventions, learnings |

The id is the file stem (or the front-matter `id`). Reading tools (`ctx_read`, `kontext read`, `confirm`, `challenge`) accept a unique prefix or, for sequential ADRs, the number (`0007`). What is written into an entry (`supersedes`, `extends`, `[[links]]`) is canonicalised to the full id, since a prefix that is unique today stops being unique when a longer id is added; an ambiguous target is refused. The brief prints full ids for the same reason. A duplicate ADR number is an error naming both files, and a new id is allocated across all kinds. A title without Latin letters gets a short hash id.

## Statuses and supersession

Entries are *active* unless `superseded`, `deprecated` or `rejected`. Capturing an entry with `supersedes: [0003]` marks the old entry `superseded` and links it back (`superseded_by`); for fields-style ADRs the `**Status:**` line is edited in place. `kontext log --all` shows the chains. This works for every kind, not only decisions.

Supersession is derived from the active successor, not only from the old file: an entry named in the `supersedes` of an active entry is superseded, and `superseded_by` is filled in when the store is read, even if the old file was never edited (a merge took one side, someone wrote the new entry by hand). `kontext check` reports such a one-sided supersede, and `kontext repair` (`--dry-run` to preview) writes the missing `status` and `superseded_by` lines. Those are edited as single lines, so nested YAML and comments survive.

An entry that no longer holds and has no replacement is *retired*: its status becomes `deprecated` and a `## Retired` section with the date and the reason is appended, so the claim and why it was dropped stay readable. Nothing is rewritten in place. Retirement goes through review like any change: an agent [challenges](./03-capture-and-review.md#challenges) the entry, and the retirement is staged with the commit (`ctx_prepare_commit retire=[id]`, or `drafts.retire` in the [application API](../reference/05-application-api.md#retire)).

## Amendments only add {#amendments}

Reviewed text is never rewritten in place. A convention, learning or incident grows by dated additions: a `## Added <YYYY-MM-DD>` section appended below the body, which is what `drafts.extend` drafts and `drafts.stage amends` writes. An amendment that changes the existing body is refused (`amend_not_additive`); change the claim by superseding the entry instead. A decision is never amended: a finding that builds on one is a new entry with `extends: [<id>]`, which `ctx_capture` and the inbox carry through to promotion. No model writes into an entry. The only in-place edits are metadata lines: `verify`, `checked`, and `status` / `superseded_by` on retire, supersede and `kontext repair`.

## Review dates {#review-by}

`review_by: YYYY-MM-DD` says when someone should look at an entry again: a rule that holds while a migration runs, an incident's temporary workaround. It is a review date, not an expiry. Once it passes, the entry is marked *review due* in the brief (on its line and in *State*), in search and in `kontext status`; nothing is hidden. Confirm it (move the date), supersede it or retire it. A malformed date, or one before the entry's `date`, is a validation error. When in doubt, leave the field out.

A kind can give its new entries a default with `review_after` (`90d`, `12w`, `6m`, `1y`), for example for incidents:

```toml
[store.kinds.incident]
review_after = "90d"
```

It is off unless set.

## Confirmations {#checked}

`checked` is when someone last confirmed that the entry still holds: a UTC time to the second (`YYYY-MM-DDTHH:MM:SSZ`, what `kontext confirm` writes) or a day (`YYYY-MM-DD`, fine by hand). Code moves on while an entry stays as it was written; when someone looks at the entry against the change and finds it still true, `kontext confirm <id>` (or `ctx_confirm`, or `drafts.confirm` in the [application API](../reference/05-application-api.md#confirm)) sets `checked` to the current UTC time in the entry's own file and stages it, so the confirmation is reviewed with the change that prompted it. The time makes a second confirmation later the same day a change of the file too, which is what drift needs to see. Nothing else in the file changes, byte for byte: the diff is one line (`**Checked:** …` in a fields-style ADR).

[Freshness](./04-context-layers.md#freshness) and [drift](./03-capture-and-review.md#drift) count from it, as they do from an edit of the entry. A time counts after its second, a day from the start of the next UTC day. `kontext check` refuses a `checked` in neither form or in the future. Retiring or superseding an entry keeps the field.

Each confirmation is also a line in the [verification ledger](./03-capture-and-review.md#verifications), `.ai/confirmations.log`, with the commit it looked at and how (`read`, `agent`, `bulk`); `checked` is a cache of it. When a merge leaves `checked` behind its ledger, `kontext check` warns and `kontext confirm --repair` rewrites it. An optional `verify:` field names one command that shows the entry holds; [`kontext verify`](./03-capture-and-review.md#verify) runs it.

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

## How entries are parsed {#parsing}

Front matter is read the way people and formatters write it:

- lists as flow lists (`[a, b]`), also wrapped over several lines (as prettier does), or as block lists (`- a`),
- trailing `# comments`, a UTF-8 BOM, CRLF line endings (kept when a line is edited),
- statuses `retired` and `archived`, and statuses with an emoji, read as `deprecated` with a warning.

A literal path in `paths` is anchored at the repository root; a bracketed segment such as `app/[slug]/page.tsx` matches the literal file first and then as a glob, the way git does, regardless of what the working tree holds. An entry that does not parse is an error in `kontext check`, never skipped silently.

## Keep entries brief

Entries are meant to be read by agents inside a token budget. `kontext check` warns when a body exceeds `store.max_body_lines` (80 non-empty lines by default). Move details to regular docs and link them.

## The index file

`.ai/README.md` lists every entry by kind with its date, status and one-line summary. Text you write above or below the markers is kept; the part between `<!-- kontext:index:start … -->` and `<!-- kontext:index:end -->` is regenerated. Because the file is re-rendered on every commit that touches knowledge and is marked `merge=union`, parallel branches do not conflict on it.

## Validation

`kontext check` (all entries) and `kontext check --staged` (exactly what would be committed; this is what the pre-commit hook runs) report:

| Level | Check |
| --- | --- |
| error | an entry that does not parse, missing title, unknown kind, duplicate id or ADR number, an ambiguous `supersedes` or `extends` target, a one-sided supersede whose old file still reads active (`kontext repair` fixes it), a malformed `review_by` or one before the entry's date, a `checked` that is not a date or lies in the future, a `verify` that is not one line of at most 500 characters, high-confidence secret |
| warning | decision without status or date, unusual status, malformed date, long entry, `supersedes` pointing nowhere, a reference that is a prefix rather than a full id, a superseded file that does not name its successor (`kontext repair` fixes it), a status read as `deprecated`, unparsable or future lines in the ledger, a `paths` pattern matching no tracked file, a `checked` behind its [ledger](./03-capture-and-review.md#verifications) (all entries only), medium-confidence secret |
| info | an active entry links to one that is superseded, retired or deprecated (a [changed premise](./03-capture-and-review.md#premises); all entries only) |
