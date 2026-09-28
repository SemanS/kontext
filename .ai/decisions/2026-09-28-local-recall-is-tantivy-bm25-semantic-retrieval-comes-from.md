---
id: 2026-09-28-local-recall-is-tantivy-bm25-semantic-retrieval-comes-from
kind: decision
title: Local recall is Tantivy BM25; semantic retrieval comes from adapters
status: accepted
date: 2026-09-28
paths: [src/index.rs]
author: SemanS
origin: cli
---

## Context
Most agent questions are code-shaped (identifiers, paths, ticket keys), and a vector store would add a service and an embedding model to every clone.

## Decision
The core indexes entries, repo docs (section-chunked) and commit history with Tantivy BM25, with camelCase/snake_case expansion and phrase matching for identifiers. Semantic search is federated from adapters such as OpenViking.

## Consequences
Fast (sub-second on a 3k-file monorepo) and offline; paraphrase-style recall depends on having a semantic adapter configured.
