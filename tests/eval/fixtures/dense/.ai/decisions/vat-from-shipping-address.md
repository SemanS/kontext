---
id: vat-from-shipping-address
kind: decision
title: VAT follows the shipping address
status: accepted
date: 2026-02-25
paths: ["src/checkout/total.rs"]
---

## Decision

VAT is chosen from the shipping address, not the billing address.
