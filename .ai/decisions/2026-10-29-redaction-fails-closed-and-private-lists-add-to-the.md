---
id: 2026-10-29-redaction-fails-closed-and-private-lists-add-to-the
kind: decision
title: Redaction fails closed, and private lists add to the repository's
status: accepted
date: 2026-10-29
summary: "A redaction pattern that does not compile is an error in status and check, and distill, threads and autopilot refuse to send anything to a model; private redact and allow lists are merged with the repository's, never replace them; redaction covers whole PEM blocks, national phone numbers, IPv6 and every match on a line."
tags: [security, secrets, redaction]
paths: [src/secrets.rs, src/config.rs, src/distill.rs, src/threads.rs, src/adapters/mod.rs]
verify: cargo test --lib distill_refuses_invalid_redact_pattern
author: SemanS
origin: agent
---

## Decision

- **Fail closed.** A redaction pattern that does not compile is reported by `kontext status` and `kontext check`; distill, threads and autopilot refuse to send text to a model until it is fixed.
- **Private lists add.** Private `redact` and `allow` lists are merged with the repository's lists; they never replace them.
- **The promise is kept as written.** Redaction masks whole PEM blocks, national phone numbers (separators, a minimum digit count), IPv6 addresses and every match on a line, not only the first.
- Adapter error messages drop query strings and userinfo and pass through redaction. Bash-mode outputs and compaction summaries from Claude threads are left out of distill input.

## Alternatives

- **Skip an invalid pattern with a warning**: rejected, a typo would silently send the secret it was meant to mask.

## Consequences

A broken config stops model calls instead of leaking. A personal list can tighten the repository's redaction but cannot loosen it by replacement.
