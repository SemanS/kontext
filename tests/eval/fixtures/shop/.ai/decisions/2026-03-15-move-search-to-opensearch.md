---
id: 2026-03-15-move-search-to-opensearch
kind: decision
title: Move product search to OpenSearch
status: proposed
date: 2026-03-15
paths: [src/search/**]
---

## Decision

Replace the SQL LIKE queries in product search with an OpenSearch index fed from the catalog.
