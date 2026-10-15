---
id: 2026-10-15-mcp-annotations-distinguish-the-application-apis-external
kind: decision
title: MCP annotations distinguish the application API's external adapter access
status: accepted
date: 2026-10-15
summary: Every tool retains annotations; the mixed application API declares possible external adapter access and local non-destructive writes.
tags: [mcp, api]
paths: [src/tools.rs, src/mcp.rs, src/api.rs]
supersedes: [2026-09-28-every-mcp-tool-declares-annotations-none-is-marked]
author: SemanS
origin: agent
---

## Context
The structured application API includes read operations, extraction through configured LLM adapters, and capture/staging that can queue adapter events. Declaring that entire surface closed-world would misdescribe its observable behavior.

## Decision
Keep annotations on every MCP tool and mark local inbox/store/index writes as non-destructive. Existing text tools retain their annotations. The mixed ctx_api tool is conservatively readOnlyHint=false, destructiveHint=false, idempotentHint=false and openWorldHint=true because some operations can contact configured external adapters. Its description distinguishes local knowledge files from adapter traffic. Individual read and retry semantics remain documented by operation.

## Consequences
Clients receive accurate capability hints without assuming every application operation is read-only or every retry idempotent. Configured adapter trust and the normal Git review boundary continue to apply.
