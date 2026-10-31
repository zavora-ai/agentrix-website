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
