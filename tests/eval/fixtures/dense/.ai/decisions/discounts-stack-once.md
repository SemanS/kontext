---
id: discounts-stack-once
kind: decision
title: Discounts stack once
status: accepted
date: 2026-02-02
paths: ["src/checkout/*.rs"]
---

## Decision

Each discount applies at most once per cart, in the order the promotion table lists.
