---
id: one-currency-per-cart
kind: decision
title: One currency per cart
status: accepted
date: 2026-01-12
paths: ["src/checkout/**"]
---

## Decision

A cart holds one currency; mixing currencies is an error, not a conversion.
