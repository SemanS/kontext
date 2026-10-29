---
id: 2026-10-29-api-v1-grows-only-additively-and-narrows-an-input-only
kind: decision
title: API v1 grows only additively, and narrows an input only where it contradicted a recorded decision or security promise
status: accepted
date: 2026-10-29
summary: "Within version 1, operations, optional inputs, fields, error codes, enum values and capability strings may be added and clients ignore unknown ones; removing, renaming or changing the type or meaning of anything bumps the version; narrowing an input is allowed only where the old behaviour contradicted a recorded decision or a security promise, announced by a capability string and a migration note."
tags: [api, compatibility]
paths: [src/api.rs, src/api_read.rs, src/api_write.rs, src/tools.rs, docs/en/reference/05-application-api.md, docs/sk/reference/05-application-api.md]
extends: [2026-10-15-applications-consume-one-versioned-knowledge-service]
author: SemanS
origin: agent
---

## Decision

- **Additive within version 1.** Operations, optional inputs, response fields, error codes, enum values and capability strings may be added. Clients ignore what they do not know.
- **A version bump for anything else.** Removing or renaming an operation, input or field, or changing its type or meaning, bumps `api_version`. Parameters get aliases, never renames.
- **The narrowing exception.** An input may be narrowed within version 1 only when the old behaviour contradicted a recorded decision or a security promise. The narrowing is announced by a capability string and a migration note. This covers `amend_not_additive`, refusing `HEAD` as a revision, and `ctx_threads` refusing stdin over MCP.
- The policy is written in reference/05 (en, sk). `api.describe` returns the JSON Schema of every operation's input. One `model_unavailable` code, one `llm` parser, one notion of revision: `stale_entry` everywhere with the old code as an alias; the stale guard accepts only a full commit or blob hash.
- No `#[non_exhaustive]` or typed Rust errors until a crate consumer uses them.

## Alternatives

- **Bump the version for every narrowing**: rejected, it would make clients pay for a fix of behaviour that was never promised.
- **Never narrow within a version**: rejected, it would keep security holes and rewrite paths alive until 2.0.

## Consequences

Peer and other clients can upgrade within version 1 by reading capability strings. A narrowing needs a decision or a security promise to point to, which keeps it rare.
