---
id: 2026-09-28-git-hooks-never-wait-on-adapters
kind: decision
title: Git hooks never wait on adapters
status: accepted
date: 2026-09-28
paths: [src/hooks.rs, src/events.rs]
author: SemanS
origin: cli
---

## Context
Commits must stay instant and must not fail because a memory service is down.

## Decision
Hooks only validate, stage the index and queue events in an outbox; a detached `kontext outbox flush` delivers them and keeps failures for retry (up to 8 attempts). Every hook block is guarded so machines without kontext are unaffected.

## Consequences
Adapter state is eventually consistent (`kontext outbox` shows what is pending).
