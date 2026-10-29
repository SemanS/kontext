---
id: totals-are-pure
kind: decision
title: Total functions are pure
status: accepted
date: 2026-03-10
paths: ["src/*/total.rs"]
---

## Decision

A total function reads its inputs and returns a value; it never writes or calls out.
