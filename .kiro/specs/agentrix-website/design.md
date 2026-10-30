# Design — Agentrix Website

## Overview

One site is one TenantKernel over one SQLite database, woken by seven standing
triggers plus a nightly KPI trigger. The kernel starts governed runs; Zavora
product code provides the Runner targets, KPI sources, signal emitters and the
inbox UX behind them. An AWP edge serves humans, agent twins and a public CLI
from signed releases admitted by the LaunchGate. Migration builds candidates
from public evidence and admits them only past the parity gate. The host
self-check (shipped in Phase 0) fails startup loudly on any missing store,
missing trigger or unfiled probe signal.

## Architecture

```mermaid
flowchart TD
    subgraph HOST[site-host · one process per site]
      SC[self-check] --> K[TenantKernel]
      ST[(SQLite · runs, approvals, agenda, signals, charters, KPIs, watermarks)] <--> K
      TR[8 standing triggers] --> K
      KPS[KPI tracker · nightly] --> K
    end
    subgraph RUNS[runs · kernel starts, product executes]
      K --> PR[Security Guard · model-free]
      K --> WA[Facilities Manager · model-free]
      K --> CO[Content Editor · Growth]
      K --> RE[Reputation Manager · Growth]
      K --> OU[Sales Rep · Revenue]
      K --> SU[Chief of Staff · Steward]
      K --> DI[Analyst · Steward]
      LAD[autonomy ladder] -.- RUNS
      KPIBreach[KPI breach] -.->|pause/demote| LAD
    end
    subgraph EDGE[AWP edge · signed releases]
      REL[(signed releases)] --> H[human pages]
      REL --> TW[agent twins]
      REL --> CLI[public CLI]
      LG[LaunchGate] --> REL
    end
    subgraph OWNER[owner control]
      IB[inbox · decision cards] <--> K
      FR[freeze] -.-> K
      RB[rollback] -.-> REL
      QH[quiet hours] -.-> K
    end
    subgraph MIG[migration]
      CR[crawl public evidence] --> CAND[candidate site]
      CAND --> PAR{parity ≥ 0.95?}
      PAR -->|yes| LG
      PAR -->|no| FIX[fix and re-score]
      FIX --> CAND
    end
    SIG[signals · gaps, reviews, leads, escalations] --> K
```

The wake flow: a cron expression or a filed signal wakes the kernel; the kernel
claims a run on the job's queue; the ladder checks the job's level; the product
Runner target executes; effects at or below the level commit, anything above
files a decision card to the inbox.

## Components

| Component | Owns | Upstream |
|---|---|---|
| `site-host` | Store opening, trigger registration, self-check, kernel build | `adk-kernel`, `adk-session` |
| TenantKernel wiring | Partitions, supervisors, wake router, control plane | `adk-kernel` managed feature |
| Runner targets | Security Guard, Facilities Manager, Content Editor, Reputation Manager, Sales Rep, Chief of Staff, Analyst executors | Product code (new) |
| KPI sources | Commerce, beacon, reviews, inbox metrics feeding `KpiTracker` | Product code (new) |
| Signal emitters | Gap detector, review watcher, lead capture, escalation classifier | Product code (new) |
| AWP edge | Human pages, twins, CLI, rate limits, capability policy | `adk-website` edge + product twins |
| Generator | Candidate sites from templates and migrated evidence | `adk-website` kinds + product code |
| Inbox | Decision cards, approvals, drafts, incidents, digest UX | Product code (new) |
| Migration | Crawler, evidence log, parity scorer, go-live card | Product code (new) |
| LaunchGate | Release admission (approvals, parity, policy) | `adk-website` + product checks |

Extension before forking: `WakeSource` implementations, free-form `Wake::Signal`
kinds, `Guardrail` implementations, `Document.metadata` citations and
visitor-UUID principals ship as product code. The eight narrow gaps that need
framework support are tracked as upstream issues in `docs/ISSUES.md`.

## Data model

- **Stores** (one SQLite database): runs, approvals, agenda, partitions, cron
  watermarks, signals, charters, KPIs. No in-memory store in served
  deployments (Requirement 6.4).
- **Release**: immutable bundle `{ digest, pages, twins, provenance, charter }`;
  the edge serves only admitted digests; rollback names a prior digest.
- **Decision card**: `{ id, site, kind, payload, state, created, decided }`;
  states open, approved, rejected, expired; exactly-once execution on decision.
- **Budget**: `{ site, usd, tokens, media }` with reservation before metered
  calls and commit or release after.
- **Parity report**: `{ content, links, products, metadata, overall }` scored
  0.0–1.0 per axis; go-live needs overall 0.95 or above.

## Correctness properties

- **P1 (1.1, 7.3).** For any startup, the registered trigger set includes all
  seven site processes, or startup exits nonzero naming the missing kind.
- **P2 (1.3).** For any job action above the job's ladder level, no effect
  commits and a decision card exists afterwards.
- **P3 (1.4).** For any breached KPI area, the area's jobs are paused or demoted
  before their next wake fires.
- **P4 (2.3).** For any release, every twin carries the same facts as its human
  page; the digest-match check compares them field by field.
- **P5 (3.2).** For any decided card, its effect executes exactly once across
  restarts (idempotency keys on execution).
- **P6 (3.3, 5.5).** For any metered call, a reservation on exactly one site's
  budget precedes the call, and a call with no remaining budget is refused.
- **P7 (4.3).** For any candidate below parity 0.95, no LaunchGate admission
  exists.
- **P8 (5.2, 5.3).** For any cross-site read or approval attempt, the request is
  denied and an audit record exists.
- **P9 (6.2, 6.3).** For any restart, every held approval reopens and every
  missed cron tick fires exactly once.
- **P10 (8.1).** For any probe wake, claim latency is model-free and within 60
  seconds at p95.

## Error handling

- Store open or migration failures abort startup with the store name and the
  database error (no fallback to memory).
- Runner target failures park the run with its error; the supervisor retries per
  the run's policy, then files an incident card.
- Budget exhaustion refuses the call and files a top-up card; in-flight
  reservations release on failure.
- Twin generation failures serve the last good twin with a staleness header and
  file a regeneration card; human pages never block on twins.
- Migration evidence gaps cap the affected parity axis and are listed in the
  report; silence never scores.

## Testing strategy

- **Unit**: ladder checks, budget arithmetic, parity scoring, card state
  machine, twin field projection.
- **Property**: P2 (arbitrary actions vs levels), P5 (decision replay),
  P6 (interleaved reservations), P8 (cross-site fuzz over stores).
- **Integration**: self-check green twice (Phase 0 gate); digest match human vs
  twin; approval survives restart; rollback serves prior digest; tier-above send
  refused unapproved; cap halts spend; incident card plus rollback drill.
- **Load**: probe wake-to-claim p95; concierge first-token p95; twin serve p95.
