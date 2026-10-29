---
id: release-gates-and-red-first-regression-tests
kind: convention
title: Release gates and red-first regression tests
date: 2026-10-29
summary: "A tag publishes only after fmt, clippy, tests on Linux and macOS, MSRV, docs build, EN/SK parity, kontext ci over this store, the API snapshot and Peer's suites pass; the first commit of a fix is a failing test; eval expectations are written by a different session than the implementation."
tags: [release, ci, tests, eval]
paths: [.github/workflows/**, CONTRIBUTING.md, CONTRIBUTING_SK.md, RELEASE.md, tests/**, scripts/**]
author: SemanS
origin: agent
---

A tag publishes only after every gate passes: `cargo fmt --check`, clippy, the test suite on Linux and macOS, the MSRV job reading `rust-version`, the docs build, the `docs/en` ↔ `docs/sk` parity test, `kontext ci` over this repository's `.ai/`, the API response snapshot (`tests/api_contract.rs`) and Peer's integration suites against the candidate (`scripts/release-check.sh`). The tag must match `Cargo.toml`. The first commit of a fix is a failing test, and its output is in the pull request. Eval expectations and qrels labels are written by a different session than the one implementing the change. There is no cadence rule: the pace of releases is the maintainer's; the gates keep it safe.
