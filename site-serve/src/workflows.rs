//! The started kernel: seven standing triggers, three hosted agent runners, and the
//! guarded workflows for the model-free targets.
//!
//! Probe, Watch and Digest run as workflows through [`SiteWorkflows`], and every
//! execution passes the [`Guard`](agentrix_site_defense::Guard) first: frozen
//! sites skip, quiet hours defer, and the verdict is audited. The trigger table
//! mirrors `site-host` (same kinds, jobs, playbooks and targets).

use std::path::Path;
use std::sync::Arc;

use adk_core::schedule::MissedFirePolicy;
use adk_core::{ApprovalPolicy, ApproverSelector, StoreApprovalService};
use adk_kernel::charter::{SiteProcessKind, SqliteCharterStore, SqliteKpiStore, SqliteSignalStore};
use adk_kernel::{
    CronWakeSource, Kernel, KernelConfig, Partitions, SourceWatermark, SqliteAgendaStore,
    SqliteCronWatermarkStore, SqlitePartitionStore,
};
use adk_runner::Runner;
use adk_server::background::{WorkflowExecutor, WorkflowState};
use agentrix_site_defense::{Freeze, Guard, QuietHours};
use agentrix_site_edge::Current;
use agentrix_site_growth::Growth;
use agentrix_site_inbox::CardStore;
use agentrix_site_release::ReleaseStore;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// The three model-free workflow targets.
pub const PROBE_WORKFLOW: &str = "website.guardian.probe";
/// The facilities watch workflow.
pub const WATCH_WORKFLOW: &str = "website.guardian.watch";
/// The weekly digest workflow.
pub const DIGEST_WORKFLOW: &str = "website.steward.digest";

/// The guarded workflows: probe, watch and digest over the product stores, each
/// execution checked against freeze and quiet hours first.
pub struct SiteWorkflows {
    site: String,
    current: Arc<Current>,
    releases: Arc<ReleaseStore>,
    cards: Arc<CardStore>,
    growth: Arc<Growth>,
    freeze: Arc<Freeze>,
    quiet: Option<QuietHours>,
}

impl SiteWorkflows {
    /// Workflows over the product stores, with quiet hours from the environment
    /// (`QUIET_START_MIN`/`QUIET_END_MIN`, UTC minutes) when both are set.
    pub fn new(
        site: String,
        current: Arc<Current>,
        releases: Arc<ReleaseStore>,
        cards: Arc<CardStore>,
        growth: Arc<Growth>,
        freeze: Arc<Freeze>,
    ) -> Self {
        let quiet = QuietHours::from_env();
        Self {
            site,
            current,
            releases,
            cards,
            growth,
            freeze,
            quiet,
        }
    }

    fn guard(&self) -> Guard<'_> {
        let guard = Guard::new(&self.freeze);
        match self.quiet {
            Some(quiet) => guard.with_quiet(quiet),
            None => guard,
        }
    }
}

#[async_trait::async_trait]
impl WorkflowExecutor for SiteWorkflows {
    fn has_workflow(&self, workflow_id: &str) -> bool {
        matches!(
            workflow_id,
            PROBE_WORKFLOW | WATCH_WORKFLOW | DIGEST_WORKFLOW
        )
    }

