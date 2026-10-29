---
id: 2026-10-29-trust-pins-are-sha-256-over-everything-shared-that-reaches
kind: decision
title: Trust pins are SHA-256 over everything shared that reaches an adapter, and only a person at a terminal pins
status: accepted
date: 2026-10-29
summary: "kontext trust pins sha256 over the canonical TOML of the shared adapters, vars and project.name; a legacy FNV pin is refused, never upgraded; shared vars never override user or local adapter vars; project.name must be a slug; trust runs only from a TTY and connect denies it to agents."
tags: [security, trust, adapters]
paths: [src/config.rs, src/cli.rs, src/connect.rs, src/adapters/mod.rs]
supersedes: [2026-09-28-repository-declared-adapters-run-only-after-kontext-trust]
verify: cargo test --test maintenance trust_gate_skips_untrusted_and_revokes_on_edit
author: SemanS
origin: agent
---

## Context

The 2026-09-28 pin was a 12-hex (48-bit) FNV-1a hash of the adapters table text alone (F12). The shared `[vars]` and `[project] name` were not in it and still flowed into trusted personal adapters (F18). Anything that could run a shell could also run `kontext trust`.

## Decision

- **What is pinned.** `adapters_hash` is `sha256:<hex>` over the canonical TOML of the shared layer's `adapters`, `vars` and `project.name`, stored in `~/.config/kontext/trust.toml`. Any change to them revokes trust until it is given again. User-level and clone-local configs stay trusted.
- **A legacy pin is refused.** A 12-hex FNV pin is rejected with a message that says to run `kontext trust` again. It is never upgraded silently: an upgrade would legalise a forged pin.
- **Shared values never reach personal adapters.** Shared `[vars]` never override the vars of an adapter declared in the user or local layer. A shared `project.name` must be a slug. `{{repo.remote}}` carries no userinfo.
- **Only a person at a terminal pins.** `kontext trust` refuses without an interactive terminal and prints what it pins. `kontext connect` installs a host deny rule (Claude Code and Codex PreToolUse on Bash) for `kontext trust` run by an agent.
- **Stated limits.** Against a process that can edit `trust.toml` or allocate a PTY (Codex and Cursor can), the gate is honour-based. Trust covers the text of the table, not the scripts it runs from the repository: a trusted `command = ["sh", "tools/x.sh"]` keeps its trust when `tools/x.sh` changes (F99). The security docs say both.

## Alternatives

- **Upgrade a matching FNV pin to SHA-256 on first read**: rejected, a colliding forged pin would become a valid one.
- **Hash the scripts an adapter runs**: rejected for now, the set of files a command reads is open; the scope is documented instead.
- **Keep the TTY check as the whole gate**: rejected, it is not proof of a person; the host deny rule is the second line.

## Consequences

Every teammate re-trusts once after the upgrade. A repository can no longer steer a personal adapter through vars or the project name. The docs stop promising more than the gate holds.
