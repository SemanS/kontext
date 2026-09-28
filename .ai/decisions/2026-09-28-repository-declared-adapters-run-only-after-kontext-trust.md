---
id: 2026-09-28-repository-declared-adapters-run-only-after-kontext-trust
kind: decision
title: Repository-declared adapters run only after kontext trust
status: accepted
date: 2026-09-28
paths: [src/config.rs]
author: SemanS
origin: cli
---

## Context
Adapters can start processes and call URLs. A cloned repository must not be able to make kontext (or its git hooks) run arbitrary commands.

## Decision
Adapters in the shared config are skipped until the user runs `kontext trust`, which pins a hash of the adapters table in ~/.config/kontext/trust.toml. User-level and clone-local configs are trusted.

## Consequences
One extra step per teammate for shared adapters; changing the table requires trusting it again.
