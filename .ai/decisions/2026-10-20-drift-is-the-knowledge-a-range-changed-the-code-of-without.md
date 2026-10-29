---
id: 2026-10-20-drift-is-the-knowledge-a-range-changed-the-code-of-without
kind: decision
title: Drift is the knowledge a range changed the code of without a look at the entry
status: superseded
date: 2026-10-20
checked: 2026-10-27T20:20:09Z
summary: entries.drift lists active entries whose paths match files changed in base..head (or base..HEAD plus the working tree) and whose own file was not changed in the same range; deterministic, git-only, no model.
paths: [src/drift.rs, src/api_read.rs, src/cli.rs, src/tools.rs]
author: SemanS
origin: cli
superseded_by: [2026-10-29-drift-needs-a-base-and-a-look-at-or-after-the-last-governed]
---

## Context

prepare-commit lists the entries that govern staged paths, but it runs from git hooks, and a team that commits with jj (colocated) never runs them. Nothing answered which entries a range of commits left behind, for CI or for a coordination tool, and a review of one repository found about ten entries that no longer matched the code.

## Decision

entries.drift (kontext drift, ctx_drift) takes base (default: the merge base of HEAD with origin/HEAD, else origin/main, else origin/master, else HEAD), head (default: the working tree, so uncommitted and untracked changes count, which is how a jj working copy looks) and kinds (default decision, convention, learning, incident; never architecture unless asked). An entry is listed when it is active, has paths, a non-knowledge file changed in the range matches them (glob::path_matches, as governing does), and its own file did not change in the same range: an edit, a retirement or a confirmation (checked) counts as having looked. With an explicit head the entries are read as committed there. Output is ordered by id, with full commit ids. kontext ci runs check plus drift over base..HEAD (committed only), as text or Markdown.

## Consequences

Detection costs no model tokens and works the same under git and jj. A listed entry is a prompt to look, not a verdict: confirm it, edit it, or supersede or retire it. Drift is a warning in CI unless --strict.
