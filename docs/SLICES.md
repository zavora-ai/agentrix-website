# Build Slices — End-State Intact

Every slice uses the same contracts (kernel Runner, governed capability
path, LadderPolicy/AutonomyStore, ReleaseManifest/SiteLog). Later slices
add executors; none bypass holds, Known-fact gating, or credential
tiers. Each slice is demoable the day it lands.

| Slice | Ships | Demo beat unlocked |
|---|---|---|
| 0 Foundation | Features + durable stores + scheduler + tiers + freeze proof | Restart durability; kill switch |
| 1 Read-only dual site | Generator site + twins + Verifier + public AwpEdge | Beat 4 digest match (read-only) |
| 2 Draft-first content | Content/Reviews jobs → drafts + gate + inbox | Beat 3 draft approval |
| 3 Publish + rollback | Pipeline publish/delta + SiteLog + first-publish hold | Beat 1 publish moment |
| 4 Concierge + signals | PublicTools + RAG ingest + emit_signal → Digest | Beat 2 |
| 5 Email/support | IMAP/SMTP + escalation wake + held replies (+ async ASR drafts) | Beat 3 mail |
| 6 Campaigns/social | Outreach + Mastodon + RecordedAds (labeled) + KPI digest | Campaign card |
| 7 Commerce/bookings | Quote/Order/Payment/Booking with PaymentDesk/Calendar | Beat 4 held payment |
| 8 Self-defense + cost ops | Log ingestion + detector + reversible playbook + spend alerts | Beat 3 incident |
| 9 Live adapters | Live ads, realtime ASR WS, SAM — when built, never mocked | — |

Conformance per slice: AWP S1–S9/E1–E13 (GET-only limits stated),
host `run_all`, driver contracts, kernel K2/K3/K9, website ladder tests.
