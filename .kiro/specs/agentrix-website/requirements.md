# Requirements Document

Agentrix Website — the agentic website (Website 4.0).

## Introduction

Websites today are passive documents: they present content, log visits, and wait.
Their audience is shifting from humans to AI agents, their security is
infrastructure they do not control, and they do no autonomous work — no
proactive outreach, no self-measurement, no cost management. Agentrix Website
turns a website into a governed autonomous agent: seven charter jobs keep it
safe, fresh, found, growing, solvent and accountable; it serves humans and AI
agents from the same signed release; and its owner steers it from one inbox with
budgets, quiet hours, approvals, freeze and rollback. Migration from legacy
sites needs no credentials on the old stack and passes a parity gate before
anything goes live.

This feature delivers the product in nine phases (0–8). The host scaffold,
durable stores and startup self-check exist (Phase 0); the phases below take the
site from read-only dual serving to a fully autonomous business.

## Glossary

- **Site**: one website tenant: its content, agents, jobs, budgets and owner.
- **TenantKernel**: one `adk-kernel` instance per site, with per-site durable
  stores and partitions.
- **Charter job**: one of seven standing jobs, each presented under its role name
  (see Charter roles) and woken by schedule or signal.
- **Site process**: the kernel process kind a charter job runs as (`site.probe`,
  `site.watch`, `site.content`, `site.reviews`, `site.outreach`, `site.support`,
  `site.digest`).
- **Autonomy ladder**: per-job levels (Propose, ActReversibleOnly,
  ActWithApproval, ActWithinBudget) bounding what a job may do unwatched.
- **KPI guardrail**: a measured indicator with target and breach policy; a
  breach pauses or demotes the jobs in its area.
- **Signal**: a typed event (content gap, review seen, lead captured, support
  escalation) filed to the agenda that wakes a charter job.
- **Twin**: the machine-readable projection of a page served to AI agents; the
  human page and its twin derive from one signed release.
- **AWP edge**: the HTTP edge serving humans, agent twins and the public CLI
  from signed releases.
- **Signed release**: an immutable, content-addressed bundle of a site version;
  serving and rollback both name releases.
- **LaunchGate**: the check that admits a release to serving (approvals, parity,
  policy).
- **Digest**: the Steward's periodic review of site health, spend and actions
  for the owner.
- **Inbox**: the owner's single queue of decision cards (approvals, drafts,
  incidents, digests).
- **Decision card**: one actionable inbox item that survives restarts until
  decided.
- **Parity gate**: the migration check comparing legacy and new sites; migration
  completes only at parity 0.95 or above.
- **Quiet hours**: the owner's nightly window during which proactive runs wait.
- **Freeze**: the governance kill switch halting autonomous action site-wide.

## Charter roles

The seven charter jobs are the site's standing staff. Owner-facing surfaces
name them by role; the framework identifiers stay unchanged behind the names.

| Role | Job | Process | Agent | Wakes | Starts at |
|---|---|---|---|---|---|
| Security Guard | protect | `site.probe` | Guardian (workflow, model-free) | Every 5 minutes | ActReversibleOnly |
| Facilities Manager | hosting | `site.watch` | Guardian (workflow, model-free) | Daily 06:00 UTC | Propose |
| Content Editor | freshness | `site.content` | Growth | `content_gap` signal | ActWithinBudget |
| Reputation Manager | social | `site.reviews` | Growth | `review_seen` signal | Propose |
| Sales Rep | nurture | `site.outreach` | Revenue | Daily 09:00 UTC | ActWithinBudget |
| Chief of Staff | owner-questions | `site.support` | Steward | `support_escalation` signal | ActWithinBudget |
| Analyst | weekly-review | `site.digest` | Steward (workflow) | Mondays 07:00 UTC | ActWithinBudget |

