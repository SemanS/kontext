---
id: 2026-10-03-decision-freshness-counts-later-commits-on-its-paths-from
kind: decision
title: Decision freshness counts later commits on its paths from one cached git log
status: accepted
date: 2026-10-03
summary: Flag decisions whose paths saw brief.stale_after_commits+ commits since their date, computed by one git log cached per HEAD.
paths: [src/freshness.rs]
author: SemanS
origin: agent
---

A decision is flagged as possibly stale when at least `brief.stale_after_commits` (default 20, 0 = off) non-merge commits touched its `paths` on a later day than its `date`. One `git log --name-only` since the oldest watched decision (bounded by `index.max_commits`) covers all decisions; the counts are cached in the worktree state dir keyed by HEAD, max_commits and a fingerprint of the decisions' ids/dates/paths, so a brief costs one `rev-parse` when nothing changed and never one git call per decision.

Why: commit count is cheap, explainable and needs no extra state; a flag is a prompt to re-check, not a verdict. Decisions without date or paths are never flagged. Proposing `refresh:` tasks is left for later.
