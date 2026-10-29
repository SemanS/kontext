---
id: 2026-10-29-a-verify-command-is-code-from-the-repository-and-runs-only
kind: decision
title: A verify command is code from the repository and runs only after its text is pinned
status: accepted
date: 2026-10-29
summary: "A verify: command runs only when sha256(entry path, NUL, command) is pinned under [repos.<slug>.verify] in trust.toml by a person at a terminal; otherwise untrusted_verify returns the command and nothing runs. The runner scrubs the environment, redacts output and kills the process group. The pin binds consent to the text, not to the code the command runs."
tags: [security, verify, trust, ledger]
paths: [src/verification.rs, src/api_write.rs, src/config.rs, src/util.rs, src/tools.rs, src/cli.rs, src/mcp.rs, skills/**]
extends: [2026-10-29-trust-pins-are-sha-256-over-everything-shared-that-reaches, 2026-10-27-a-capture-names-the-command-that-proved-it-and-a-verification-says-where-it-landed]
verify: cargo test --test maintenance verify_refuses_unpinned_command
author: SemanS
origin: agent
---

## Context

`verify:` from any entry, committed or not, ran through `sh -c` with the full environment and no trust gate; `ctx_verify` was annotated non-destructive and Codex auto-approves tools after `kontext connect codex`. A scratch clone with `verify: echo PWNED-$(whoami) > pwned.txt` ran it (F7).

## Decision

- **Pin per command.** A verify runs only when `sha256(<entry path> NUL <command>)` is pinned in `trust.toml` under `[repos.<slug>.verify]`. Without a pin the operation returns `untrusted_verify` with the command text, its hash and the line `kontext trust --verify <id>`; nothing runs and nothing is appended to the ledger. Changing the command drops the pin.
- **Only a person pins.** `kontext trust --verify [<id>|--all]` works only from an interactive terminal and prints what it pins. No MCP tool and no `ctx_api` operation pins. The kontext-audit and kontext-verify skills show the command on `untrusted_verify` and stop; they never run `kontext trust`.
- **The runner is bounded.** Environment allowlist: PATH, HOME, USER, LANG, LC_*, TMPDIR, TERM, plus `[verify] pass_env` read from the user layer only. `output_tail` goes through `secrets::redact`. A dirty tree is recorded in the ledger line. After the shell exits stdout is read for at most 2 s, then the process group is killed. The MCP server kills a running verify when stdin closes.
- **Over MCP an unpinned command goes back to the agent.** When the command is unpinned or the tree is dirty, `ctx_verify` returns the command; the agent runs it in its own sandbox and the result is recorded as an `agent` stamp, never as `verify`. A `verify` ledger line is proof a person or CI ran.
- **CI runs the base revision only.** A future `kontext ci --verify-touched` runs the commands as written at the base revision (`git show <base>:<file>`), lists added or changed commands as "new, not run in CI", writes no ledger line and needs no local trust.

## What the pin does not cover

The pin binds consent to the text of the command, not to the code `cargo test` or `npm test` then runs from the tree. HOME stays readable and the network open. In Codex, `ctx_verify` runs inside the MCP server, outside the agent's sandbox. A command on the default branch of a trusted repository is not consent. OS sandboxes (sandbox-exec, bwrap, landlock) are not the gate.

## Alternatives

- **Treat trusting the adapters as trusting every verify**: rejected, adapters and entries are reviewed by different people at different times.
- **Pin the whole `.ai/` tree**: rejected, any new entry would need a re-pin and the prompt would teach people to approve blindly.
- **Claim pin plus allowlist closes the promise**: rejected, it does not; the scope above is part of the decision.

## Consequences

A cloned repository runs no verify command until a person reads and pins it. Peer's automatic checks receive `untrusted_verify` and must show it as "unverified, show the command" (migration note). Pinning is per command, so a team pins only the checks it uses.
