---
id: 2026-09-28-adapters-are-configuration-the-core-never-names-a-product
kind: decision
title: Adapters are configuration; the core never names a product
status: accepted
date: 2026-09-28
summary: External systems plug in through generic mcp/http/command drivers, capability ops and events declared in TOML presets.
paths: [src/adapters/**, presets/**]
commits: [8c35950]
origin: init
---

## Context

The stack spans OpenViking, CodeGraph, Serena, Agent LCM, sessions and LLM CLIs, and it will keep changing.

## Decision

The core knows capabilities (search, read, store, history, code, brief, llm, health), events (capture, promote, sync) and three drivers. Products exist only as presets; MCP arguments are bound from each tool's schema when a preset does not spell them out.

## Consequences

Swapping a memory system is a config change. Mapping quirks live in presets (`items`, `split`, `map`), and `kontext adapters inspect/test` is the way to write and check them.

