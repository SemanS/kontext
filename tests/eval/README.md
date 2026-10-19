# Evaluation set

Real situations an agent meets, run against the `kontext` binary on a fixture repository
(`fixtures/shop`: a small web shop with decisions, conventions, learnings and an incident).
Each scenario is scored as found / missed / wrongly suppressed, plus noise (what must not
appear), output tokens (characters / 4) and wall-clock milliseconds.

```sh
cargo test --test eval -- --nocapture                  # the table; also target/tmp/eval-report.md
KONTEXT_EVAL_ONLY="exact phrase" cargo test --test eval -- --nocapture
KONTEXT_EVAL_VERBOSE=1 …                               # print the output of a step that missed
```

- **found**: an `expect` or `keep` string appears.
- **missed**: an `expect` string is absent, a ranked hit is below its rank, or the output is over its token limit.
- **wrongly suppressed**: a `keep` string is absent although its source exists (a hit dropped by deduplication, a candidate hidden).
- **noise**: a `forbid` string appears.

Scenarios are defined in [`scenarios.toml`](scenarios.toml). A scenario marked `pending` is
reported but not asserted; every other scenario must pass in `cargo test` (and CI).
Milliseconds come from a debug build on a developer machine and are only comparable within one run.

[`baseline.md`](baseline.md) is the table measured on 0.2.1, before the knowledge-maintenance work.
