---
id: 2026-10-29-since-and-freshness-count-commits-by-ancestry-from-the
kind: decision
title: Since and freshness count commits by ancestry from the commit of the look; a failed verify is not a look
status: accepted
date: 2026-10-29
summary: "since and freshness count non-merge commits on governed paths reachable from HEAD and not from the commit of the last look, from one git log --format='%H %P' --name-only HEAD bounded by index.max_commits; a time window only when there is no anchor; a failed verify is shown but does not reset the count; truncated and lower_bound flags."
tags: [freshness, since, ledger, verify]
paths: [src/verification.rs, src/freshness.rs, src/ledger.rs, src/ops.rs]
supersedes: [2026-10-20-freshness-counts-commits-since-the-last-look-at-an-entry]
verify: cargo test --test maintenance since_counts_branch_merged_after_the_look
author: SemanS
origin: agent
---

## Context

Counting by position in `git log` or by time loses a change developed before a confirmation and merged after it, the usual GitHub merge commit (F10). A failed verify also counted as a look and reset the count.

## Decision

- **Ancestry, not position.** One read of `git log --format='%H %P' --name-only HEAD`, bounded by `index.max_commits`, keeps merge commits in the graph. `since` and freshness count non-merge commits on the governed paths that are reachable from HEAD and not from the anchor commit (the commit the last look named). A time window is used only when an entry has no anchor.
- **A failed verify is not a look.** It is shown, but it does not replace an earlier read and does not reset the count.
- **Agent and bulk stamps stay looks for freshness.** Changing that needs a decision and data (open question 4 of the 2026-10-29 analysis).
- **Honest bounds.** `truncated` when the bound cut the walk, `lower_bound` in a shallow clone.
- One map of ledger landings on HEAD serves the brief and the API, instead of two `git log -p` reads.
- **Changes a clause of 2026-10-27-a-verification-is-a-commit-in-a-ledger-and-the-brief-says-what-changed-since:** an entry's verification is no longer "the latest of its ledger lines and its frontmatter"; it is the latest look by the rules above (a failed verify excluded). That entry is superseded by 2026-10-29-verified-means-read-by-a-person-or-a-passed-verify-stamps, which carries this rule.
- Kept from the superseded entry: active decisions, conventions and learnings with paths are watched; editing or confirming an entry counts as a look; incidents and module maps are not watched; an entry with no anchor and no date is never flagged.

## Alternatives

- **Keep log position with a time fallback**: rejected, it is the bug.
- **Count merge commits as changes**: rejected, the changes they bring are already counted once through their parents.

## Consequences

A merged branch is counted however old its commits are. Performance is guarded by a synthetic 250-entry / 5 000-commit fixture: git calls counted through the `GIT_TRACE2_EVENT` test and brief time within +20 %.
