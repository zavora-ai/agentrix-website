# Implementation Plan: Agentrix Website

## Overview

Nine phases, each independently shippable and each closing with its named
validation and a commit. Phase 0 (foundation) is done: plan, upstream issues
and the host self-check. Phases build in dependency order; the wave graph marks
where work can fan out.

## Task Dependency Graph

```json
{
  "waves": [
    { "wave": 1, "tasks": ["1"] },
    { "wave": 2, "tasks": ["2"] },
    { "wave": 3, "tasks": ["3", "4"] },
    { "wave": 4, "tasks": ["5"] },
    { "wave": 5, "tasks": ["6", "7"] },
    { "wave": 6, "tasks": ["8"] },
    { "wave": 7, "tasks": ["9"] }
  ]
}
```

- Wave 1: foundation (done).
- Wave 2: read-only dual site; everything else serves from it.
- Wave 3: drafts/inbox and publish/rollback are independent once releases exist.
- Wave 4: concierge needs the inbox and published twins.
- Wave 5: email/support and campaigns/social fan out from the concierge.
- Wave 6: commerce needs budgets, approvals and the LaunchGate proven.
- Wave 7: self-defense closes over the full loop.

## Tasks

- [x] 1. Phase 0: foundation
  - Write this spec (requirements, design, tasks) and the Phase 0 findings.
  - File the eight upstream gap issues on `zavora-ai/adk-rust`.
  - Ship `site-host` with eight durable stores, eight triggers and the
    fail-loud self-check.
  - Checkpoint: findings reproduce against source; `cargo test -p
    agentrix-site-host` green twice (restart durability proof).
  - _Requirements: 6.1, 6.4, 7.1, 7.2, 7.3, 7.4_

- [ ] 2. Phase 1: read-only dual site
  - Build the generator: template kinds to pages plus agent twins from one
    release bundle.
  - Serve human pages, twins and the public CLI from the AWP edge with rate
    limits and the anonymous capability policy.
  - Add the digest-match check comparing twin facts to human page facts.
  - Checkpoint: digest match passes on a generated business site; twins serve
    within budget.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 8.3_

- [ ] 3. Phase 2: draft-first content
  - Implement the Content runner target: gap signals to held drafts, never
    direct publish.
  - Build the unified inbox: decision cards for approvals, drafts, incidents
    and digests with exactly-once execution.
  - Checkpoint: file a draft, restart the host, approve the card; the draft
    applies once.
  - _Requirements: 1.2, 3.1, 3.2, 6.2_

- [ ] 4. Phase 3: publish plus rollback
  - Wire the LaunchGate: release admission on approvals, parity and policy.
  - Implement rollback to a named prior release for pages and twins.
  - Checkpoint: publish release N+1, roll back to N; both audiences serve N.
  - _Requirements: 3.5_

- [ ] 5. Phase 4: concierge plus signals
  - Implement the Concierge runner target with visitor-UUID principals and
    grounded answers with citations.
  - Add signal emitters: content gaps, review sightings, lead capture and
    support escalations.
  - Enforce per-site retrieval scoping across every store.
  - Checkpoint: zero cross-site chunks under a two-site fuzz run; first token
    within budget.
  - _Requirements: 1.2, 4.5, 5.2, 8.2_

- [ ] 6. Phase 5: email plus support
  - Connect live mailboxes; the Support target drafts held replies for
    owner-questions.
  - Enforce tier checks: a tier-above send without approval is impossible, not
    merely unapproved.
  - Checkpoint: attempt an unapproved tier-above send; it is refused and
    carded.
  - _Requirements: 1.3, 3.1_

- [ ] 7. Phase 6: campaigns plus social
  - Implement the Outreach target and the Reviews target with live Mastodon
    posting and recorded (never spent) ads.
  - Wire KPI sources and breach policy: caps halt spend, breaches demote jobs.
  - Implement the Steward's weekly Digest as a filed decision card.
  - Checkpoint: breach a test KPI; the area's jobs demote before the next
    wake; spend halts at cap.
  - _Requirements: 1.4, 3.3, 3.6_

- [ ] 8. Phase 7: commerce
  - Implement held payments over AWP: 202 accepted, owner approval, completion,
    idempotent throughout.
  - Attribute every metered call to exactly one site's budget with
    reservation-before-call.
  - Checkpoint: 202 → approve → complete runs once; a retest completes
    idempotent.
  - _Requirements: 3.2, 3.3, 5.5_

- [ ] 9. Phase 8: self-defense plus costs
  - Implement the Probe and Watch runner targets model-free with log, KPI and
    stock wakes.
  - Add quiet-hours deferral, freeze propagation and the incident card plus
    rollback drill.
  - Enforce tenant isolation end to end: per-site kernels, stores, approvals
    and audits.
  - Checkpoint: incident drill from probe wake to rollback; probe wake-to-claim
    within budget; cross-site attempts denied and audited.
  - _Requirements: 1.1, 1.5, 1.6, 5.1, 5.3, 5.4, 6.3, 8.1_
