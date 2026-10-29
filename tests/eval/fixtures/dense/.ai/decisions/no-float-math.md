---
id: no-float-math
kind: decision
title: No floating point in checkout
status: accepted
date: 2026-03-02
paths: ["src/checkout/**"]
---

## Decision

Checkout code uses integers and checked arithmetic; f32 and f64 are rejected in review.
