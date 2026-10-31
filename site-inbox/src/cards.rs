//! Durable decision cards with exactly-once decision and recoverable execution.
//!
//! The contract, from spec correctness property P5:
//!
//! - A card is decided at most once: [`CardStore::decide`] moves `open` to
//!   `approved` or `rejected` inside one transaction, and any later decision
//!   fails with [`StoreError::AlreadyDecided`].
//! - An approved card's effect runs at least once: `decide` runs the caller's
//!   executor after committing the decision, marks the card executed only on
//!   success, and [`CardStore::recover`] retries every decided-but-unexecuted
//!   card at startup. Callers key external effects on the card's idempotency
//!   key, which turns at-least-once attempts into exactly-once effects.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

/// What the inbox can get wrong.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The card is not open, so the decision is refused.
    #[error("card {0} is already decided")]
    AlreadyDecided(String),
    /// No card has the id.
    #[error("no card {0}")]
    NotFound(String),
    /// The executor reported failure; the decision stands, the effect retries.
    #[error("card {id} executor failed: {reason}")]
    Executor {
        /// The card.
        id: String,
        /// The failure.
        reason: String,
    },
    /// The database failed.
    #[error("card store: {0}")]
    Db(#[from] sqlx::Error),
    /// A payload did not serialize.
    #[error("card payload: {0}")]
    Json(#[from] serde_json::Error),
}

/// What kind of decision a card asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    /// Approve or reject a held draft.
    Draft,
    /// Acknowledge or escalate an incident.
    Incident,
    /// Acknowledge a digest report.
    Digest,
    /// A generic approval.
    Approval,
}

impl CardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Incident => "incident",
            Self::Digest => "digest",
            Self::Approval => "approval",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "incident" => Self::Incident,
            "digest" => Self::Digest,
            "approval" => Self::Approval,
            _ => Self::Draft,
        }
    }
}

/// Where a card stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardState {
    /// Waiting for a decision.
    Open,
    /// Approved; the effect may still be pending (see `executed`).
    Approved,
    /// Rejected; no effect runs.
    Rejected,
}

impl CardState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => Self::Open,
        }
    }
}

/// The owner's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run the effect.
    Approve,
    /// Close the card with no effect.
    Reject,
}

/// One pending human decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// The card's id.
    pub id: String,
    /// The site it belongs to.
    pub site: String,
    /// What it asks.
    pub kind: CardKind,
    /// The charter role that filed it, such as `Content Editor`.
    pub role: String,
    /// One-line summary.
    pub title: String,
    /// The decision's payload: a draft, an incident report, a digest.
    pub payload: serde_json::Value,
    /// Where it stands.
    pub state: CardState,
    /// Whether an approved card's effect has run.
    pub executed: bool,
    /// Key external effects idempotently under this.
    pub idempotency_key: String,
    /// When it was filed.
    pub created: DateTime<Utc>,
    /// When it was decided, if ever.
    pub decided: Option<DateTime<Utc>>,
}

/// A card to file: the store assigns the id, key and timestamps.
#[derive(Debug, Clone)]
pub struct NewCard {
    /// The site it belongs to.
    pub site: String,
    /// What it asks.
    pub kind: CardKind,
    /// The charter role filing it.
    pub role: String,
    /// One-line summary.
    pub title: String,
    /// The payload.
    pub payload: serde_json::Value,
}

impl NewCard {
    /// A card for `site` asking `kind`, filed by `role`.
    pub fn new(
        site: impl Into<String>,
        kind: CardKind,
        role: impl Into<String>,
        title: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            site: site.into(),
            kind,
            role: role.into(),
            title: title.into(),
            payload,
        }
    }
}

/// Runs an approved card's effect over its payload. Effects must be idempotent
/// under the card's idempotency key: `decide` and `recover` retry until one call
/// reports success.
pub trait Execute: Send + Sync {
    /// Runs the effect; `Err` leaves the card approved-but-unexecuted for retry.
    fn execute(&self, card: &Card) -> Result<(), String>;
}

/// The cards in one SQLite database.
pub struct CardStore {
    pool: SqlitePool,
}

