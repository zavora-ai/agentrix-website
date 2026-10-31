//! The budget ledger: caps set the ceiling, reservations precede spend, and the cap
//! halts everything past it. Money moves reserve → commit or release; there is no
//! direct spend path, so an over-cap call is refused before anything is sent.

use thiserror::Error;
use uuid::Uuid;

use crate::Growth;

/// What the ledger can get wrong.
#[derive(Debug, Error)]
pub enum BudgetError {
    /// The reservation would pass the cap.
    #[error("budget: {purpose} needs {amount} micro-USD but {remaining} remain of {cap}")]
    OverCap {
        /// What wanted the money.
        purpose: String,
        /// Micro-USD asked.
        amount: i64,
        /// Micro-USD left under the cap.
        remaining: i64,
        /// The cap.
        cap: i64,
    },
    /// No reservation has the id.
    #[error("no reservation {0}")]
    UnknownReservation(String),
    /// The reservation is not open.
    #[error("reservation {0} is already settled")]
    Settled(String),
    /// The site has no budget row.
    #[error("site {0} has no budget")]
    NoBudget(String),
    /// The database failed.
    #[error("budget store: {0}")]
    Store(String),
}

impl From<sqlx::Error> for BudgetError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.to_string())
    }
}

/// An open hold on budget: commit it after the metered call, release it on failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    /// The reservation's id.
    pub id: String,
    /// The site whose budget holds it.
    pub site: String,
    /// Micro-USD held.
    pub amount_micro: i64,
    /// What it is for.
    pub purpose: String,
}

impl Growth {
    /// Sets (or replaces) `site`'s cap in micro-USD, preserving spend.
    pub async fn set_cap(&self, site: &str, cap_micro: i64) -> Result<(), BudgetError> {
        sqlx::query(
            "INSERT INTO budgets (site, cap_micro, spent_micro, reserved_micro) VALUES (?, ?, 0, 0)
             ON CONFLICT(site) DO UPDATE SET cap_micro = ?",
        )
        .bind(site)
        .bind(cap_micro)
        .bind(cap_micro)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// `(cap, spent, reserved)` in micro-USD.
    pub async fn budget(&self, site: &str) -> Result<(i64, i64, i64), BudgetError> {
        let row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT cap_micro, spent_micro, reserved_micro FROM budgets WHERE site = ?",
        )
        .bind(site)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or_else(|| BudgetError::NoBudget(site.to_string()))
    }

    /// Holds `amount_micro` for `purpose`, or refuses when the cap cannot cover it.
    /// The refusal names the shortfall; nothing is held, nothing is sent.
    pub async fn reserve(
        &self,
        site: &str,
        amount_micro: i64,
        purpose: &str,
    ) -> Result<Reservation, BudgetError> {
        let (cap, spent, reserved) = self.budget(site).await?;
        let remaining = cap - spent - reserved;
        if amount_micro > remaining {
            return Err(BudgetError::OverCap {
                purpose: purpose.to_string(),
                amount: amount_micro,
                remaining,
                cap,
            });
        }
        let reservation = Reservation {
            id: format!("rsv-{}", Uuid::new_v4().as_simple()),
            site: site.to_string(),
            amount_micro,
            purpose: purpose.to_string(),
        };
        sqlx::query("INSERT INTO reservations (id, site, amount_micro, purpose, state) VALUES (?, ?, ?, ?, 'open')")
            .bind(&reservation.id)
            .bind(site)
            .bind(amount_micro)
            .bind(purpose)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE budgets SET reserved_micro = reserved_micro + ? WHERE site = ?")
            .bind(amount_micro)
            .bind(site)
            .execute(&self.pool)
            .await?;
        Ok(reservation)
    }

    /// Commits an open reservation: held money becomes spent.
    pub async fn commit(&self, id: &str) -> Result<(), BudgetError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT site, amount_micro FROM reservations WHERE id = ? AND state = 'open'",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((site, amount)) = row else {
            return Err(BudgetError::Settled(id.to_string()));
        };
        sqlx::query("UPDATE reservations SET state = 'committed' WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "UPDATE budgets SET reserved_micro = reserved_micro - ?, spent_micro = spent_micro + ? WHERE site = ?",
        )
        .bind(amount)
        .bind(amount)
        .bind(site)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Releases an open reservation: held money returns to the cap.
    pub async fn release(&self, id: &str) -> Result<(), BudgetError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT site, amount_micro FROM reservations WHERE id = ? AND state = 'open'",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((site, amount)) = row else {
            return Err(BudgetError::Settled(id.to_string()));
        };
        sqlx::query("UPDATE reservations SET state = 'released' WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE budgets SET reserved_micro = reserved_micro - ? WHERE site = ?")
            .bind(amount)
            .bind(site)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
