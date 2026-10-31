//! Social posters: Instagram primary, Mastodon alternative, one trait.
//!
//! [`SocialPoster`] publishes one status and returns the network's post id. The
//! target never knows which network it posts to. Instagram needs a media URL for
//! every post (its API publishes no text-only statuses), so [`InstagramSocial`]
//! pairs each caption with the account's brand image; [`MastodonSocial`] posts
//! text to any instance's statuses API.

use std::sync::Mutex;
use std::time::Duration;

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
    /// The network call failed.
    #[error("social post: {0}")]
    Post(String),
}

/// Publishes one status and returns the network's post id.
pub trait SocialPoster: Send + Sync {
    /// Posts `status`, returning the network's post id.
    fn post(&self, status: &str) -> Result<String, SocialError>;
}

/// Records posts in memory instead of publishing: the test social network.
#[derive(Debug, Default)]
pub struct LogSocial {
    log: Mutex<Vec<String>>,
}

impl LogSocial {
    /// The statuses recorded so far.
    pub fn posts(&self) -> Vec<String> {
        self.log.lock().expect("post log").clone()
    }
}

impl SocialPoster for LogSocial {
    fn post(&self, status: &str) -> Result<String, SocialError> {
        self.log.lock().expect("post log").push(status.to_string());
        Ok(format!("post-{}", Uuid::new_v4().as_simple()))
    }
}

fn http() -> Result<reqwest::blocking::Client, SocialError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("agentrix-site/0.1")
        .build()
        .map_err(|e| SocialError::Post(e.to_string()))
}

/// Publishes through the Instagram Content Publishing API: a media container with
/// the caption over the account's brand image, then the publish call. Reads
/// `META_IG_USER_ID`, `META_IG_ACCESS_TOKEN` and `META_IG_IMAGE_URL` when built
/// with [`from_env`](Self::from_env); the token needs
/// `instagram_business_content_publish`.
pub struct InstagramSocial {
    user_id: String,
    token: String,
    image_url: String,
    client: reqwest::blocking::Client,
}

impl InstagramSocial {
    /// A poster for `user_id` pairing every caption with `image_url`.
    pub fn new(
        user_id: impl Into<String>,
        token: impl Into<String>,
        image_url: impl Into<String>,
    ) -> Result<Self, SocialError> {
        Ok(Self {
            user_id: user_id.into(),
            token: token.into(),
            image_url: image_url.into(),
            client: http()?,
        })
    }

    /// A poster from the environment, or what is missing.
    pub fn from_env() -> Result<Self, SocialError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| SocialError::Post(format!("{name} is not set")))
        };
        Self::new(
            var("META_IG_USER_ID")?,
            var("META_IG_ACCESS_TOKEN")?,
            var("META_IG_IMAGE_URL")?,
        )
    }

    /// The media-container endpoint: pure, so tests pin it without posting.
    pub fn media_url(&self) -> String {
        format!("https://graph.facebook.com/v21.0/{}/media", self.user_id)
    }

    /// The publish endpoint: pure, so tests pin it without posting.
    pub fn publish_url(&self) -> String {
        format!(
            "https://graph.facebook.com/v21.0/{}/media_publish",
            self.user_id
        )
    }
}

impl SocialPoster for InstagramSocial {
    fn post(&self, status: &str) -> Result<String, SocialError> {
        let failed = |e: reqwest::Error| SocialError::Post(e.to_string());
        let container: serde_json::Value = self
            .client
            .post(self.media_url())
            .form(&[
                ("image_url", self.image_url.as_str()),
                ("caption", status),
                ("access_token", self.token.as_str()),
            ])
            .send()
            .map_err(failed)?
            .error_for_status()
            .map_err(failed)?
            .json()
            .map_err(failed)?;
        let creation = container
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SocialError::Post("instagram returned no container id".into()))?;
        let published: serde_json::Value = self
            .client
            .post(self.publish_url())
            .form(&[
                ("creation", creation),
                ("access_token", self.token.as_str()),
            ])
            .send()
            .map_err(failed)?
            .error_for_status()
            .map_err(failed)?
            .json()
            .map_err(failed)?;
        published
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| SocialError::Post("instagram returned no media id".into()))
    }
}

/// Publishes to a Mastodon instance's statuses API. Reads `MASTODON_INSTANCE`
/// (such as `https://mastodon.social`) and `MASTODON_ACCESS_TOKEN` when built
/// with [`from_env`](Self::from_env).
pub struct MastodonSocial {
    instance: String,
    token: String,
    client: reqwest::blocking::Client,
}

impl MastodonSocial {
    /// A poster for `instance` with `token`.
    pub fn new(instance: impl Into<String>, token: impl Into<String>) -> Result<Self, SocialError> {
        Ok(Self {
            instance: instance.into(),
            token: token.into(),
            client: http()?,
        })
    }

    /// A poster from the environment, or what is missing.
    pub fn from_env() -> Result<Self, SocialError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| SocialError::Post(format!("{name} is not set")))
        };
        Self::new(var("MASTODON_INSTANCE")?, var("MASTODON_ACCESS_TOKEN")?)
    }

    /// The statuses endpoint: pure, so tests pin it without posting.
    pub fn status_url(&self) -> String {
        format!("{}/api/v1/statuses", self.instance.trim_end_matches('/'))
    }
}

impl SocialPoster for MastodonSocial {
    fn post(&self, status: &str) -> Result<String, SocialError> {
        let failed = |e: reqwest::Error| SocialError::Post(e.to_string());
        let answer: serde_json::Value = self
            .client
            .post(self.status_url())
            .bearer_auth(&self.token)
            .form(&[("status", status)])
            .send()
            .map_err(failed)?
            .error_for_status()
            .map_err(failed)?
            .json()
            .map_err(failed)?;
        answer
            .get("id")
            .and_then(|id| {
                id.as_str()
                    .map(str::to_string)
                    .or_else(|| id.as_i64().map(|id| id.to_string()))
            })
            .ok_or_else(|| SocialError::Post("mastodon returned no id".into()))
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
    social: &dyn SocialPoster,
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
