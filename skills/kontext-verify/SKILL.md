---
name: kontext-verify
description: Give knowledge entries an executable check - a `verify` command that proves a learning or decision still holds, so `kontext verify` can re-run it instead of someone re-reading. Use when asked to make the knowledge verifiable, add verify commands, or turn confirmations into checks.
---

# Make the knowledge verifiable

A confirmation says a person read an entry at a commit. A `verify` command says a machine can show the claim still holds, with no false alarms: it checks the role-bearing assumption (a pinned version, a path that must exist, a test that encodes the rule, a flag's default), not a value that merely appears in the text. One line, run with `sh` from the repository root; exit 0 means the claim holds.

## 1. Pick the entries that can carry a check

```sh
kontext api entries.list '{"v":1}'    # `verify: null` and kind decision | convention | learning
```

Good candidates, in order: learnings and pitfalls that name a command, a test or an error; decisions that fix a default, a limit or a layout (a file must exist, a dependency must be pinned, a test must pass); conventions a linter or a grep can enforce. Skip entries whose claim is a judgement ("we prefer …") or a history; a check that always passes proves nothing.

## 2. Write the check from the claim, then run it

For each entry (`ctx_read kx:<id>` level 2), write the one command that would fail if the claim stopped holding:

- a test: `cargo test -q --lib scheduler::refuses_local_inference`
- a pin: `grep -q 'onnxruntime = "=1.22' Cargo.toml`
- a layout: `test -f apps/server/src/inference_policy.rs`
- a guard: `! grep -rn 'unwrap()' src/api/ --include='*.rs' | grep -v test`

Keep it under 500 characters, deterministic, with no network and nothing it writes. Run it by hand first: it must pass now and it must be clear what change would make it fail.

## 3. Record it

- An entry already in the store: add `verify: "<command>"` to its front matter (quote it), then `kontext check` and `kontext verify <id> --because "first run"`. The outcome lands in the ledger and the health view reads *verified by its check*.
- A draft in the inbox: `kontext inbox verify <inbox-id> "<command>"`; promotion keeps it.
- A new capture: `kontext capture … --verify "<command>"` (`ctx_capture` with `verify`).

Edit only the `verify` line of an existing entry; a claim that needs rewording is a superseding entry, not an edit.

## 4. Hand over

Report which entries got a check and which could not (and why), and what `kontext verify` printed. The edits are staged; the person reviews and commits them. Teams can then run `kontext verify <ids>` or `kontext drift --fail` in their check script, so a claim that stops holding fails a build instead of waiting for a reader.
