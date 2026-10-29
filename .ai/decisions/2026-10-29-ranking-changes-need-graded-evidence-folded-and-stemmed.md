---
id: 2026-10-29-ranking-changes-need-graded-evidence-folded-and-stemmed
kind: decision
title: Ranking changes need graded evidence; folded and stemmed fields are lower-boost shadows
status: accepted
date: 2026-10-29
summary: "No ranking change lands before a graded eval set (tests/eval/qrels.tsv, TREC format, nDCG@5, recall@10, MRR by slice, baseline in the repository, CI fails on a slice drop). Folding and stemming come as shadow fields title_f/body_f at a lower boost on Tantivy 0.26.2; superseded hits are demoted with their replacement, never hidden."
tags: [search, ranking, eval, tantivy]
paths: [src/index.rs, src/ops.rs, src/api_read.rs, tests/eval/**, tests/eval.rs]
extends: [2026-09-28-local-recall-is-tantivy-bm25-semantic-retrieval-comes-from]
author: SemanS
origin: agent
---

## Decision

- **Evidence first.** `tests/eval/qrels.tsv` (TREC format) over the shop fixture and this repository's `.ai/`, sliced by exact identifiers, singular/plural, Slovak with and without diacritics, paraphrase, and focused briefs at 200 to 1 400 tokens. nDCG@5, recall@10 and MRR are reported next to the current table; the baseline is committed and CI fails when a slice drops. Labels come from a developer script outside the binary and a different model, checked by a person, committed before the change they judge.
- **Shadow fields.** `title_f` and `body_f` with LowerCaser, AsciiFolding and the English stemmer, at a lower boost than the exact fields; `SCHEMA_VERSION` bump. Stay on Tantivy 0.26.2, which has both.
- **Order without hiding.** Sources weighted on the qrels (entries above doc chunks at equal score); superseded hits demoted with "→ superseded by [id]", never hidden; dedup over the whole normalised text never keeps a superseded twin above the active one; `entries.search` shortcuts only on an exact id or path.
- The default `[search] adapters` changes only with measured cold-start time and tokens in `tests/eval/README.md`.

## Alternatives

- **Upgrade to Tantivy 0.27 for folding and stemming**: rejected, 0.26.2 has both and the upgrade adds MSRV and schema risk.
- **Embeddings or a reranker in the core**: rejected, dense and rerank signals stay adapters, measured on the qrels.
- **Rank governing entries by English words such as never, only or must**: rejected, an unmeasured English-only heuristic.

## Consequences

Every ranking change carries a number on a committed set. Slovak and inflected queries reach entries without weakening exact-identifier hits.
