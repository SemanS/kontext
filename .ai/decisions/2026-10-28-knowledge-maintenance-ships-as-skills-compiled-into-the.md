---
id: 2026-10-28-knowledge-maintenance-ships-as-skills-compiled-into-the
kind: decision
title: Knowledge maintenance ships as skills compiled into the binary, and flows are a brief section
status: accepted
date: 2026-10-28
tags: [skills, flows, brief]
paths: [skills/**, src/skills.rs, src/connect.rs, src/ops.rs]
author: SemanS
origin: cli
---

## Context

A store that grew by capture is a pile: vocabulift's had 227 entries, no links between them before its flows, 101 with no recorded look, none with a verify command. Maintaining it is work an agent can do, but only inside a budget and under a person's review: repository knowledge goes stale in silence, frontier agents update it with 30–70 % F1, and public skill repositories show every substantive edit merged by a named human. kontext had the operations (drift, freshness, confirm, challenge, verify, entries.related, supersede) but no recipe that strings them together, and no way for a brief to say which process an entry belongs to.

## Decision

Maintenance recipes are skills: SKILL.md files in `skills/`, compiled into the binary with include_str! and installed per project by `kontext connect skills --write` into `.claude/skills/<name>/SKILL.md` (project scope, committed, re-running brings an edited copy back to the shipped text). Four ship: kontext-audit (a budgeted look, one outcome per entry, never delete), kontext-flows (reorganise into trails), kontext-verify (give entries a verify command), kontext-merge (duplicates and conflicts via entries.related, one superseding entry per group). Each ends in the inbox or staged files for a person to review.

A flow is an architecture entry with id `flow-<name>` or tag `flow` and a `## Trail` of `[[id]] — sentence` steps. A focused brief lists the flows whose paths overlap the focus (up to four, with step counts) after the learnings; an unfocused brief says in one line how many exist. Flows are recognised by shape, not by a new kind, so older binaries read them as ordinary architecture entries.

## Alternatives

- MCP prompts (like kontext-commit): rejected, a prompt is one message; the recipes need steps, commands and stop rules, and SKILL.md works in clients that never see the MCP prompt list.
- A new entry kind `flow` with its own trailer: rejected for now, it would change the entry format and every reader; the architecture kind carries the shape without a migration.
- Rendering flows as a graph: rejected, the trail is ordered and small; a graph of path overlaps is a hairball (192 of 213 vocabulift entries share a path prefix).

## Consequences

The binary carries the skills, so an upgrade changes them in a project only when the command is re-run. The skills name CLI and MCP operations; a renamed operation must update them (the unit test checks each skill's front matter and numbered steps, not the commands). The brief's flows section costs budget only with a focus.
