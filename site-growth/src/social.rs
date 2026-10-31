//! The Sales Rep and Reputation Manager targets: held nurture drafts, metered
//! social posts, and ads that are recorded with zero spend and no spend path.

use std::sync::Mutex;

use agentrix_site_concierge::{LeadSignal, ReviewSignal};
use agentrix_site_inbox::{Card, CardKind, CardStore, NewCard, StoreError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::Growth;
use crate::budget::BudgetError;

/// What the targets can get wrong.
#[derive(Debug, Error)]
pub enum SocialError {
    /// The ledger failed or refused.
    #[error("{0}")]
    Budget(#[from] BudgetError),
    /// The inbox failed.
    #[error("social cards: {0}")]
    Cards(#[from] StoreError),
    /// The payload did not serialize.
    #[error("social payload: {0}")]
    Json(#[from] serde_json::Error),
}

/// Posts one status. Live Mastodon and the recording stub implement the trait.
pub trait Mastodon: Send + Sync {
    /// Posts `status`, returning the post id.
    fn post(&self, status: &str) -> Result<String, SocialError>;
}

/// Records posts in memory instead of publishing: the test social network.
#[derive(Debug, Default)]
pub struct LogMastodon {
    log: Mutex<Vec<String>>,
}

impl LogMastodon {
    /// The statuses recorded so far.
    pub fn posts(&self) -> Vec<String> {
        self.log.lock().expect("post log").clone()
    }
}

impl Mastodon for LogMastodon {
    fn post(&self, status: &str) -> Result<String, SocialError> {
        self.log.lock().expect("post log").push(status.to_string());
        Ok(format!("post-{}", Uuid::new_v4().as_simple()))
    }
}

/// The metered cost of one social post, in micro-USD.
pub const POST_COST_MICRO: i64 = 50;

/// A held nurture draft for one lead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NurtureDraft {
    /// The lead's reference.
    pub lead_ref: String,
    /// The channel it came over.
    pub channel: String,
    /// The drafted follow-up.
    pub message: String,
}

/// What a reply publish did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostOutcome {
    /// Posted and committed: the post id.
    Posted(String),
    /// Held at the cap: the filed card, for the owner to fund or approve.
    Held(Card),
}

/// An ad that was recorded and labeled, never spent: `spent_micro` is always zero,
/// and no function in this crate moves ad money.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdRecord {
    /// The record's id.
    pub id: String,
    /// The campaign.
    pub campaign: String,
    /// The channel, such as `recorded-social`.
    pub channel: String,
    /// The creative.
    pub content: String,
    /// What it would have spent, had spend been allowed.
    pub would_spend_micro: i64,
    /// Always zero.
    pub spent_micro: i64,
}

/// The Sales Rep target: one held nurture draft per lead, each with its draft
/// card. Drafts only — outreach never contacts unwatched.
pub async fn nurture(
    site: &str,
    cards: &CardStore,
    leads: &[LeadSignal],
) -> Result<Vec<(NurtureDraft, Card)>, SocialError> {
    let mut filed = Vec::new();
    for lead in leads {
        let draft = NurtureDraft {
            lead_ref: lead.contact_ref.clone(),
            channel: lead.channel.clone(),
            message: format!(
                "Hello — following up on your {} enquiry. A person reviews every \
                 follow-up before it sends; reply any time and we will pick it up.",
                lead.channel
            ),
        };
        let card = cards
            .file(NewCard::new(
                site,
                CardKind::Draft,
                "Sales Rep",
                format!("Sales Rep: follow up with lead {}", lead.contact_ref),
                serde_json::to_value(&draft)?,
            ))
            .await?;
        filed.push((draft, card));
    }
    Ok(filed)
}

/// The Reputation Manager target: reserves the post's cost, posts the reply, and
/// commits — or, at the cap, files the reply's card and holds it. The cap halts
/// spend before anything is sent, so a held reply is never half-posted.
pub async fn publish_reply(
    growth: &Growth,
    site: &str,
    cards: &CardStore,
    social: &dyn Mastodon,
    review: &ReviewSignal,
    reply: &str,
) -> Result<PostOutcome, SocialError> {
    let reservation = match growth.reserve(site, POST_COST_MICRO, "social post").await {
        Ok(reservation) => reservation,
        Err(BudgetError::OverCap { .. }) => {
            let card = cards
                .file(NewCard::new(
                    site,
                    CardKind::Draft,
                    "Reputation Manager",
                    format!(
                        "Reputation Manager: reply held at cap ({})",
                        review.platform
                    ),
                    serde_json::json!({ "review": review, "reply": reply }),
                ))
                .await?;
            return Ok(PostOutcome::Held(card));
        }
        Err(other) => return Err(other.into()),
    };
    let post = social.post(reply)?;
    growth.commit(&reservation.id).await?;
    Ok(PostOutcome::Posted(post))
}

/// Records an ad as recorded-and-labeled: stored with its would-spend, committed
/// spend always zero. There is deliberately no function that spends ad money.
pub async fn record_ad(
    growth: &Growth,
    site: &str,
    campaign: &str,
    channel: &str,
    content: &str,
    would_spend_micro: i64,
) -> Result<AdRecord, SocialError> {
    let record = AdRecord {
        id: format!("ad-{}", Uuid::new_v4().as_simple()),
        campaign: campaign.to_string(),
        channel: channel.to_string(),
        content: content.to_string(),
        would_spend_micro,
        spent_micro: 0,
    };
    growth
        .ad_insert(
            site,
            &record.id,
            &record.campaign,
            &record.channel,
            &record.content,
            record.would_spend_micro,
        )
        .await?;
    Ok(record)
}

impl Growth {
    async fn ad_insert(
        &self,
        site: &str,
        id: &str,
        campaign: &str,
        channel: &str,
        content: &str,
        would_spend_micro: i64,
    ) -> Result<(), BudgetError> {
        sqlx::query(
            "INSERT INTO ad_records (id, site, campaign, channel, content, would_spend_micro, spent_micro)
             VALUES (?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(id)
        .bind(site)
        .bind(campaign)
        .bind(channel)
        .bind(content)
        .bind(would_spend_micro)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every recorded ad of `site`.
    pub async fn ads(&self, site: &str) -> Result<Vec<AdRecord>, BudgetError> {
        let rows: Vec<(String, String, String, String, i64, i64)> =
            sqlx::query_as("SELECT id, campaign, channel, content, would_spend_micro, spent_micro FROM ad_records WHERE site = ?")
                .bind(site)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, campaign, channel, content, would_spend_micro, spent_micro)| AdRecord {
                    id,
                    campaign,
                    channel,
                    content,
                    would_spend_micro,
                    spent_micro,
                },
            )
            .collect())
    }
}