impl CardStore {
    /// Opens (creating and migrating) the store at `url`, such as
    /// `sqlite:///tmp/inbox/cards.db?mode=rwc`.
    pub async fn open(url: &str) -> Result<Self, StoreError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS cards (
                id TEXT PRIMARY KEY,
                site TEXT NOT NULL,
                kind TEXT NOT NULL,
                role TEXT NOT NULL,
                title TEXT NOT NULL,
                payload TEXT NOT NULL,
                state TEXT NOT NULL,
                executed INTEGER NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                created TEXT NOT NULL,
                decided TEXT
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Files `card` as open and returns it with its id and key assigned.
    pub async fn file(&self, card: NewCard) -> Result<Card, StoreError> {
        let filed = Card {
            id: format!("card-{}", Uuid::new_v4().as_simple()),
            site: card.site,
            kind: card.kind,
            role: card.role,
            title: card.title,
            payload: card.payload,
            state: CardState::Open,
            executed: false,
            idempotency_key: format!("exec-{}", Uuid::new_v4().as_simple()),
            created: Utc::now(),
            decided: None,
        };
        sqlx::query(
            "INSERT INTO cards
             (id, site, kind, role, title, payload, state, executed, idempotency_key, created, decided)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&filed.id)
        .bind(&filed.site)
        .bind(filed.kind.as_str())
        .bind(&filed.role)
        .bind(&filed.title)
        .bind(filed.payload.to_string())
        .bind(filed.state.as_str())
        .bind(0_i64)
        .bind(&filed.idempotency_key)
        .bind(filed.created.to_rfc3339())
        .bind(Option::<String>::None)
        .execute(&self.pool)
        .await?;
        Ok(filed)
    }

    /// The card `id`, or `None`.
    pub async fn get(&self, id: &str) -> Result<Option<Card>, StoreError> {
        let row: Option<Row> = sqlx::query_as("SELECT * FROM cards WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// Every open card of `site`, oldest first.
    pub async fn list_open(&self, site: &str) -> Result<Vec<Card>, StoreError> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT * FROM cards WHERE site = ? AND state = 'open' ORDER BY created",
        )
        .bind(site)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// Decides the open card `id`: commits the verdict in one transaction, then —
    /// for approvals only — runs `execute` over the card, marking it executed on
    /// success. A failing executor leaves the card approved-but-unexecuted for
    /// [`recover`](Self::recover); deciding a non-open card fails without touching
    /// anything.
    pub async fn decide(
        &self,
        id: &str,
        decision: Decision,
        execute: &dyn Execute,
    ) -> Result<Card, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<Row> = sqlx::query_as("SELECT * FROM cards WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Err(StoreError::NotFound(id.to_string()));
        };
        let mut card: Card = row.try_into()?;
        if card.state != CardState::Open {
            return Err(StoreError::AlreadyDecided(id.to_string()));
        }
        card.state = match decision {
            Decision::Approve => CardState::Approved,
            Decision::Reject => CardState::Rejected,
        };
        card.decided = Some(Utc::now());
        sqlx::query("UPDATE cards SET state = ?, decided = ? WHERE id = ?")
            .bind(card.state.as_str())
            .bind(card.decided.map(|at| at.to_rfc3339()))
            .bind(&card.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        if decision == Decision::Approve {
            execute
                .execute(&card)
                .map_err(|reason| StoreError::Executor {
                    id: card.id.clone(),
                    reason,
                })?;
            sqlx::query("UPDATE cards SET executed = 1 WHERE id = ?")
                .bind(&card.id)
                .execute(&self.pool)
                .await?;
            card.executed = true;
        }
        Ok(card)
    }

    /// Runs `execute` over every approved-but-unexecuted card and returns the ids
    /// that completed. Call at startup: it finishes what a crash interrupted.
    pub async fn recover(&self, execute: &dyn Execute) -> Result<Vec<String>, StoreError> {
        let rows: Vec<Row> =
            sqlx::query_as("SELECT * FROM cards WHERE state = 'approved' AND executed = 0")
                .fetch_all(&self.pool)
                .await?;
        let mut done = Vec::new();
        for row in rows {
            let card: Card = row.try_into()?;
            execute
                .execute(&card)
                .map_err(|reason| StoreError::Executor {
                    id: card.id.clone(),
                    reason,
                })?;
            sqlx::query("UPDATE cards SET executed = 1 WHERE id = ?")
                .bind(&card.id)
                .execute(&self.pool)
                .await?;
            done.push(card.id);
        }
        Ok(done)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    site: String,
    kind: String,
    role: String,
    title: String,
    payload: String,
    state: String,
    executed: i64,
    idempotency_key: String,
    created: String,
    decided: Option<String>,
}

impl TryFrom<Row> for Card {
    type Error = StoreError;

    fn try_from(row: Row) -> Result<Self, StoreError> {
        let created = row.created.parse::<DateTime<Utc>>().map_err(|e| {
            StoreError::Db(sqlx::Error::Decode(
                format!("bad created timestamp: {e}").into(),
            ))
        })?;
        let decided = row
            .decided
            .map(|at| at.parse::<DateTime<Utc>>())
            .transpose()
            .map_err(|e| {
                StoreError::Db(sqlx::Error::Decode(
                    format!("bad decided timestamp: {e}").into(),
                ))
            })?;
        Ok(Card {
            id: row.id,
            site: row.site,
            kind: CardKind::parse(&row.kind),
            role: row.role,
            title: row.title,
            payload: serde_json::from_str(&row.payload)?,
            state: CardState::parse(&row.state),
            executed: row.executed != 0,
            idempotency_key: row.idempotency_key,
            created,
            decided,
        })
    }
}
