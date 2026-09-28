---
id: 2026-09-28-every-mcp-tool-declares-annotations-none-is-marked
kind: decision
title: Every MCP tool declares annotations; none is marked destructive
status: accepted
date: 2026-09-28
tags: [mcp, codex]
paths: [src/tools.rs, src/connect.rs]
author: SemanS
origin: cli
---

## Context

Codex decides approval from MCP tool annotations. With approval_policy = "never" (a common setting for agents run by orchestrators such as Superset) it refused every kontext call: "MCP tool call requires approval, but approval policy is never". Tools marked destructiveHint: true are refused the same way.

## Decision

Every tool carries annotations: ctx_brief, ctx_search, ctx_read, ctx_why and ctx_log are readOnlyHint: true; the writing tools are destructiveHint: false — they touch only the local inbox, the working tree and the git index, and `drop` discards local drafts that were never shared. `kontext connect codex --write` also sets default_tools_approval_mode = "approve" on the server entry.

## Consequences

A new tool must get an entry in tools::annotations. Marking a tool destructive makes Codex refuse it under approval_policy = "never" — reserve it for tools that change shared state.
