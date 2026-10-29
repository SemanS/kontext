---
id: freeze-prices-at-checkout
kind: decision
title: Freeze prices at checkout
status: accepted
date: 2026-01-28
paths: ["src/checkout"]
---

## Decision

Prices are copied into the cart when checkout starts and never re-read.
