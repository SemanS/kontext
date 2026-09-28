---
id: 2026-09-28-speak-mcp-directly-over-json-rpc-instead-of-depending-on-an
kind: decision
title: Speak MCP directly over JSON-RPC instead of depending on an SDK
status: accepted
date: 2026-09-28
paths: [src/mcp.rs, src/adapters/mcp_client.rs]
author: SemanS
origin: cli
---

## Context
kontext is both an MCP server and an MCP client (for adapters), and Rust SDK APIs were still moving.

## Decision
Implement the small part of the protocol we need by hand: newline-delimited JSON-RPC over stdio, streamable HTTP for clients, versions 2024-11-05 … 2025-11-25 negotiated, concurrent request handling.

## Consequences
No SDK churn and a tiny dependency tree; new protocol features (elicitation, sampling) must be added explicitly when needed.
