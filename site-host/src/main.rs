//! Agentrix Website Phase 0: the host scaffold.
//!
//! One site on one kernel over durable SQLite stores: it opens the run, approval, agenda,
//! partition, watermark, signal, charter and KPI stores in one database, registers the
//! seven site processes' standing triggers plus the KPI tracker's nightly trigger and
//! watermarked cron source, files a probe signal through a [`SignalSink`](adk_kernel::charter::SignalSink),
//! and runs the fail-loud [`SelfCheck`](selfcheck::SelfCheck). The trigger table mirrors
//! `adk-website`'s `jobs::processes` (same kinds, jobs, playbooks and targets); Phase 1
//! puts the drivers behind the triggers. It starts no kernel and serves no edge.
//!
//! ```sh
//! cargo run -p agentrix-site-host -- data
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use adk_core::schedule::MissedFirePolicy;
use adk_core::{AdkError, ApprovalPolicy, ApproverSelector, NewRun, Result, StoreApprovalService};
use adk_kernel::charter::{
    KpiTracker, SignalSink, SiteProcessKind, SqliteCharterStore, SqliteKpiStore, SqliteSignalStore,
};
use adk_kernel::{
    CronWakeSource, Kernel, KernelConfig, Partitions, SourceWatermark, SqliteAgendaStore,
    SqliteCronWatermarkStore, SqlitePartitionStore,
};
use adk_session::{SqliteApprovalStore, SqliteRunStore};
use awp_types::{Bounded, Signal, SignalKind};
use serde_json::json;

mod selfcheck;

use selfcheck::SelfCheck;

/// The demo site's application name; its back-office partition hangs below it.
const APP: &str = "demo";

/// A site process: its kind, its job, the agent or workflow that runs it, and its
/// standing trigger. Mirrors `adk_website::jobs::processes`.
pub(crate) struct Process {
    pub(crate) kind: SiteProcessKind,
    pub(crate) job: &'static str,
    pub(crate) target: &'static str,
    pub(crate) trigger: adk_kernel::Trigger,
}

/// The seven processes `adk-website` wakes for a business site: the probe, the watch and
/// the digest run as workflows, the others run their job's agent with the playbook.
fn processes(site: uuid::Uuid, partition: &adk_kernel::PartitionId) -> Vec<Process> {
    const TABLE: [(SiteProcessKind, &str, &str, &str); 7] = [
        (
            SiteProcessKind::Probe,
            "protect",
            "protect",
            "website.guardian.probe",
        ),
        (
            SiteProcessKind::Watch,
            "hosting",
            "hosting",
            "website.guardian.watch",
        ),
        (SiteProcessKind::Content, "freshness", "freshness", "growth"),
        (
            SiteProcessKind::Reviews,
            "social",
            "review-replies",
            "growth",
        ),
        (SiteProcessKind::Outreach, "nurture", "nurture", "revenue"),
        (
            SiteProcessKind::Support,
            "owner-questions",
            "owner-questions",
            "steward",
        ),
        (
            SiteProcessKind::Digest,
            "weekly-review",
            "weekly-review",
            "website.steward.digest",
        ),
    ];
    TABLE
        .into_iter()
        .map(|(kind, job, playbook, target)| {
            let run = NewRun::new(
                kind.queue().as_str(),
                APP,
                format!("site:{APP}"),
                target,
                json!({
                    "site": site,
                    "job": job,
                    "playbook": playbook,
                    "text": format!("Run the {playbook} playbook for job {job}."),
                }),
            );
            let trigger = kind.trigger(kind.default_wake(partition), run, partition.clone());
            Process {
                kind,
                job,
                target,
                trigger,
            }
        })
        .collect()
}

fn store(error: impl std::fmt::Display) -> AdkError {
    AdkError::config(format!("the host's stores could not open: {error}"))
}