    async fn execute(
        &self,
        workflow_id: &str,
        _input: WorkflowState,
        _cancel_token: CancellationToken,
    ) -> std::result::Result<Value, String> {
        let now = chrono::Utc::now();
        match self.guard().check(&self.site, workflow_id, now).await {
            Ok(agentrix_site_defense::GuardDecision::Run) => {}
            Ok(agentrix_site_defense::GuardDecision::Frozen(reason)) => {
                return Ok(json!({ "skipped": "frozen", "reason": reason }));
            }
            Ok(agentrix_site_defense::GuardDecision::Deferred(wake)) => {
                return Ok(json!({ "skipped": "deferred", "wake": wake.to_rfc3339() }));
            }
            Err(error) => return Err(error.to_string()),
        }
        match workflow_id {
            PROBE_WORKFLOW => {
                let report = agentrix_site_defense::probe(&self.releases, &self.current.get(), &[])
                    .await
                    .map_err(|e| e.to_string())?;
                let filed = agentrix_site_defense::file_incidents(&self.cards, &self.site, &report)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(
                    json!({ "clean": report.clean, "incidents": report.incidents.len(), "cards": filed.len() }),
                )
            }
            WATCH_WORKFLOW => {
                let input = agentrix_site_defense::WatchInput {
                    cert_expiry_days: env_u32("CERT_EXPIRY_DAYS", 90),
                    domain_expiry_days: env_u32("DOMAIN_EXPIRY_DAYS", 400),
                    backup_age_days: env_u32("BACKUP_AGE_DAYS", 0),
                    advisories: std::env::var("WATCH_ADVISORIES")
                        .map(|list| {
                            list.split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                let proposals = agentrix_site_defense::watch(&input);
                let filed =
                    agentrix_site_defense::file_proposals(&self.cards, &self.site, &proposals)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(json!({ "proposals": proposals.len(), "cards": filed.len() }))
            }
            DIGEST_WORKFLOW => {
                let (doc, card) =
                    agentrix_site_growth::weekly_review(&self.growth, &self.site, &self.cards)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(
                    json!({ "card": card.id, "spent": doc.spent_micro, "breaches": doc.breaches.len() }),
                )
            }
            _ => Err(format!("unknown workflow {workflow_id}")),
        }
    }
}

fn env_u32(name: &str, fallback: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

/// The kernel's durable stores, opened and migrated: the kernel runs on them and
/// the product tools read pending signals and poll runs through the same handles.
pub struct KernelStores {
    /// Claimed and finished runs.
    pub runs: Arc<adk_session::SqliteRunStore>,
    /// Held approvals.
    pub approvals: Arc<adk_session::SqliteApprovalStore>,
    /// The back-office agenda.
    pub agenda: Arc<SqliteAgendaStore>,
    /// Partitions and their quotas.
    pub partitions: Arc<SqlitePartitionStore>,
    /// Cron watermarks.
    pub watermarks: Arc<SqliteCronWatermarkStore>,
    /// Typed signals and their counts.
    pub signals: Arc<SqliteSignalStore>,
    /// Committed charters.
    pub charters: Arc<SqliteCharterStore>,
    /// KPI series and guardrail state.
    pub kpis: Arc<SqliteKpiStore>,
}

/// The served site's kernel id: one site, one kernel, one id.
pub fn site_id() -> uuid::Uuid {
    uuid::Uuid::nil()
}

/// The kernel database URL under `data`.
pub fn kernel_db_url(data: &Path) -> String {
    format!("sqlite://{}?mode=rwc", data.join("kernel.db").display())
}

/// Opens and migrates every kernel store in one database under `data`.
pub async fn open_kernel_stores(data: &Path) -> anyhow::Result<KernelStores> {
    let url = kernel_db_url(data);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .map_err(|e| anyhow::anyhow!("kernel stores: {e}"))?;
    let runs = Arc::new(adk_session::SqliteRunStore::from_pool(pool.clone()));
    runs.migrate().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let approvals = Arc::new(adk_session::SqliteApprovalStore::from_pool(pool.clone()));
    approvals
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let agenda = Arc::new(SqliteAgendaStore::new(pool.clone()));
    agenda.migrate().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let partitions = Arc::new(SqlitePartitionStore::new(pool.clone()));
    partitions
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let watermarks = Arc::new(SqliteCronWatermarkStore::new(pool.clone()));
    watermarks
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let signals = Arc::new(SqliteSignalStore::new(pool.clone()));
    signals
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let charters = Arc::new(SqliteCharterStore::new(pool.clone()));
    charters
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let kpis = Arc::new(SqliteKpiStore::new(pool));
    kpis.migrate().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(KernelStores {
        runs,
        approvals,
        agenda,
        partitions,
        watermarks,
        signals,
        charters,
        kpis,
    })
}

/// The standing triggers: the host's single table, with the weekly digest
/// firing a missed tick once instead of skipping it — a down Monday must
/// still report the week.
pub fn standing_triggers(
    app: &str,
    site: uuid::Uuid,
    partition: &adk_kernel::PartitionId,
) -> Vec<agentrix_site_host::Process> {
    agentrix_site_host::processes(app, site, partition)
        .into_iter()
        .map(|mut process| {
            if process.kind == SiteProcessKind::Digest
                && let adk_kernel::Wake::Cron { expr, .. } = process.trigger.wake.clone()
            {
                process.trigger.wake = adk_kernel::Wake::Cron {
                    expr,
                    missed: MissedFirePolicy::FireOnce,
                };
            }
            process
        })
        .collect()
}

/// Builds (not starts) the site kernel: durable SQLite stores, the seven standing
/// triggers plus the KPI tracker's nightly trigger, the hosted agent runners, and
/// the guarded workflows.
pub async fn build_kernel(
    app: &str,
    site_name: &str,
    data: &Path,
    stores: &KernelStores,
    growth: Arc<Growth>,
    runners: Vec<(String, Arc<Runner>)>,
    workflows: Arc<SiteWorkflows>,
) -> anyhow::Result<Kernel> {
    let runs = stores.runs.clone();
    let approval_store = stores.approvals.clone();
    let approvals: Arc<dyn adk_core::ApprovalService> = Arc::new(
        StoreApprovalService::new(approval_store)
            .with_router(ApprovalPolicy::new([ApproverSelector::human("owner")])),
    );
    let agenda = stores.agenda.clone();
    let watermarks = stores.watermarks.clone();
    let charters = stores.charters.clone();
    let kpis = stores.kpis.clone();

    let site = site_id();
    let partition: adk_kernel::PartitionId = format!("{app}.back-office").into();
    let partitions = Partitions::new(stores.partitions.clone());
    partitions
        .put(
            adk_kernel::Partition::new(partition.clone(), "back-office").with_quotas(
                adk_kernel::Quotas::new()
                    .with_max_running(4)
                    .with_max_queued(64),
            ),
            adk_core::audit_log::AuditActor::subject(adk_core::principal::SubjectRef::human(
                "owner",
            )),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut builder = Kernel::builder()
        .config(
            KernelConfig::default()
                .with_app_name(app)
                .with_home(data.join("kernel")),
        )
        .run_store(runs)
        .approval_service(approvals)
        .partitions(partitions)
        .agenda_store(agenda)
        .watermark_store(watermarks.clone());
    for process in standing_triggers(app, site, &partition) {
        builder = builder.trigger(process.trigger);
    }
    crate::tracker::seed_charter(charters.as_ref(), site)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let tracker =
        crate::tracker::site_tracker(site, site_name, growth, charters.clone(), kpis.clone());
    builder = builder.trigger(tracker.trigger(app, "0 0 3 * * *"));
    let watermark = Arc::new(SourceWatermark::new(watermarks, "kernel:kpi"))
        as Arc<dyn adk_agent::ambient::TickWatermark>;
    let cron = CronWakeSource::watermarked(
        "kernel:kpi",
        "0 0 3 * * *",
        MissedFirePolicy::FireOnce,
        watermark,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    builder = builder.wake_source(Arc::new(cron));
    for (target, runner) in runners {
        builder = builder.partition_agent(partition.clone(), target, runner);
    }
    builder = builder.workflows(workflows);
    builder.build().map_err(|e| anyhow::anyhow!("{e}"))
}
