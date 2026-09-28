---
id: 2026-09-28-dual-license-under-mit-or-apache-2-0
kind: decision
title: Dual-license under MIT OR Apache-2.0
status: accepted
date: 2026-09-28
paths: [LICENSE-MIT, LICENSE-APACHE, Cargo.toml]
author: SemanS
origin: cli
---

## Context
kontext is meant to be freely distributable and adoptable by companies and their clients, including in proprietary codebases. Copyleft licenses (e.g. AGPL-3.0) deter that kind of use.

## Decision
The project is dual-licensed under MIT OR Apache-2.0 at the user's option, the Rust ecosystem convention. Contributions are accepted under the same terms.

## Consequences
Anyone may use, modify and redistribute kontext, commercially too, keeping the copyright notices; Apache-2.0 adds an explicit patent grant. Relicensing later would need every contributor's consent.
