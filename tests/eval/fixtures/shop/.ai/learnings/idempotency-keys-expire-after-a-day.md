---
id: idempotency-keys-expire-after-a-day
kind: learning
title: Idempotency keys expire after a day
paths: [src/payments/**]
---

The provider forgets an idempotency key after 24 hours. A checkout retried the next day is a new attempt with a new key; keys per attempt, attempts per checkout, checkout per key never cross a day.
