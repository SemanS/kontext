---
id: 2026-10-20-freshness-counts-commits-since-the-last-look-at-an-entry
kind: decision
title: Freshness counts commits since the last look at an entry
status: accepted
date: 2026-10-20
checked: 2026-10-27T20:20:09Z
summary: Active decisions, conventions and learnings with paths are flagged after freshness.threshold_commits commits on their paths since the latest of their date, their checked confirmation and the last commit that changed their own file.
paths: [src/freshness.rs, src/store.rs, src/ops.rs, src/cli.rs]
supersedes: [2026-10-03-decision-freshness-counts-later-commits-on-its-paths-from]
author: SemanS
origin: cli
---

## Context

The first freshness check watched decisions only and counted from their date. A decision that someone had reviewed and found still true stayed flagged, so the flag was noise; conventions and learnings, which go stale just as often, were never flagged.

## Decision

Watch active decisions, conventions and learnings that have paths and a day to count from (their date or their checked confirmation). Count non-merge commits on their paths after the latest of: the day after their date, the day after their checked date, and the last commit in the history that changed the entry's own file (by its position in the log, since commits within one second share a timestamp). One git log still covers every entry, cached per HEAD with a fingerprint of the entries' ids, files, days and paths.

## Consequences

Confirming an entry (kontext confirm / ctx_confirm) or editing it resets its count, even before the change is committed. Incidents (a record of what happened) and module maps (refreshed by init) are not watched. An entry with neither a date nor a confirmation is still never flagged: the age of its file alone says nothing.
