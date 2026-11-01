//! The Analyst target: the weekly review as a filed decision card — spend against
//! cap, every indicator against its target, every breach, every demotion.

use agentrix_site_inbox::{Card, CardKind, CardStore, NewCard};
use serde::{Deserialize, Serialize};

use crate::Growth;
use crate::budget::BudgetError;
use crate::ladder::START_LEVELS;

/// The weekly review document: what the site did, spent and measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DigestDoc {
    /// The site reviewed.
    pub site: String,
    /// Micro-USD spent.
    pub spent_micro: i64,
    /// Micro-USD capped.
    pub cap_micro: i64,
    /// `(kpi, value, target, direction)` per indicator.
    pub indicators: Vec<(String, f64, f64, String)>,
    /// One line per breach.
    pub breaches: Vec<String>,
    /// `(job, level)` after this week's demotions.
    pub levels: Vec<(String, String)>,
}

/// Composes the weekly review and files it as an Analyst digest card: spend, all
/// indicators, all breaches, and the current ladder. The review reports; the
/// KPI tracker owns demotion, nightly, before the next wake — so the one level
/// the review prints is already the enforced one.
pub async fn weekly_review(
    growth: &Growth,
    site: &str,
    cards: &CardStore,
) -> Result<(DigestDoc, Card), BudgetError> {
    let (cap_micro, spent_micro, _) = growth.budget(site).await.unwrap_or((0, 0, 0));
    let indicators = growth.snapshot(site).await?;
    let breaches: Vec<String> = growth
        .breaches(site)
        .await?
        .iter()
        .map(|breach| {
            format!(
                "{} at {} against target {}",
                breach.kpi, breach.value, breach.target
            )
        })
        .collect();
    let mut levels = Vec::new();
    for (job, _) in START_LEVELS {
        let level = growth.level(site, job).await?;
        levels.push((job.to_string(), format!("{level:?}")));
    }
    let doc = DigestDoc {
        site: site.to_string(),
        spent_micro,
        cap_micro,
        indicators,
        breaches,
        levels,
    };
    let title = format!("Analyst: weekly review — spent {spent_micro} of {cap_micro} micro-USD");
    let card = cards
        .file(NewCard::new(
            site,
            CardKind::Digest,
            "Analyst",
            title,
            serde_json::to_value(&doc).unwrap(),
        ))
        .await
        .map_err(|e| BudgetError::Store(e.to_string()))?;
    Ok((doc, card))
}
