---
id: 2026-10-29-supersession-is-derived-from-active-successors-and
kind: decision
title: Supersession is derived from active successors, and references are full ids
status: accepted
date: 2026-10-29
summary: "An entry named in the supersedes of an active entry is superseded on load, with superseded_by filled in; adr-tools ## Status sections are read and written in their own style; references are canonicalised to full ids on write; ambiguous targets are refused; kontext check reports one-sided supersessions."
tags: [store, supersede, adr]
paths: [src/store.rs, src/ops.rs, src/api_write.rs, src/cli.rs]
extends: [2026-09-28-reuse-existing-adr-directories-in-their-own-style]
verify: cargo test --lib successor_alone_supersedes
author: SemanS
origin: agent
---

## Context

Supersession was recorded only on the old file, so a successor whose predecessor was not rewritten left two active, contradictory entries (F19). adr-tools ADRs with a `## Status` section were read without a status (F20). Short prefixes from the brief were stored as references and later became ambiguous (F21).

## Decision

- **The effective status is computed on load.** An entry named in `supersedes` of an active entry is superseded, and its `superseded_by` is filled in, whether or not its own file says so.
- **ADR styles stay their own.** A `## Status` section (adr-tools, Nygard) is read and written in that style. A fields-style entry without a Status line gets one. `status` and `superseded_by` are edited as single lines, so nested YAML and comments survive.
- **References are full ids.** Promote, capture and stage refuse a target that does not resolve to exactly one entry and write the full id. The `starts_with("{s}-")` rule goes away. A duplicate ADR number is an error naming both files. Ids are allocated across all kinds; the staged check uses the `check_all` rule.
- `kontext check` reports a one-sided supersession; `kontext repair --dry-run` shows the edits that would add the missing side.

## Consequences

A successor alone is enough to retire its predecessor in every reader: brief, search, API and Peer. Old short references keep resolving where they are unique; new ones are always full.
