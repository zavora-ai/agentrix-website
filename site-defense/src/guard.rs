//! The run guard: freeze halts, quiet hours defer, the audit log remembers.
//!
//! [`Guard::check`] runs before every target: a frozen site skips with its reason
//! audited, a run inside quiet hours defers until the window ends, and only then
//! does the target run. Freeze and the audit live in SQLite beside the products
//! they govern.

use chrono::{DateTime, TimeZone, Utc};
use thiserror::Error;

/// What the guard can get wrong.
#[derive(Debug, Error)]
pub enum GuardError {
    /// The database failed.
    #[error("guard store: {0}")]
    Store(String),
}

impl From<sqlx::Error> for GuardError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.to_string())
    }
}

/// The owner's nightly window in UTC minutes past midnight: proactive runs inside
/// it wait. Overnight windows (`start > end`) work: 23:00–06:00 defers 02:00.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietHours {
    /// Window start, minutes past midnight UTC.
    pub start_min: u16,
    /// Window end, minutes past midnight UTC.
    pub end_min: u16,
}

impl QuietHours {
    /// Quiet hours from `QUIET_START_MIN`/`QUIET_END_MIN` (UTC minutes past
    /// midnight) when both are set and parse; `None` otherwise.
    pub fn from_env() -> Option<Self> {
        let start: u16 = std::env::var("QUIET_START_MIN").ok()?.parse().ok()?;
        let end: u16 = std::env::var("QUIET_END_MIN").ok()?.parse().ok()?;
        Some(Self {
            start_min: start,
            end_min: end,
        })
    }

    /// Whether `now` falls inside the window.
    pub fn defers(&self, now: DateTime<Utc>) -> bool {
        let mins = now.hour_min();
        if self.start_min <= self.end_min {
            mins >= self.start_min && mins < self.end_min
        } else {
            mins >= self.start_min || mins < self.end_min
        }
    }

    /// The next instant at or after `now` outside the window.
    pub fn next_wake(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        if !self.defers(now) {
            return now;
        }
        let day = now.date_naive();
        let end = day
            .and_hms_opt(
                u32::from(self.end_min / 60),
                u32::from(self.end_min % 60),
                0,
            )
            .expect("valid window end");
        let mut wake = Utc.from_utc_datetime(&end);
        if wake <= now {
            wake += chrono::Duration::days(1);
        }
        wake
    }
}

trait HourMin {
    fn hour_min(&self) -> u16;
}

impl HourMin for DateTime<Utc> {
    fn hour_min(&self) -> u16 {
        use chrono::Timelike as _;
        self.hour() as u16 * 60 + self.minute() as u16
    }
}

/// What a run check decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// Run now.
    Run,
    /// Frozen: skip, with the reason.
    Frozen(String),
    /// Inside quiet hours: run at the instant instead.
    Deferred(DateTime<Utc>),
}

/// The per-site freeze switch plus the audit log, in one SQLite database.
pub struct Freeze {
    pool: sqlx::sqlite::SqlitePool,
}

impl Freeze {
    /// Opens (creating and migrating) the store at `url`.
    pub async fn open(url: &str) -> Result<Self, GuardError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| GuardError::Store(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS site_freeze (
                site TEXT PRIMARY KEY, frozen INTEGER NOT NULL, reason TEXT NOT NULL, at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS defense_events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL,
                site TEXT NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL)",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Freezes `site` with `reason`: every target run skips until unfreeze.
    pub async fn freeze(&self, site: &str, reason: &str) -> Result<(), GuardError> {
        sqlx::query(
            "INSERT INTO site_freeze (site, frozen, reason, at) VALUES (?, 1, ?, ?)
             ON CONFLICT(site) DO UPDATE SET frozen = 1, reason = ?, at = ?",
        )
        .bind(site)
        .bind(reason)
        .bind(Utc::now().to_rfc3339())
        .bind(reason)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.audit(site, "frozen", reason).await?;
        Ok(())
    }

    /// Unfreezes `site`.
    pub async fn unfreeze(&self, site: &str) -> Result<(), GuardError> {
        sqlx::query("UPDATE site_freeze SET frozen = 0 WHERE site = ?")
            .bind(site)
            .execute(&self.pool)
            .await?;
        self.audit(site, "unfrozen", "the owner lifted the freeze")
            .await?;
        Ok(())
    }

    /// Whether `site` is frozen, with its reason.
    pub async fn frozen(&self, site: &str) -> Result<Option<String>, GuardError> {
        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT frozen, reason FROM site_freeze WHERE site = ?")
                .bind(site)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(frozen, reason)| (frozen != 0).then_some(reason)))
    }

    /// Appends to the audit log.
    pub async fn audit(&self, site: &str, kind: &str, detail: &str) -> Result<(), GuardError> {
        sqlx::query("INSERT INTO defense_events (at, site, kind, detail) VALUES (?, ?, ?, ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(site)
            .bind(kind)
            .bind(detail)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The audit log for `site`, oldest first.
    pub async fn events(&self, site: &str) -> Result<Vec<(String, String)>, GuardError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT kind, detail FROM defense_events WHERE site = ? ORDER BY seq")
                .bind(site)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }
}

/// The check every target run passes: freeze first, then quiet hours.
pub struct Guard<'a> {
    freeze: &'a Freeze,
    quiet: Option<QuietHours>,
}

impl<'a> Guard<'a> {
    /// A guard over `freeze` with no quiet hours.
    pub fn new(freeze: &'a Freeze) -> Self {
        Self {
            freeze,
            quiet: None,
        }
    }

    /// A guard that also defers inside `quiet`.
    pub fn with_quiet(mut self, quiet: QuietHours) -> Self {
        self.quiet = Some(quiet);
        self
    }

    /// Checks whether `site` may run `target` at `now`: frozen skips with an audit
    /// line, quiet hours defer with one, otherwise the target runs.
    pub async fn check(
        &self,
        site: &str,
        target: &str,
        now: DateTime<Utc>,
    ) -> Result<GuardDecision, GuardError> {
        if let Some(reason) = self.freeze.frozen(site).await? {
            let detail = format!("{target} skipped: {reason}");
            self.freeze.audit(site, "frozen-skip", &detail).await?;
            return Ok(GuardDecision::Frozen(reason));
        }
        if let Some(quiet) = self.quiet
            && quiet.defers(now)
        {
            let wake = quiet.next_wake(now);
            let detail = format!("{target} deferred to {wake}");
            self.freeze.audit(site, "deferred", &detail).await?;
            return Ok(GuardDecision::Deferred(wake));
        }
        Ok(GuardDecision::Run)
    }
}
