//! KPI targets and readings: the latest reading per indicator breaches when it
//! misses its target in its direction. Breaches name their area, and the ladder
//! demotes that area's jobs.

use serde::{Deserialize, Serialize};

use crate::Growth;
use crate::budget::BudgetError;

/// Which way the indicator must move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Higher is better: breaches below target.
    Up,
    /// Lower is better: breaches above target.
    Down,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// A breached indicator: what missed, where, and by how much.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Breach {
    /// The indicator.
    pub kpi: String,
    /// Its area, whose jobs demote.
    pub area: String,
    /// Latest reading.
    pub value: f64,
    /// The target it missed.
    pub target: f64,
}

impl Growth {
    /// Sets `site`'s target for `kpi`: `target`, `direction`, owning `area`.
    pub async fn set_target(
        &self,
        site: &str,
        kpi: &str,
        target: f64,
        direction: Direction,
        area: &str,
    ) -> Result<(), BudgetError> {
        sqlx::query(
            "INSERT INTO kpi_targets (site, kpi, target, direction, area) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(site, kpi) DO UPDATE SET target = ?, direction = ?, area = ?",
        )
        .bind(site)
        .bind(kpi)
        .bind(target)
        .bind(direction.as_str())
        .bind(area)
        .bind(target)
        .bind(direction.as_str())
        .bind(area)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records a reading of `kpi`.
    pub async fn record(&self, site: &str, kpi: &str, value: f64) -> Result<(), BudgetError> {
        sqlx::query("INSERT INTO kpi_readings (site, kpi, value, at) VALUES (?, ?, ?, ?)")
            .bind(site)
            .bind(kpi)
            .bind(value)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every indicator whose latest reading misses its target, with its area.
    pub async fn breaches(&self, site: &str) -> Result<Vec<Breach>, BudgetError> {
        let targets: Vec<(String, f64, String, String)> =
            sqlx::query_as("SELECT kpi, target, direction, area FROM kpi_targets WHERE site = ?")
                .bind(site)
                .fetch_all(&self.pool)
                .await?;
        let mut breaches = Vec::new();
        for (kpi, target, direction, area) in targets {
            let latest: Option<(f64,)> = sqlx::query_as(
                "SELECT value FROM kpi_readings WHERE site = ? AND kpi = ? ORDER BY seq DESC LIMIT 1",
            )
            .bind(site)
            .bind(&kpi)
            .fetch_optional(&self.pool)
            .await?;
            let Some((value,)) = latest else { continue };
            let breached = if direction == "up" {
                value < target
            } else {
                value > target
            };
            if breached {
                breaches.push(Breach {
                    kpi,
                    area,
                    value,
                    target,
                });
            }
        }
        Ok(breaches)
    }

    /// Latest reading per indicator with its target, for the digest.
    pub async fn snapshot(
        &self,
        site: &str,
    ) -> Result<Vec<(String, f64, f64, String)>, BudgetError> {
        let targets: Vec<(String, f64, String)> =
            sqlx::query_as("SELECT kpi, target, direction FROM kpi_targets WHERE site = ?")
                .bind(site)
                .fetch_all(&self.pool)
                .await?;
        let mut rows = Vec::new();
        for (kpi, target, direction) in targets {
            let latest: Option<(f64,)> = sqlx::query_as(
                "SELECT value FROM kpi_readings WHERE site = ? AND kpi = ? ORDER BY seq DESC LIMIT 1",
            )
            .bind(site)
            .bind(&kpi)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((value,)) = latest {
                rows.push((kpi, value, target, direction));
            }
        }
        Ok(rows)
    }
}
