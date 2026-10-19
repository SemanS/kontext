---
id: 2026-01-10-orders-live-in-postgresql
kind: decision
title: Orders live in PostgreSQL
status: accepted
date: 2026-01-10
paths: [src/orders/**]
---

## Context

An order touches items, payments and stock in one step.

## Decision

Orders live in one PostgreSQL database; an order and its items are written in one transaction.
