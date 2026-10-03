---
id: 2026-10-03-decision-freshness-counts-later-commits-on-its-paths-from
kind: decision
title: Decision freshness counts later commits on its paths from one cached git log
status: accepted
date: 2026-10-03
summary: Flag decisions whose paths saw freshness.threshold_commits+ commits since their date, counted from one git log that is cached per HEAD.
paths: [src/freshness.rs, src/config.rs, src/ops.rs]
author: SemanS
origin: agent
---

## Context

Two agents built this roadmap item in parallel (branches kx-freshness-claude and kx-freshness-codex), and both were merged into one implementation. The goal was to flag a decision whose code moved on, without slowing the brief.

## Decision

A decision is flagged as possibly stale when at least `[freshness] threshold_commits` non-merge commits touched its `paths` after the day of its `date`. The default is 20, and 0 turns it off.

- A commit counts once, however many of its files match.
- A rename counts at both of its ends (`--no-renames`), so moving code out of a governed path counts.
- One `git log -z --name-only` covers every decision. It reads from the oldest decision's date, bounded by `index.max_commits`, with signatures off so that they do not land among the paths.
- The counts are cached in the worktree state dir, keyed by HEAD, `max_commits` and a fingerprint of the decisions' ids, dates and paths. A brief costs one `rev-parse` while nothing is committed, and never one git call per decision.
- The brief keeps room for its State section, so the flag survives a brief full of knowledge.

## Consequences

A commit count is cheap and explainable, and it needs no extra state beyond a derived cache. A flag is a prompt to check, not a verdict: a decision that still holds needs no change, and one that does not is superseded. Decisions without a date or paths are never flagged. Proposing `refresh:` tasks is left for later.
