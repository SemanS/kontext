---
id: webhook-retries-arrive-twice-from-the-provider
kind: learning
title: Webhook retries arrive twice
summary: The payment provider sends the same webhook again when our answer takes longer than ten seconds, so the handler must ignore an event id it has already processed in the payments table.
paths: [src/payments/**]
---

The payment provider sends the same webhook again when our answer takes longer than ten seconds, so the handler must ignore an event id it has already processed in the payments table.
