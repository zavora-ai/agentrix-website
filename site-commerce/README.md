# agentrix-site-commerce

Phase 7 held payments: `propose` records the intent and files its approval card
(accepted, not done); the owner approves through the inbox; `complete` verifies
an approved card naming the intent, reserves the 3% provider fee against exactly
that site's budget, charges, and commits — releasing the fee and marking the
intent failed on provider error for a later retry. One idempotency key proposes
one intent; completing twice charges once.

```sh
cargo test -p agentrix-site-commerce   # the Phase 7 gate
```

Live charges run through Stripe PaymentIntents (`STRIPE_SECRET_KEY` — test-mode
for validation; the intent carries the `payment_method`). Micro-USD converts to
minor units at charge time, refusing below one cent. The ignored live test
creates a real test charge with `STRIPE_TEST_PM`:

```sh
cargo test -p agentrix-site-commerce -- --ignored
```
