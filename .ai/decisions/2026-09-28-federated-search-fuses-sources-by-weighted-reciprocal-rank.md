---
id: 2026-09-28-federated-search-fuses-sources-by-weighted-reciprocal-rank
kind: decision
title: Federated search fuses sources by weighted reciprocal rank
status: accepted
date: 2026-09-28
tags: [search, adapters]
paths: [src/model.rs]
author: SemanS
origin: cli
---

## Context

Search normalized each source by its best score. Semantic adapters return similarity scores that sit close together (0.62, 0.61, 0.60 …) while BM25 falls off steeply, so after normalization an adapter such as OpenViking took six of eight places and pushed local decisions and docs out.

## Decision

Merge ranked lists by weighted reciprocal rank: a hit scores weight / (8 + rank) in its source. Ties keep the local source first; adapter copies of local entries (a synced …/<id>.md) collapse into the local hit.

## Consequences

Within-source score gaps no longer matter across sources; a single source keeps its own order, so local-only search is unchanged. An adapter's weight shifts it by rank, not by score.
