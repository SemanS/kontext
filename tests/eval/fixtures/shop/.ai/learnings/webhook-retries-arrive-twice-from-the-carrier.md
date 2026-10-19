---
id: webhook-retries-arrive-twice-from-the-carrier
kind: learning
title: Webhook retries arrive twice
summary: The payment provider sends the same webhook again when our answer takes longer than ten seconds, and the shipping carrier does the same for tracking events, which carry no event id at all.
paths: [src/orders/**]
---

The payment provider sends the same webhook again when our answer takes longer than ten seconds, and the shipping carrier does the same for tracking events, which carry no event id at all.
