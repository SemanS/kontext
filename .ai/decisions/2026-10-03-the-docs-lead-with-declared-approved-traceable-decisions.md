---
id: 2026-10-03-the-docs-lead-with-declared-approved-traceable-decisions
kind: decision
title: The docs lead with declared, approved, traceable decisions; adapters stay optional
status: accepted
date: 2026-10-03
paths: [docs/en/index.md, docs/sk/index.md, docs/en/getting-started/01-introduction.md, docs/sk/getting-started/01-introduction.md, README.md, README_SK.md]
author: SemanS
origin: cli
---

## Context
A draft added a catalog of popular projects wired in through adapters. The maintainer rejected it: kontext should do one job, be the declarative bridge between agents and git for a team's decisions, and be clearly better than Claude Code's memory for teams that must control what agents follow.

## Decision
The landing page, the introduction and both READMEs lead with three properties: declared (one file per decision with paths, status and date), approved (agents propose, people approve in pull requests, CODEOWNERS and CI apply), traceable (Decision: trailers, supersession, freshness). A comparison with CLAUDE.md/AGENTS.md, Claude Code's auto memory and memory services answers why not something else, and it states only what their official docs say. Adapters are presented as optional; the docs carry no catalog of third-party integrations.

## Consequences
New features are described by what they add to that job. Claims about other tools are checked against their current docs before they are published.
