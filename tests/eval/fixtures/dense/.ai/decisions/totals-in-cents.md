---
id: totals-in-cents
kind: decision
title: Cart totals are integer cents
status: accepted
date: 2026-01-05
paths: ["src/checkout/total.rs"]
---

## Decision

A cart total is an i64 count of cents from the first line item to the receipt.