/// Opens the durable stores in `data`, registers every trigger, files the probe signal,
/// and returns the green self-check, or the first line that failed.
pub async fn run(data: &Path) -> Result<SelfCheck> {
    let mut check = SelfCheck::new();
    std::fs::create_dir_all(data)
        .map_err(|e| AdkError::config(format!("the data dir {}: {e}", data.display())))?;
    let url = format!("sqlite://{}?mode=rwc", data.join("kernel.db").display());

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .map_err(store)?;
    let runs = Arc::new(SqliteRunStore::from_pool(pool.clone()));
    runs.migrate().await?;
    let approval_store = Arc::new(SqliteApprovalStore::from_pool(pool.clone()));
    approval_store.migrate().await?;
    let approvals: Arc<dyn adk_core::ApprovalService> = Arc::new(
        StoreApprovalService::new(approval_store)
            .with_router(ApprovalPolicy::new([ApproverSelector::human("owner")])),
    );
    let agenda = Arc::new(SqliteAgendaStore::new(pool.clone()));
    agenda.migrate().await?;
    let partition_store = Arc::new(SqlitePartitionStore::new(pool.clone()));
    partition_store.migrate().await?;
    let watermarks = Arc::new(SqliteCronWatermarkStore::new(pool.clone()));
    watermarks.migrate().await?;
    let signals = Arc::new(SqliteSignalStore::new(pool.clone()));
    signals.migrate().await?;
    let charters = Arc::new(SqliteCharterStore::new(pool.clone()));
    charters.migrate().await?;
    let kpis = Arc::new(SqliteKpiStore::new(pool));
    kpis.migrate().await?;
    check.stores(&url);

    let site = uuid::Uuid::nil();
    let partition: adk_kernel::PartitionId = format!("{APP}.back-office").into();
    let processes = processes(site, &partition);
    check.processes(&processes)?;

    let mut builder = Kernel::builder()
        .config(
            KernelConfig::default()
                .with_app_name(APP)
                .with_home(data.join("kernel")),
        )
        .run_store(runs)
        .approval_service(approvals)
        .partitions(Partitions::new(partition_store))
        .agenda_store(agenda.clone())
        .watermark_store(watermarks.clone());
    for process in &processes {
        builder = builder.trigger(process.trigger.clone());
    }
    let tracker = KpiTracker::new(site, charters, kpis);
    builder = builder.trigger(tracker.trigger(APP, "0 0 3 * * *"));
    let watermark = Arc::new(SourceWatermark::new(watermarks, "kernel:kpi"))
        as Arc<dyn adk_agent::ambient::TickWatermark>;
    let cron = CronWakeSource::watermarked(
        "kernel:kpi",
        "0 0 3 * * *",
        MissedFirePolicy::FireOnce,
        watermark,
    )?;
    builder = builder.wake_source(Arc::new(cron));
    let triggers = processes.len() + 1;
    let kernel = builder.build()?;
    check.kernel(triggers);

    // The sink files through the stores directly, with no wakes: the kernel is built, not
    // started, so nothing is there to wake yet.
    let sink = SignalSink::new(signals, agenda, partition);
    let probe = Signal {
        id: uuid::Uuid::new_v4(),
        kind: SignalKind::ContentGap {
            topic: Bounded::new("self-check probe", 80)
                .map_err(|e| AdkError::config(format!("the probe topic: {e}")))?,
            question_count: 1,
            samples: Vec::new(),
        },
        emitted_by: "site-host".into(),
        emitted_at: chrono::Utc::now(),
    };
    let outcome = sink.emit(probe).await?;
    check.signal(&outcome)?;
    check.durable_db(&data.join("kernel.db"))?;

    drop(kernel);
    Ok(check)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let data: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "data".into());
    match run(&data).await {
        Ok(check) => print!("{check}"),
        Err(error) => {
            eprintln!("SELF-CHECK FAILED: {error}");
            std::process::exit(1);
        }
    }
}
