---
id: 2026-10-15-applications-consume-one-versioned-knowledge-service
kind: decision
title: Applications consume one versioned knowledge service through CLI and MCP
status: accepted
date: 2026-10-15
summary: Kontext owns knowledge formats, queries, applicability, extraction, staged writes and reviewed exports; applications own their UI, shared queues and delivery.
tags: [api, integration, knowledge]
paths: [src/api.rs, src/api_read.rs, src/api_write.rs, src/distill.rs, src/ops.rs, src/store.rs, src/main.rs, src/mcp.rs, src/tools.rs]
author: SemanS
origin: agent
---

## Context
Applications duplicated the knowledge parser, store layout, active-status rules and path matching, and parsed human CLI output to write entries. A separate memory path also wrote Markdown directly.

## Decision
Expose one product-independent application service through versioned JSON CLI and structured MCP. Keep the existing human commands and agent tools compatible. Kontext owns configured-store parsing, queries, applicability, extraction/formalization, local draft lifecycle, idempotent staging and committed exports pinned to an explicit review ref. Clients own UI, authorization, shared candidate queues, identities, coordination and session delivery. Stable operation IDs and local durable receipts reconcile retries; they do not pretend to transact atomically with a remote service.

## Consequences
Clients stop parsing Markdown/human output or implementing a second matcher/writer. Content status is distinct from Git publication evidence: staged or committed content is not inherently reviewed. Review-ref policy remains explicit at the caller. Source/session evidence stays local unless safe metadata is deliberately attached. Configured storage and ADR styles work uniformly across clients.
