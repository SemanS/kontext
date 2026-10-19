---
id: 2026-02-20-round-prices-half-even-on-invoices
kind: decision
title: Round prices half-even on invoices
status: accepted
date: 2026-02-20
paths: [src/pricing/**]
---

## Decision

Invoices round every line total half-even to cents, because the accounting export sums lines.
