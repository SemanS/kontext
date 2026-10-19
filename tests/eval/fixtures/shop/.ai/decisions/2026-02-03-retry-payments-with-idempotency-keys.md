---
id: 2026-02-03-retry-payments-with-idempotency-keys
kind: decision
title: Retry payments with idempotency keys
status: accepted
date: 2026-02-03
paths: [src/payments/**]
---

## Decision

Send one idempotency key per checkout attempt to the payment provider, so a retried request never charges twice.
