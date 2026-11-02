//! The nightly KPI loop: the kernel tracker fed by product sources.
//!
//! [`GrowthKpiSource`] reads the growth store's gauges, [`GrowthControl`]
//! pauses and resumes jobs through the pause table, [`site_tracker`] wires
//! both into the [`KpiTracker`](adk_kernel::charter::KpiTracker), and
//! [`seed_charter`] commits the charter v1 the tracker ticks against. The tick
//! records samples, updates guardrail state and pauses breached jobs; the
//! tools' [`enforce`](agentrix_site_growth::Growth::enforce) demotes eagerly at
//! action time, so a breach binds the next action whatever the tick has seen.

use std::sync::Arc;

use adk_kernel::charter::{
    CharterStore, CommitDecision, JobControl, KpiSource, KpiTracker, RoleApproval,
};
use adk_kernel::{KernelError, Result as KernelResult};
use agentrix_site_growth::Growth;
use awp_types::{
    Breach, Budget, Cadence, Charter, Direction, Governance, Guardrail, GuardrailAction, Kpi,
    Measurement, MetricRef, PublicTerms, Reporting, Role, RoleHolder, SiteKind, SourceRef, Target,
    Voice,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The source name the charter's KPIs name and the tracker resolves.
pub const GROWTH_SOURCE: &str = "site-growth";

/// Reads the growth store's gauges: spend ratio and open breach count, both
/// cumulative over the budget period rather than the tick window.
pub struct GrowthKpiSource {
    site_id: Uuid,
    site: String,
    growth: Arc<Growth>,
}

impl GrowthKpiSource {
    /// A source reading `site`'s gauges for tracker site `site_id`.
    pub fn new(site_id: Uuid, site: String, growth: Arc<Growth>) -> Self {
        Self {
            site_id,
            site,
            growth,
        }
    }
}

#[async_trait::async_trait]
impl KpiSource for GrowthKpiSource {
    fn name(&self) -> &str {
        GROWTH_SOURCE
    }

    async fn read(
        &self,
        site: Uuid,
        metric: &str,
        _from: DateTime<Utc>,
        _to: DateTime<Utc>,
    ) -> KernelResult<Option<f64>> {
        if site != self.site_id {
            return Ok(None);
        }
        match metric {
            "budget.spend_ratio" => {
                let (cap, spent, _) = self.growth.budget(&self.site).await.map_err(store_error)?;
                Ok(Some(if cap > 0 {
                    spent as f64 / cap as f64
                } else {
                    0.0
                }))
            }
            "growth.breach_count" => {
                let breaches = self
                    .growth
                    .breaches(&self.site)
                    .await
                    .map_err(store_error)?;
                Ok(Some(breaches.len() as f64))
            }
            _ => Ok(None),
        }
    }
}

fn store_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::Store {
        backend: "sqlite",
        reason: error.to_string(),
    }
}

/// Pauses and resumes jobs through the growth pause table: the tracker's
/// breach arm, honored by every tool at action time.
pub struct GrowthControl {
    site_id: Uuid,
    site: String,
    growth: Arc<Growth>,
}

impl GrowthControl {
    /// A control pausing `site`'s jobs for tracker site `site_id`.
    pub fn new(site_id: Uuid, site: String, growth: Arc<Growth>) -> Self {
        Self {
            site_id,
            site,
            growth,
        }
    }

    fn check_site(&self, site: Uuid) -> KernelResult<()> {
        if site == self.site_id {
            Ok(())
        } else {
            Err(KernelError::Config {
                reason: format!("GrowthControl serves one site, not {site}"),
            })
        }
    }
}

#[async_trait::async_trait]
impl JobControl for GrowthControl {
    async fn pause(&self, site: Uuid, job: &str, reason: &str) -> KernelResult<()> {
        self.check_site(site)?;
        self.growth
            .pause_job(&self.site, job, reason)
            .await
            .map_err(store_error)
    }

    async fn resume(
        &self,
        site: Uuid,
        job: &str,
        decision: &adk_kernel::charter::OwnerDecision,
    ) -> KernelResult<bool> {
        decision.check_person()?;
        self.check_site(site)?;
        self.growth
            .resume_job(&self.site, job)
            .await
            .map_err(store_error)
    }

    async fn paused(&self, site: Uuid, job: &str) -> KernelResult<Option<String>> {
        self.check_site(site)?;
        self.growth
            .paused_reason(&self.site, job)
            .await
            .map_err(store_error)
    }
}

