---
id: 2025-06-01-export-orders-nightly-with-cron
kind: decision
title: Export orders nightly with cron
status: accepted
date: 2025-06-01
paths: [src/exports/**]
---

## Decision

A cron job exports the day's orders as CSV at 02:00.
