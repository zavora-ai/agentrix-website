# Substrate Map — Vision → ADK-Rust

What ADK-Rust already implements (source-verified in the October 2026
sweep), what Zavora builds, and what does not exist. Symbols are the
contract; features are opt-in unless noted.

## Already real

| Vision element | Substrate |
|---|---|
| Always-on staffing, jobs wake/sleep free | `adk-kernel` Wake/WakeSet/WakeRouter, 3 queues, quotas, quiet hours (`kernel.rs`, `wake.rs`, `sched.rs`) |
| Job catalogue (Probe/Content/Support/Digest…) | `adk-kernel` `charter` feature: `SiteProcessKind` + `CharterStore/KpiTracker/AutonomyLadder` (`charter/site.rs`) |
| Approval ladder, held acts, partitions | `adk-harness` `website` feature: CATALOG[27], `LadderPolicy`, Public/BackOffice partitions, non-waivable holds (`website/*`) |
| Owner flips worlds | Kernel Partitions + harness `Preset::Website`; public/back-office reconciled by kernel only |
| Dual audience, one release | `adk-site` BuiltinGenerator twins + `adk-awp` v2 edge + `adk-server` AwpEdge (A2A/MCP/consent/beacon); tiers from verified credentials only (`tiers.rs`) |
| Publishing with a gate and rollback | `adk-site` PublishingPipeline + LaunchGate + signed SiteLog + SiteHost (7 backends) |
| Drivers (email/DNS/social/analytics/search/ads-recorded) | `adk-drivers` traits + adapters + `DriverCapPolicy` holds |
| Owner console, agent-drawn UI held for humans | `adk-surface` engine + genui (`SubmitDrawn` never executes) |
| Intelligence (drafts, voice, renders, threads) | `adk-model` Muse: Spark effort ladder, strict schemas, vision, file ASR, Images, Responses chaining, Files |
| Memory with provenance | `adk-harness` facts (Known/Assumed) + `adk-memory` service/graph |

## Zavora builds (the product layer)

- Charter job executors (probe/content/review/outreach/support/digest
  runners) on the kernel Runner contract.
- Migration crawler → twin rebuild → gate parity flow (no credentials).
- KPI sources, approval inbox UX (SurfaceEngine decisions), public
  HTML themes per business (brand extraction).
- Log→detect→act self-defense playbook over audit/trace queries.
- Per-unit modality metering (images/ASR are outside token pricing).

## Does not exist (never demo as live)

- Realtime ASR WebSocket client; SAM segmentation client; live ad
  adapter (recorded replay only); streaming Responses; local schema
  repair. Wonder-1.0 is Preview — opt-in hero copy only.

## Durability wiring (Slice 0, gates everything)

Enable `website+kernel+charter+signing/verify`; wire durable stores
(process/agenda/partition/watermark/mode/rules/drafts/facts),
`schedule_store`, `approval_service`, `with_control`, credential
TierAssigner, and call `start_cron_scheduler`. Without this, "always-on"
is a demo, not a deployment.
