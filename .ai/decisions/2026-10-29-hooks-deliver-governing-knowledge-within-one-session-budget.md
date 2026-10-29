---
id: 2026-10-29-hooks-deliver-governing-knowledge-within-one-session-budget
kind: decision
title: Hooks deliver governing knowledge within one session budget and stay opt-in until the A/B report
status: accepted
date: 2026-10-29
summary: "kontext hook --host claude|codex|cursor gives the brief at session start and the governing section for a file before an edit, from committed accepted entries only, inside the one session_budget_tokens shared with ctx_brief; fail-open, never blocks, never captures, never calls a model; installed only by connect --hooks until an A/B report says otherwise."
tags: [hooks, brief, budget, platforms]
paths: [src/hooks.rs, src/connect.rs, src/ops.rs, src/config.rs, src/cli.rs]
author: SemanS
origin: agent
---

## Decision

- **What a hook delivers.** `kontext hook --host claude|codex|cursor` reads the host's hook JSON from stdin and writes the host's shape. SessionStart (startup, resume, clear, compact) and SubagentStart get the brief. PreToolUse on Edit, Write and MultiEdit (Codex apply_patch, Cursor postToolUse) gets the governing section for `tool_input.file_path`: only entries not yet told in this session or whose blob changed. Session state lives in `.git/kontext/sessions/<id>.json`.
- **One budget.** `session_budget_tokens` is the one budget shared with `ctx_brief`, set in one place; hooks and adapters get no budget of their own. Host limits apply (10 000 characters in Claude Code, about 2 500 tokens in Codex).
- **Only facts from reviewed entries.** Committed accepted entries only. An entry with a lint finding (imperatives aimed at the agent, `curl | sh`, verify lines in the body) is given by reference only.
- **Fail-open and switchable.** Exit 0 with no output on any error; target 150 ms on a 250-entry store; kill switch `KONTEXT_HOOKS=off` and `[hooks] enabled = false`. An optional Stop hook only adds context, never blocks.
- **Opt-in.** Installed only by `kontext connect --hooks`. Default-on waits for the A/B report.

## Alternatives

- **A separate budget for hooks**: rejected, unselected context lowers agent success; one budget keeps the total honest.
- **Default-on hooks now**: rejected until measured on outcomes, not retrieval.
- **Hooks that block an edit or capture automatically**: rejected, hooks inform; review stays the gate.

## Consequences

Governing knowledge reaches the agent at the moment it touches a path, which is where provenance changes behaviour. A broken hook never breaks the agent's session.
