//! Held payment intents: propose, approve, complete — idempotent at every step.

use std::sync::{Arc, Mutex};

use agentrix_site_growth::{BudgetError, Growth};
use agentrix_site_inbox::{CardKind, CardState, CardStore, NewCard, StoreError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// What payments can get wrong.
#[derive(Debug, Error)]
pub enum PaymentError {
    /// No intent has the id.
    #[error("no payment intent {0}")]
    UnknownIntent(String),
    /// The approval card does not approve this intent.
    #[error("payment approval refused: {0}")]
    BadApproval(String),
    /// The provider failed the charge; the fee reservation was released.
    #[error("provider: {0}")]
    Provider(String),
    /// The ledger failed or the fee passed the cap.
    #[error("{0}")]
    Budget(#[from] BudgetError),
    /// The inbox failed.
    #[error("payment cards: {0}")]
    Cards(#[from] StoreError),
    /// A payload did not serialize.
    #[error("payment payload: {0}")]
    Json(#[from] serde_json::Error),
    /// The database failed.
    #[error("payment store: {0}")]
    Store(String),
}

impl From<sqlx::Error> for PaymentError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.to_string())
    }
}

/// Where an intent stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntentState {
    /// Proposed and carded; no money moved.
    Held,
    /// Charged exactly once.
    Completed,
    /// The provider failed; completable again.
    Failed,
}

impl IntentState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Held,
        }
    }
}

/// A payment to propose: the details `propose` records as a held intent.
#[derive(Debug, Clone)]
pub struct NewPayment {
    /// Micro-USD (or minor units of `currency`).
    pub amount_micro: i64,
    /// ISO currency code.
    pub currency: String,
    /// What it is for.
    pub purpose: String,
    /// Opaque payer reference.
    pub payer_ref: String,
    /// Proposing twice under one key returns one intent.
    pub idempotency_key: String,
    /// The provider's payment method, when the payer saved one.
    pub payment_method: Option<String>,
}

/// One held payment: the intent, its card, and its charge record once done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentIntent {
    /// The intent's id.
    pub id: String,
    /// The site it pays.
    pub site: String,
    /// Micro-USD (or minor units of `currency`).
    pub amount_micro: i64,
    /// ISO currency code.
    pub currency: String,
    /// What it is for.
    pub purpose: String,
    /// Opaque payer reference.
    pub payer_ref: String,
    /// Proposing twice under one key returns one intent.
    pub idempotency_key: String,
    /// The provider's payment method, when the payer saved one.
    pub payment_method: Option<String>,
    /// Where it stands.
    pub state: IntentState,
    /// Its approval card.
    pub card_id: String,
    /// The provider's charge id, once completed.
    pub charge_id: Option<String>,
}

/// The completed charge: what moved and what it cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The intent.
    pub intent: String,
    /// The provider's charge id.
    pub charge_id: String,
    /// Micro-USD charged.
    pub amount_micro: i64,
    /// Micro-USD fee committed to the site's budget.
    pub fee_micro: i64,
}

/// Charges one intent. Live providers and the recording stub implement the trait.
pub trait PaymentProvider: Send + Sync {
    /// Charges the intent's amount, returning the provider's charge id.
    fn charge(&self, intent: &PaymentIntent) -> Result<String, PaymentError>;
}

/// Records charges in memory instead of moving money: the test provider.
#[derive(Debug, Default)]
pub struct LogProvider {
    log: Mutex<Vec<String>>,
}

impl LogProvider {
    /// The intent ids charged so far.
    pub fn charges(&self) -> Vec<String> {
        self.log.lock().expect("charge log").clone()
    }
}

impl PaymentProvider for LogProvider {
    fn charge(&self, intent: &PaymentIntent) -> Result<String, PaymentError> {
        self.log.lock().expect("charge log").push(intent.id.clone());
        Ok(format!("ch-{}", Uuid::new_v4().as_simple()))
    }
}

/// The provider fee: 3% of the amount, floored at 30 micro-USD.
pub fn fee_for(amount_micro: i64) -> i64 {
    (amount_micro * 3 / 100).max(30)
}

/// The held intents in one SQLite database, charging through one provider.
pub struct Commerce {
    pub(crate) pool: sqlx::sqlite::SqlitePool,
    provider: Arc<dyn PaymentProvider>,
}