- **Security Guard.** Mandate: the site stays the site — served bytes match the
  signed release, abusive requesters are throttled, drift and tampering raise
  incidents. Effects: read-only checks, drift rollback, abuse throttling;
  rollback and read-only are its only incident acts. It never calls a model,
  never publishes, never spends.
- **Facilities Manager.** Mandate: the lights stay on — hosting bills, domain
  renewals, certificate expiry, backup integrity, advisory checks against the
  edge lockfile. Effects: proposes renewals and fixes; renewals within budget.
  It never changes infrastructure unwatched and never publishes content.
- **Content Editor.** Mandate: everything true and fresh — stale facts, broken
  links, outdated pages, unanswered visitor questions. Effects: edits within
  budget; new pages are proposed, never published directly. It never publishes
  without an approval card.
- **Reputation Manager.** Mandate: earn and defend trust — replies to reviews in
  the owner's voice, disputes unfair ratings, rolls ratings into the reputation
  KPI. Effects: drafted replies and disputes via the playbook, starting at
  Propose. It never posts publicly unwatched and never fabricates reviews.
- **Sales Rep.** Mandate: turn attention into revenue — qualifies captured
  leads, nurtures with consent, follows up quotes and bookings, hands warm
  prospects to the owner with context. Effects: outreach within the playbook
  and budget caps. It never contacts anyone without consent basis, never
  exceeds budget, never closes terms the owner must sign.
- **Chief of Staff.** Mandate: no question dies — resolves what the Concierge
  escalates, drafts held replies, routes owner decisions to the inbox with
  recommendations. Effects: answers, drafts, routed decisions. It never speaks
  for the owner externally and never decides cards itself.
- **Analyst.** Mandate: account for everything — composes the weekly review of
  health, spend, actions taken and open cards, and files it as a decision
  card. Effects: reads every store, writes one card. It never acts on the
  business and never edits the past; the report is append-only history.

## Requirements

### Requirement 1: Governed charter jobs

**User Story:** As a site owner, I want the seven charter jobs to run on
schedule and on signal within their autonomy levels, so that the site protects,
freshens, promotes and accounts for itself without unwatched side effects.

#### Acceptance Criteria

1. THE site-host SHALL register standing triggers for all seven site processes
   at startup.
2. WHEN a trigger fires, THE TenantKernel SHALL run the job's playbook on the
   job's queue in the back-office partition.
3. THE TenantKernel SHALL refuse any job action above the job's ladder level and
   file a decision card instead.
4. WHEN a KPI guardrail breaches, THE TenantKernel SHALL pause or demote the
   jobs in the breached area before the next wake.
5. WHILE the site is frozen, THE TenantKernel SHALL start no proactive run.
6. THE site SHALL defer proactive runs that fall inside quiet hours until the
   window ends.
7. THE site SHALL name the seven charter jobs Security Guard, Facilities
   Manager, Content Editor, Reputation Manager, Sales Rep, Chief of Staff and
   Analyst in owner-facing surfaces.

### Requirement 2: Dual-audience serving

**User Story:** As a visitor (human or AI agent), I want the same true content
in the shape my client reads best, so that agents quote the site accurately and
humans get the full experience.

#### Acceptance Criteria

1. THE AWP edge SHALL serve every public page to humans and its twin to agents
   from the same signed release.
2. WHEN an agent requests a page twin, THE AWP edge SHALL return structured
   content with provenance and the release digest.
3. THE twin of a page SHALL carry the same facts as the human page of the same
   release.
4. WHERE the requester is an unauthenticated agent, THE AWP edge SHALL apply
   the anonymous rate limit and capability policy.
5. THE site SHALL expose a public CLI over the AWP edge for pure agentic
   interaction.

### Requirement 3: Owner control

**User Story:** As a site owner, I want one inbox, budgets, quiet hours, freeze
and rollback, so that autonomy never outruns my oversight.

#### Acceptance Criteria

1. THE site SHALL present every approval, draft, incident and digest as a
   decision card in one inbox.
