//! The autonomy ladder: each job's level per site, seeded from the spec's Charter
//! roles table. A breach demotes the breached area's jobs one step; the floor is
//! Propose, and demotion applies before the next wake because the kernel reads
//! levels when it claims runs, not when triggers fire.

use serde::{Deserialize, Serialize};

use crate::Growth;
use crate::budget::BudgetError;

/// Autonomy levels, least to most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobLevel {
    /// Watch only.
    Observe,
    /// Propose; the owner decides.
    Propose,
    /// Act reversibly.
    ActReversibleOnly,
    /// Act with approval.
    ActWithApproval,
    /// Act within budget.
    ActWithinBudget,
}

impl JobLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Propose => "propose",
            Self::ActReversibleOnly => "act-reversible-only",
            Self::ActWithApproval => "act-with-approval",
            Self::ActWithinBudget => "act-within-budget",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "observe" => Self::Observe,
            "act-reversible-only" => Self::ActReversibleOnly,
            "act-with-approval" => Self::ActWithApproval,
            "act-within-budget" => Self::ActWithinBudget,
            _ => Self::Propose,
        }
    }

    /// One step down, floored at Propose: demotion never blinds a job.
    fn down(self) -> Self {
        match self {
            Self::ActWithinBudget => Self::ActWithApproval,
            Self::ActWithApproval => Self::ActReversibleOnly,
            Self::ActReversibleOnly => Self::Propose,
            Self::Propose | Self::Observe => Self::Propose,
        }
    }
}

/// Job areas: a breach demotes its area's jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// Run and protect: Security Guard, Facilities Manager.
    Run,
    /// Be found and trusted: Content Editor, Reputation Manager.
    Growth,
    /// Revenue: Sales Rep.
    Revenue,
    /// Accountable: Chief of Staff, Analyst.
    Account,
}

impl Area {
    /// The area's jobs.
    pub fn jobs(self) -> &'static [&'static str] {
        match self {
            Self::Run => &["protect", "hosting"],
            Self::Growth => &["freshness", "social"],
            Self::Revenue => &["nurture"],
            Self::Account => &["owner-questions", "weekly-review"],
        }
    }

    /// Parses an area name, or `None`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "run" => Some(Self::Run),
            "growth" => Some(Self::Growth),
            "revenue" => Some(Self::Revenue),
            "account" => Some(Self::Account),
            _ => None,
        }
    }
}

/// The spec's starting levels: `(job, level)`.
pub const START_LEVELS: [(&str, JobLevel); 7] = [
    ("protect", JobLevel::ActReversibleOnly),
    ("hosting", JobLevel::Propose),
    ("freshness", JobLevel::ActWithinBudget),
    ("social", JobLevel::Propose),
    ("nurture", JobLevel::ActWithinBudget),
    ("owner-questions", JobLevel::ActWithinBudget),
    ("weekly-review", JobLevel::ActWithinBudget),
];

impl Growth {
    /// Seeds `site`'s ladder from [`START_LEVELS`], keeping existing rows.
    pub async fn seed_ladder(&self, site: &str) -> Result<(), BudgetError> {
        for (job, level) in START_LEVELS {
            sqlx::query("INSERT OR IGNORE INTO job_levels (site, job, level) VALUES (?, ?, ?)")
                .bind(site)
                .bind(job)
                .bind(level.as_str())
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// `job`'s level on `site`, seeding the ladder first.
    pub async fn level(&self, site: &str, job: &str) -> Result<JobLevel, BudgetError> {
        self.seed_ladder(site).await?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT level FROM job_levels WHERE site = ? AND job = ?")
                .bind(site)
                .bind(job)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row
            .map(|(level,)| JobLevel::parse(&level))
            .unwrap_or(JobLevel::Propose))
    }

    /// Demotes `area`'s jobs one step each, returning `(job, old, new)` per job.
    /// Breaches call this before the next wake; the kernel reads levels at claim
    /// time, so the demotion binds the very next run.
    pub async fn demote_area(
        &self,
        site: &str,
        area: Area,
    ) -> Result<Vec<(String, JobLevel, JobLevel)>, BudgetError> {
        let mut moved = Vec::new();
        for job in area.jobs() {
            let old = self.level(site, job).await?;
            let new = old.down();
            sqlx::query("UPDATE job_levels SET level = ? WHERE site = ? AND job = ?")
                .bind(new.as_str())
                .bind(site)
                .bind(job)
                .execute(&self.pool)
                .await?;
            moved.push((job.to_string(), old, new));
        }
        Ok(moved)
    }

    /// Applies every current breach: demotes each breached area once per call.
    pub async fn apply_breaches(
        &self,
        site: &str,
    ) -> Result<Vec<(String, JobLevel, JobLevel)>, BudgetError> {
        let mut moved = Vec::new();
        let mut done = Vec::new();
        for breach in self.breaches(site).await? {
            let Some(area) = Area::parse(&breach.area) else {
                continue;
            };
            if done.contains(&area) {
                continue;
            }
            done.push(area);
            moved.extend(self.demote_area(site, area).await?);
        }
        Ok(moved)
    }
}
