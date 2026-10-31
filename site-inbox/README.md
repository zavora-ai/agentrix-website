# agentrix-site-inbox

Phase 2 unified inbox: every pending human decision is one durable card in
SQLite — draft approvals, incidents, digests, generic approvals. A card is
decided exactly once (a second decision is refused); an approved card's effect
runs through the caller's executor and retries via `recover` until it succeeds,
so a crash between decision and effect loses neither. Cards name jobs by
charter role (`Content Editor`, never `freshness`).

The Content Editor target turns content-gap signals into held drafts and files
each draft's approval card. Draft-first is structural: the crate has no publish
path.

```sh
cargo test -p agentrix-site-inbox   # the Phase 2 gate
```
