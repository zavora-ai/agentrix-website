# Agentrix Website — Implementation Plan

Status: Phase 0 in progress. This is the execution record for the plan
approved 2026-10-30. Each phase closes with its named validation and a
commit.

## Spec

**Goals.** G1 Governed autonomous site: 7 charter jobs
(Probe/Watch/Content/Reviews/Outreach/Support/Digest) wake on
schedule/signal under the autonomy ladder and KPI guardrails. G2
Dual-audience serving (humans + AWP agents, same signed release). G3
Owner control: one inbox, budgets (USD/tokens/media), quiet hours,
freeze, rollback, weekly Digest. G4 Credential-free migration with
parity gate. G5 Multi-site tenant isolation.

**Non-goals (v1).** Spatial/3D serving (concepts stay demo surfaces);
live ad spend and paid social (Mastodon live, ads recorded/labeled);
JS/SPA generation; hand-rolled fleet autoscaler.

**SLOs.** Probe wake→run ≤60s p95, model-free · Concierge first token
≤2s p95 · zero cross-site retrieval/approval leaks · cost reserved
before calls · held approvals survive restart · migration parity ≥0.95.

## Design decisions

- **Tenancy**: one TenantKernel per site (`adk-kernel` managed feature);
  per-site durable stores; fleet view is a product aggregator in v1.
- **Hosting**: features `charter, channels, managed, proc` + Muse;
  explicit durable stores; startup fails loudly if any store is
  InMemory or any charter job is absent.
- **Executors are product work**: kernel starts governed runs; Zavora
  writes the Runner targets, KPI sources, signal emitters, inbox UX.
- **Extension before forking**: `WakeSource` trait, `Wake::Signal`
  free-form kinds, `Guardrail` impls, `Document.metadata` citations and
  visitor-UUID principals ship as product code; only the narrow gaps
  below go upstream.

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

## Phases and validation

| Phase | Ships | Closes with |
|---|---|---|
| 0 Foundation | This plan, issues, host self-check | Findings table above; restart durability proof |
| 1 Read-only dual site | Generator site + twins + AWP edge | Digest match human vs twin |
| 2 Draft-first content | Held drafts + unified inbox | Approval survives restart |
| 3 Publish + rollback | Signed releases via LaunchGate | Rollback to prior digest |
| 4 Concierge + signals | Visitor principal, grounded answers | Zero cross-site chunks |
| 5 Email + support | Live mail, held replies | Tier-above send impossible unapproved |
| 6 Campaigns + social | Outreach, Mastodon, KPI digest | Cap halts spend; breach demotes |
| 7 Commerce | Held payments via AWP | 202 → approve → complete, idempotent |
| 8 Self-defense + costs | Log/KPI/stock wakes, reversible acts | Incident card + rollback drill |

## Risks carried as gaps

KpiTracker wiring (Phase 0), DurableRunStore guarantees, LaunchGate /
AwpEdge internals effort, live Meta shapes for SAM/realtime, live-driver
breadth (Google reviews, Instagram, SPA crawling). Each names its phase
gate; none is assumed away.