2. WHEN the owner decides a card, THE site SHALL execute the decision exactly
   once and close the card.
3. THE site SHALL reserve budget (USD, tokens, media units) before metered calls
   and refuse calls with no remaining budget.
4. WHEN the owner freezes the site, THE site SHALL halt autonomous action
   within one minute and report the freeze.
5. WHEN the owner rolls back, THE site SHALL serve the named prior release and
   its twins within five minutes.
6. THE Steward SHALL compose the weekly Digest and file it as a decision card.

### Requirement 4: Credential-free migration with parity gate

**User Story:** As a legacy site owner, I want to migrate without handing over
credentials, so that I get an agentic site proven to match my old one before it
goes live.

#### Acceptance Criteria

1. THE migration pipeline SHALL build a candidate site from public evidence
   (crawl, feeds, structured data) with no credentials on the legacy stack.
2. THE migration pipeline SHALL score the candidate against the legacy site on
   content, links, products and metadata parity.
3. IF parity is below 0.95, THEN THE migration pipeline SHALL NOT admit the
   candidate to the LaunchGate.
4. WHEN parity reaches 0.95 or above, THE migration pipeline SHALL file a
   go-live decision card with the parity report attached.
5. THE migration pipeline SHALL record every evidence source and transform in
   the candidate's provenance log.

### Requirement 5: Multi-site tenant isolation

**User Story:** As an agency host, I want each site on its own TenantKernel with
its own stores, so that no site reads, spends or approves for another.

#### Acceptance Criteria

1. THE host SHALL run each site on a dedicated TenantKernel with dedicated
   durable stores.
2. THE retrieval layer SHALL return chunks only from the requesting site's
   corpus.
3. THE approval service SHALL decide cards only against the owning site's
   approvers and budgets.
4. IF a request names a foreign site's resource, THEN THE host SHALL deny the
   request and audit the denial.
5. THE host SHALL attribute every metered call to exactly one site's budget.

### Requirement 6: Durability across restarts

**User Story:** As a site owner, I want state to survive restarts, so that a
crash loses neither my decisions nor the site's memory.

#### Acceptance Criteria

1. THE host SHALL persist runs, approvals, agenda, signals, charters, KPIs,
   autonomy levels and watermarks in durable stores.
2. WHEN the host restarts, THE host SHALL resume held approvals as open
   decision cards.
3. WHEN the host restarts, THE cron watermarks SHALL fire each missed tick
   exactly once.
4. THE host SHALL use no in-memory store in a served deployment.

### Requirement 7: Startup self-check

**User Story:** As an operator, I want startup to fail loudly on a bad
configuration, so that the site never runs silently degraded.

#### Acceptance Criteria

1. THE site-host SHALL report every store opened and every trigger registered
   at startup.
2. IF any store is in-memory or fails to open, THEN THE site-host SHALL exit
   nonzero naming the store.
3. IF any of the seven site processes has no trigger, THEN THE site-host SHALL
   exit nonzero naming the missing kind.
4. THE site-host SHALL file a probe signal at startup and exit nonzero unless
   the probe files or counts.

### Requirement 8: Latency budgets

**User Story:** As a visitor, I want the site to react within its budgets, so
that protection runs promptly and conversations feel live.

#### Acceptance Criteria

1. THE Probe job SHALL go from wake to claimed run within 60 seconds at p95,
   without a model call.
2. THE Concierge SHALL emit its first token within 2 seconds at p95 of a
   visitor message.
3. THE AWP edge SHALL serve cached page twins within 500 milliseconds at p95.

### Non-goals (v1)

- Spatial and 3D serving: the concept surfaces stay demos, not served paths.
- Live ad spend and paid social: Mastodon posts go live; ads are recorded and
  labeled, never spent.
- JavaScript single-page-application generation.
- A hand-rolled fleet autoscaler; the fleet view is a product aggregator.
