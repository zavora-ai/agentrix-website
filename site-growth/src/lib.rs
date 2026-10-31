//! Agentrix Website Phase 6: campaigns plus social.
//!
//! One [`Growth`] store holds the site's money, measures and mandates: the budget
//! ledger refuses reservations past the cap ([`budget`]), KPI readings breach
//! against their targets ([`kpi`]), and breaches demote the breached area's jobs
//! one ladder step ([`ladder`]). The Sales Rep nurtures leads into held drafts,
//! the Reputation Manager posts replies through metered budgets, ads are recorded
//! with zero spend and no spend path ([`social`]), and the Analyst files the
//! weekly review as a decision card ([`analyst`]).

pub mod analyst;
pub mod budget;
pub mod kpi;
pub mod ladder;
pub mod social;

pub use analyst::{DigestDoc, weekly_review};
pub use budget::{BudgetError, Reservation};
pub use kpi::{Breach, Direction};
pub use ladder::{Area, JobLevel, START_LEVELS};
pub use social::{
    AdRecord, LogMastodon, Mastodon, NurtureDraft, PostOutcome, nurture, publish_reply, record_ad,
};

/// The site's money, measures and mandates in one SQLite database.
pub struct Growth {
    pool: sqlx::sqlite::SqlitePool,
}

impl Growth {
    /// Opens (creating and migrating) the store at `url`.
    pub async fn open(url: &str) -> Result<Self, BudgetError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| BudgetError::Store(e.to_string()))?;
        for table in [
            "CREATE TABLE IF NOT EXISTS budgets (
                site TEXT PRIMARY KEY, cap_micro INTEGER NOT NULL,
                spent_micro INTEGER NOT NULL, reserved_micro INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS reservations (
                id TEXT PRIMARY KEY, site TEXT NOT NULL, amount_micro INTEGER NOT NULL,
                purpose TEXT NOT NULL, state TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS kpi_targets (
                site TEXT NOT NULL, kpi TEXT NOT NULL, target REAL NOT NULL,
                direction TEXT NOT NULL, area TEXT NOT NULL,
                PRIMARY KEY (site, kpi))",
            "CREATE TABLE IF NOT EXISTS kpi_readings (
                seq INTEGER PRIMARY KEY AUTOINCREMENT, site TEXT NOT NULL, kpi TEXT NOT NULL,
                value REAL NOT NULL, at TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS job_levels (
                site TEXT NOT NULL, job TEXT NOT NULL, level TEXT NOT NULL,
                PRIMARY KEY (site, job))",
            "CREATE TABLE IF NOT EXISTS ad_records (
                id TEXT PRIMARY KEY, site TEXT NOT NULL, campaign TEXT NOT NULL,
                channel TEXT NOT NULL, content TEXT NOT NULL,
                would_spend_micro INTEGER NOT NULL, spent_micro INTEGER NOT NULL)",
        ] {
            sqlx::query(table).execute(&pool).await?;
        }
        Ok(Self { pool })
    }
}
