# Agentrix Website — Execution Record

Status: Phase 0 done. The normative spec is
[`.kiro/specs/agentrix-website/](../.kiro/specs/agentrix-website/)
(requirements, design, tasks); this file records execution: findings, upstream
issues, phase gates and risks. Each phase closes with its named validation and
a commit.

## Phase 0 findings (verified 2026-10-30)

| Check | Result |
|---|---|
| Site triggers registered in serve glue | Yes — `adk-website/src/deploy/serve.rs:197` extends `site.triggers()` |
| Approvals durable in served deployment | Yes — `deploy/kernel.rs:102` `SqliteApprovalStore` + `StoreApprovalService` |
| `KpiTracker` trigger registered | **Not found** in `adk-website/src` (only UI/measurement mentions) — KPI pause/demote path unverified; treated as a gap until wired |
| Charter stores on Mongo/Firestore/Neo4j | Memory/SQLite/Postgres/Redis only (minor) |
| Silent no-model job loss | Real — `serve.rs:222` gates 4 of 7 jobs on model+sessions; our host must self-check and report active jobs at startup |

## Upstream gaps (issues filed on zavora-ai/adk-rust)

See [ISSUES.md](ISSUES.md) for numbers. Eight feature requests:
managed signal wakes, token/media quotas, one-call charter wiring,
RAG citation propagation, durable harness approvals/drafts, sandbox
egress enforcement, Muse realtime/streaming clients, visitor
principal + cross-store erasure.

## Phase gates

| Phase | Gate | Result |
|---|---|---|
| 0 Foundation | Findings reproduce; `cargo test -p agentrix-site-host` green twice | Pass (`bf4a369`) |
| 1 Read-only dual site | Digest match human vs twin | Pass (see commit) |
| 2 Draft-first content | Approval survives restart | Pass (see commit) |
| 3 Publish + rollback | Rollback to prior digest | Pass (see commit) |
| 4 Concierge + signals | Zero cross-site chunks | Pass (see commit) |
| 5 Email + support | Tier-above send impossible unapproved | Pass (see commit) |
| 6 Campaigns + social | Cap halts spend; breach demotes | — |
| 7 Commerce | 202 → approve → complete, idempotent | — |
| 8 Self-defense + costs | Incident card + rollback drill | — |
| 9 Migration | Parity ≥0.95 gates go-live; below refuses | Pass (see commit) |

## Risks carried as gaps

KpiTracker wiring (Phase 0), DurableRunStore guarantees, LaunchGate /
AwpEdge internals effort, live Meta shapes for SAM/realtime, live-driver
breadth (Google reviews, Instagram, SPA crawling). Each names its phase
gate; none is assumed away.
