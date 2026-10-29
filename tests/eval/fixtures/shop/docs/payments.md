# Payments

The checkout charges a card through the payment provider and records the charge in the payments table.

## Retrying payments

When a payment request times out, the checkout retries it. To retry payments safely it sends the same idempotency key again: retry payments with idempotency keys, never without them, or a retried payment request can charge the card twice. Payments are retried up to three times, a few seconds apart, and a payment that still fails is shown to the customer as declined.
