# Evaluation set

Real situations an agent meets, run against the `kontext` binary on a fixture repository
(`fixtures/shop`: a small web shop with decisions, conventions, learnings and an incident).
Each scenario is scored as found / missed / wrongly suppressed, plus noise (what must not
appear), bad exit statuses, output tokens (characters / 4) and wall-clock milliseconds.

```sh
cargo test --test eval -- --nocapture                  # the table; also target/tmp/eval-report.md
KONTEXT_EVAL_ONLY="exact phrase" cargo test --test eval -- --nocapture
KONTEXT_EVAL_VERBOSE=1 …                               # print the output of a step that failed
KONTEXT_EVAL_JSON=/tmp/eval.json …                     # the rows as JSON too
```

Every sandbox runs with `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1` and its own HOME
(`tests/common/mod.rs`), so a developer's git config cannot change a result.

## Scoring

Each check of a `run` step is one expectation:

- **found**: an `expect` or `keep` string appears, a `rank` needle is within its rank, the output is within `max_tokens`.
- **missed**: an `expect` string is absent, a ranked hit is below its rank (also counted under **rank**), or the output is over its token limit (also counted under **over budget**).
- **wrongly suppressed**: a `keep` string is absent although its source exists (a hit dropped by deduplication, a candidate hidden).
- **noise**: a `forbid` string appears.
- **bad exit**: the step ended with another exit status than it expects. Every `run` step, scored or not, must exit 0 unless it sets `expect_exit = N` (a validation error, a refused write). An absence check on a command that crashed measures nothing, so a crash fails the step.

A ranked hit is a numbered line that ends in its URI (`3. Title … [meta] <kx:id>`); numbered text
inside a snippet never counts. The total line sums every column.

Scenarios are defined in [`scenarios.toml`](scenarios.toml). A scenario marked `pending` is
reported but not asserted; when it passes, its result reads `pass (remove pending)` and the report
asks to remove the marker. Every other scenario must pass in `cargo test` (and CI, where the report
is the eval job's summary and artifact). Milliseconds come from a debug build and are only
comparable within one run.

## Retrieval quality (qrels)

When `qrels.tsv` and `queries.toml` exist here, `retrieval_quality` runs
`kontext search --json -n 10` for each query on its fixture and prints nDCG@5, recall@10 and MRR
per query and per slice (also `target/tmp/eval-retrieval.md`).

- `queries.toml`: `[[query]]` tables with `id`, `text`, optional `slice` (default `all`) and `fixture` (default `shop`).
- `qrels.tsv`: `query_id<TAB>doc_id<TAB>relevance`, or the TREC form `query_id<TAB>0<TAB>doc_id<TAB>relevance`. `doc_id` is an entry id or a hit URI (`kx:` is optional); relevance is graded (0 = not relevant). `#` lines and a header are skipped.
- `qrels-baseline.toml`: `[slice."<name>"]` with `ndcg5`, `recall10` and `mrr`. When it exists, a slice that falls below its baseline by more than the tolerance fails the test: 0.01 for `exact-identifier`, 0.02 for the others. Rising above it is reported, with a reminder to update the file. Without the file nothing is asserted. `KONTEXT_UPDATE_QRELS_BASELINE=1 cargo test --test eval retrieval_quality` writes it from the current run.

## History

[`baseline.md`](baseline.md) is the table measured on 0.2.1, before the knowledge-maintenance work.

| | scenarios passing | expectations found | wrongly suppressed | noise |
| --- | --- | --- | --- | --- |
| 0.2.1 ([baseline](baseline.md)) | 8 / 29 | 23 / 64 | 2 | 1 |
| 0.3.0 | 33 / 33 | 69 / 69 | 0 | 0 |

Four scenarios were added with the work they measure (two ranking checks for the phrase bonus,
the hub that is named in fallback mode, and a model answer about entries it was not shown).
Since 0.9.1, rank and token-budget checks count as expectations, so "expectations found" is not
comparable with the rows above.