impl Commerce {
    /// Opens (creating and migrating) the store at `url`.
    pub async fn open(url: &str, provider: Arc<dyn PaymentProvider>) -> Result<Self, PaymentError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| PaymentError::Store(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS intents (
                id TEXT PRIMARY KEY,
                site TEXT NOT NULL,
                amount_micro INTEGER NOT NULL,
                currency TEXT NOT NULL,
                purpose TEXT NOT NULL,
                payer_ref TEXT NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                payment_method TEXT,
                state TEXT NOT NULL,
                card_id TEXT NOT NULL,
                charge_id TEXT
            )",
        )
        .execute(&pool)
        .await?;
        crate::orders::migrate(&pool)
            .await
            .map_err(|e| PaymentError::Store(e.to_string()))?;
        Ok(Self { pool, provider })
    }

    /// Proposes a payment: records the held intent and files its approval card —
    /// the 202. Proposing twice under `idempotency_key` returns the first intent
    /// and files nothing new.
    pub async fn propose(
        &self,
        site: &str,
        cards: &CardStore,
        payment: NewPayment,
    ) -> Result<(PaymentIntent, agentrix_site_inbox::Card), PaymentError> {
        if let Some(existing) = self.by_key(&payment.idempotency_key).await? {
            let card = cards.get(&existing.card_id).await?.ok_or_else(|| {
                PaymentError::BadApproval(format!("intent {} lost its card", existing.id))
            })?;
            return Ok((existing, card));
        }
        let intent = PaymentIntent {
            id: format!("pi-{}", Uuid::new_v4().as_simple()),
            site: site.to_string(),
            amount_micro: payment.amount_micro,
            currency: payment.currency.clone(),
            purpose: payment.purpose.clone(),
            payer_ref: payment.payer_ref.clone(),
            idempotency_key: payment.idempotency_key.clone(),
            payment_method: payment.payment_method.clone(),
            state: IntentState::Held,
            card_id: String::new(),
            charge_id: None,
        };
        let card = cards
            .file(NewCard::new(
                site,
                CardKind::Approval,
                "Chief of Staff",
                format!(
                    "Chief of Staff: approve {} micro-{} for {}",
                    intent.amount_micro, intent.currency, intent.purpose
                ),
                serde_json::json!({ "intent": intent.id, "amount_micro": intent.amount_micro }),
            ))
            .await?;
        let mut intent = intent;
        intent.card_id = card.id.clone();
        sqlx::query(
            "INSERT INTO intents
             (id, site, amount_micro, currency, purpose, payer_ref, idempotency_key, payment_method, state, card_id, charge_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(&intent.id)
        .bind(&intent.site)
        .bind(intent.amount_micro)
        .bind(&intent.currency)
        .bind(&intent.purpose)
        .bind(&intent.payer_ref)
        .bind(&intent.idempotency_key)
        .bind(&intent.payment_method)
        .bind(intent.state.as_str())
        .bind(&intent.card_id)
        .execute(&self.pool)
        .await?;
        Ok((intent, card))
    }

    /// Completes a held (or failed) intent: verifies `approval_card` is an approved
    /// `Approval` naming the intent, reserves the fee against the intent's site —
    /// exactly one site, before the call — charges, and commits. A completed
    /// intent returns its receipt without charging again; a provider failure marks
    /// the intent failed and releases the fee for a later retry.
    pub async fn complete(
        &self,
        growth: &Growth,
        cards: &CardStore,
        intent_id: &str,
        approval_card: &str,
    ) -> Result<Receipt, PaymentError> {
        let intent = self
            .by_id(intent_id)
            .await?
            .ok_or_else(|| PaymentError::UnknownIntent(intent_id.to_string()))?;
        if intent.state == IntentState::Completed {
            let charge_id = intent.charge_id.clone().unwrap_or_default();
            return Ok(Receipt {
                intent: intent.id,
                charge_id,
                amount_micro: intent.amount_micro,
                fee_micro: fee_for(intent.amount_micro),
            });
        }
        let card = cards
            .get(approval_card)
            .await?
            .ok_or_else(|| PaymentError::BadApproval(format!("no card {approval_card}")))?;
        if card.kind != CardKind::Approval {
            return Err(PaymentError::BadApproval(format!(
                "card {approval_card} is not a payment approval"
            )));
        }
        if card.state != CardState::Approved {
            return Err(PaymentError::BadApproval(format!(
                "card {approval_card} is not approved"
            )));
        }
        let named = card
            .payload
            .get("intent")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if named != intent.id {
            return Err(PaymentError::BadApproval(format!(
                "card {approval_card} names a different intent"
            )));
        }
        let fee = fee_for(intent.amount_micro);
        let reservation = growth.reserve(&intent.site, fee, "payment fee").await?;
        match self.provider.charge(&intent) {
            Ok(charge_id) => {
                growth.commit(&reservation.id).await?;
                sqlx::query("UPDATE intents SET state = 'completed', charge_id = ? WHERE id = ?")
                    .bind(&charge_id)
                    .bind(&intent.id)
                    .execute(&self.pool)
                    .await?;
                Ok(Receipt {
                    intent: intent.id,
                    charge_id,
                    amount_micro: intent.amount_micro,
                    fee_micro: fee,
                })
            }
            Err(error) => {
                growth.release(&reservation.id).await?;
                sqlx::query("UPDATE intents SET state = 'failed' WHERE id = ?")
                    .bind(&intent.id)
                    .execute(&self.pool)
                    .await?;
                Err(error)
            }
        }
    }

    /// The intent `id`, or `None`.
    pub async fn get(&self, id: &str) -> Result<Option<PaymentIntent>, PaymentError> {
        self.by_id(id).await
    }

    async fn by_id(&self, id: &str) -> Result<Option<PaymentIntent>, PaymentError> {
        let row: Option<Row> = sqlx::query_as("SELECT * FROM intents WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(PaymentIntent::from))
    }

    async fn by_key(&self, key: &str) -> Result<Option<PaymentIntent>, PaymentError> {
        let row: Option<Row> = sqlx::query_as("SELECT * FROM intents WHERE idempotency_key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(PaymentIntent::from))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    site: String,
    amount_micro: i64,
    currency: String,
    purpose: String,
    payer_ref: String,
    idempotency_key: String,
    payment_method: Option<String>,
    state: String,
    card_id: String,
    charge_id: Option<String>,
}

impl From<Row> for PaymentIntent {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            site: row.site,
            amount_micro: row.amount_micro,
            currency: row.currency,
            purpose: row.purpose,
            payer_ref: row.payer_ref,
            idempotency_key: row.idempotency_key,
            payment_method: row.payment_method,
            state: IntentState::parse(&row.state),
            card_id: row.card_id,
            charge_id: row.charge_id,
        }
    }
}
