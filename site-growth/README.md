# agentrix-site-growth

Phase 6 campaigns plus social: the budget ledger refuses past the cap before
anything is sent (reserve → commit or release, no direct spend); KPI readings
breach against directional targets and demote the breached area's jobs one
ladder step; the Sales Rep drafts held nurture follow-ups; the Reputation
Manager posts replies through metered reservations and holds at the cap; ads
record with zero spend and no spend path; the Analyst files the weekly review —
spend, indicators, breaches, ladder — as a digest card.

```sh
cargo test -p agentrix-site-growth   # the Phase 6 gate
```

Live posters: Instagram is primary (`META_IG_USER_ID`, `META_IG_ACCESS_TOKEN`
with `instagram_business_content_publish`, `META_IG_IMAGE_URL` brand image);
Mastodon is the alternative (`MASTODON_INSTANCE`, `MASTODON_ACCESS_TOKEN`).
Ignored live tests post for real — run with credentials set:

```sh
cargo test -p agentrix-site-growth -- --ignored
```