/// The site's tracker: growth sources and control over the kernel stores.
pub fn site_tracker(
    site_id: Uuid,
    site: &str,
    growth: Arc<Growth>,
    charters: Arc<dyn CharterStore>,
    kpis: Arc<dyn adk_kernel::charter::KpiStore>,
) -> KpiTracker {
    KpiTracker::new(site_id, charters, kpis)
        .with_source(Arc::new(GrowthKpiSource::new(
            site_id,
            site.to_string(),
            growth.clone(),
        )))
        .with_control(Arc::new(GrowthControl::new(
            site_id,
            site.to_string(),
            growth,
        )))
}

/// Commits the charter v1 the tracker ticks against: the owner holds the Owner
/// role, two KPIs feed two pausing guardrails — spend past 90% pauses the
/// spending jobs, any open breach pauses the outward-acting ones. Answering
/// and reporting keep running under breach; silence never scores. Returns
/// `false`, committing nothing, when a charter is already in force.
/// The charter kind for a template: shops sell, blogs opine, news informs.
pub fn site_kind_for(template: &str) -> SiteKind {
    match template {
        "shop" => SiteKind::Ecommerce,
        "blog" => SiteKind::Personal,
        "news" => SiteKind::Media,
        _ => SiteKind::Business,
    }
}

pub async fn seed_charter(
    charters: &dyn CharterStore,
    site_id: Uuid,
    kind: SiteKind,
) -> KernelResult<bool> {
    if charters.current(site_id).await?.is_some() {
        return Ok(false);
    }
    let now = Utc::now();
    let day = std::time::Duration::from_secs(86_400);
    let charter = Charter {
        version: 1,
        site_id,
        kind,
        governance: Governance {
            roles: vec![RoleHolder {
                principal: "owner".to_string(),
                roles: vec![Role::Owner],
            }],
            approval_chains: Vec::new(),
        },
        mission: "Run the site as a governed autonomous business.".to_string(),
        strategy: Vec::new(),
        goals: Vec::new(),
        kpis: vec![
            Kpi {
                id: "spend_ratio".to_string(),
                name: "Budget spend ratio".to_string(),
                metric: MetricRef::from("budget.spend_ratio"),
                source: SourceRef::from(GROWTH_SOURCE),
                direction: Direction::Down,
                baseline: Some(Measurement {
                    value: 0.0,
                    window: day,
                    measured_at: now,
                }),
                target: Some(Target {
                    value: 0.9,
                    horizon: now + chrono::Duration::days(365),
                }),
                cadence: Cadence::Daily,
            },
            Kpi {
                id: "breach_count".to_string(),
                name: "Open KPI breaches".to_string(),
                metric: MetricRef::from("growth.breach_count"),
                source: SourceRef::from(GROWTH_SOURCE),
                direction: Direction::Down,
                baseline: Some(Measurement {
                    value: 0.0,
                    window: day,
                    measured_at: now,
                }),
                target: Some(Target {
                    value: 0.0,
                    horizon: now + chrono::Duration::days(365),
                }),
                cadence: Cadence::Daily,
            },
        ],
        guardrails: vec![
            Guardrail {
                id: "spend_cap".to_string(),
                metric: MetricRef::from("budget.spend_ratio"),
                threshold: 0.9,
                breach: Breach::Above,
                window: day,
                action: GuardrailAction::PauseJob,
                jobs: vec!["nurture".to_string(), "social".to_string()],
            },
            Guardrail {
                id: "breaches_open".to_string(),
                metric: MetricRef::from("growth.breach_count"),
                threshold: 0.0,
                breach: Breach::Above,
                window: day,
                action: GuardrailAction::PauseJob,
                jobs: vec![
                    "freshness".to_string(),
                    "social".to_string(),
                    "nurture".to_string(),
                ],
            },
        ],
        authority: Vec::new(),
        budget: Budget {
            period: Cadence::Monthly,
            currency: "USD".to_string(),
            caps: Default::default(),
        },
        voice: Voice {
            tone: "Plain, warm, precise.".to_string(),
            languages: Vec::new(),
            disclosure: "Drafted by the site's staff; the owner approved it.".to_string(),
        },
        red_lines: Vec::new(),
        reporting: Reporting {
            review: Cadence::Weekly,
            routes: Vec::new(),
            quiet_hours: None,
        },
        public: PublicTerms::default(),
        price_rule: Default::default(),
        handoff_threshold: None,
        publish_delay_hours: None,
        amount_thresholds: Default::default(),
        signed: None,
    };
    let decision = CommitDecision::new("seed:charter-v1", now).with_approval(RoleApproval::new(
        Role::Owner,
        adk_core::principal::SubjectRef::human("owner"),
        now,
    ));
    charters.commit(charter, decision).await?;
    Ok(true)
}
