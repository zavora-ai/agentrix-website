//! Mail intake, tiers, held drafts and the two send paths.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agentrix_site_inbox::{Card, CardKind, CardState, CardStore, NewCard, StoreError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// What sending can get wrong.
#[derive(Debug, Error)]
pub enum SendError {
    /// No draft has the id.
    #[error("no draft {0}")]
    UnknownDraft(String),
    /// The draft already sent.
    #[error("draft {0} already sent")]
    AlreadySent(String),
    /// The draft's tier needs an approval the call did not present. The send card
    /// the attempt filed, so the owner can approve and retry.
    #[error("draft {draft} needs approval for tier {tier:?}")]
    NeedsApproval {
        /// The draft.
        draft: String,
        /// Its tier.
        tier: Tier,
        /// The filed send card.
        card: Box<Card>,
    },
    /// The approval card does not approve this draft.
    #[error("approval refused: {0}")]
    BadApproval(String),
    /// The sender failed; the draft stays unsent.
    #[error("sender: {0}")]
    Sender(String),
    /// The inbox failed.
    #[error("send cards: {0}")]
    Cards(#[from] StoreError),
    /// The database failed.
    #[error("outbox: {0}")]
    Db(#[from] sqlx::Error),
    /// A payload did not serialize.
    #[error("send payload: {0}")]
    Json(#[from] serde_json::Error),
}

/// Sensitivity of a reply: each tier needs at least the authority below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    /// General correspondence: the only tier that may auto-send.
    Public,
    /// Quotes, prices, bookings, orders: needs an approved send card.
    Standard,
    /// Personal data, commitments, legal: needs an approved send card.
    Sensitive,
}

impl Tier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Standard => "standard",
            Self::Sensitive => "sensitive",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "standard" => Self::Standard,
            "sensitive" => Self::Sensitive,
            _ => Self::Public,
        }
    }
}

/// One unread mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundMail {
    /// Stable id within the mailbox.
    pub id: String,
    /// Sender address.
    pub from: String,
    /// Subject.
    pub subject: String,
    /// Plain-text body.
    pub body: String,
}

/// A held reply draft: written, tiered, carded — never sent on intake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftReply {
    /// The draft's id.
    pub id: String,
    /// The mail answered.
    pub in_reply_to: String,
    /// Recipient.
    pub to: String,
    /// Reply subject.
    pub subject: String,
    /// Reply body.
    pub body: String,
    /// Its tier.
    pub tier: Tier,
    /// Its draft card.
    pub card_id: String,
    /// Whether it sent.
    pub sent: bool,
}

/// A sent mail, as the sender recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMail {
    /// Sender-assigned id.
    pub id: String,
    /// The draft it sent.
    pub draft: String,
    /// Recipient.
    pub to: String,
    /// Subject.
    pub subject: String,
    /// Body.
    pub body: String,
}

/// Reads unread mail. Live IMAP and the fixture file reader implement the trait.
pub trait Mailbox: Send + Sync {
    /// The unread mail, oldest first.
    fn unread(&self) -> Result<Vec<InboundMail>, SendError>;
}

/// Reads `*.json` mails from `dir`, sorted by file name: the offline mailbox.
pub struct FileMailbox {
    dir: PathBuf,
}

impl FileMailbox {
    /// A mailbox over `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl Mailbox for FileMailbox {
    fn unread(&self) -> Result<Vec<InboundMail>, SendError> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map_err(|e| SendError::Sender(format!("mailbox {}: {e}", self.dir.display())))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|path| {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| SendError::Sender(format!("{}: {e}", path.display())))?;
                serde_json::from_str(&raw).map_err(SendError::Json)
            })
            .collect()
    }
}

/// Sends one mail. Live SMTP and the recording stub implement the trait.
pub trait Sender: Send + Sync {
    /// Sends the mail, returning the sent record.
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<SentMail, SendError>;
}

/// Records sent mail in memory instead of sending: the test sender.
#[derive(Debug, Default)]
pub struct LogSender {
    log: Mutex<Vec<SentMail>>,
}

impl LogSender {
    /// The mail recorded so far.
    pub fn sent(&self) -> Vec<SentMail> {
        self.log.lock().expect("sender log").clone()
    }
}

impl Sender for LogSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<SentMail, SendError> {
        let mail = SentMail {
            id: format!("sent-{}", Uuid::new_v4().as_simple()),
            draft: String::new(),
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
        };
        self.log.lock().expect("sender log").push(mail.clone());
        Ok(mail)
    }
}

/// Classifies a reply's tier from its signals. v1 heuristics, documented and
/// deterministic: personal data, commitments and legal words are Sensitive;
/// commercial words are Standard; everything else is Public.
pub fn classify(subject: &str, body: &str) -> Tier {
    let text = format!("{subject}\n{body}").to_lowercase();
    let sensitive = [
        "contract",
        "password",
        "id number",
        "passport",
        "bank",
        "account number",
        "lawsuit",
        "court",
        "legal",
        "salary",
        "medical",
    ];
    if sensitive.iter().any(|word| text.contains(word)) || has_contact_data(&text) {
        return Tier::Sensitive;
    }
    let standard = [
        "quote", "price", "kes", "invoice", "discount", "book", "order", "refund", "payment",
        "delivery",
    ];
    if standard.iter().any(|word| text.contains(word)) {
        return Tier::Standard;
    }
    Tier::Public
}

/// An email address or a long digit run: personal data until proven otherwise.
fn has_contact_data(text: &str) -> bool {
    if text.contains('@') {
        return true;
    }
    let mut run = 0_usize;
    for c in text.chars() {
        if c.is_ascii_digit() {
            run += 1;
            if run >= 7 {
                return true;
            }
        } else if !matches!(c, ' ' | '-' | '+' | '(' | ')') {
            run = 0;
        }
    }
    false
}

/// The held drafts in one SQLite database, sending through one [`Sender`].
pub struct Outbox {
    pool: sqlx::sqlite::SqlitePool,
    sender: Arc<dyn Sender>,
}

impl Outbox {
    /// Opens (creating and migrating) the outbox at `url`, sending through `sender`.
    pub async fn open(url: &str, sender: Arc<dyn Sender>) -> Result<Self, SendError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| SendError::Sender(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS drafts (
                id TEXT PRIMARY KEY,
                in_reply_to TEXT NOT NULL,
                recipient TEXT NOT NULL,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                tier TEXT NOT NULL,
                card_id TEXT NOT NULL,
                sent INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool, sender })
    }

    /// The Chief of Staff intake: drafts one held reply per unread mail, files each
    /// draft's card, and sends nothing. Returns the drafts filed.
    pub async fn intake(
        &self,
        site: &str,
        cards: &CardStore,
        mailbox: &dyn Mailbox,
    ) -> Result<Vec<DraftReply>, SendError> {
        let mut filed = Vec::new();
        for mail in mailbox.unread()? {
            let subject = format!("Re: {}", mail.subject);
            let body = format!(
                "Hello,\n\nThank you for writing about \"{}\". \
                 This is a held draft from the site's team — a person reviews every reply before it sends.\n\nQuoting your note: {}\n",
                mail.subject,
                mail.body.lines().next().unwrap_or("")
            );
            let tier = classify(&subject, &format!("{}\n{body}", mail.body));
            let draft = DraftReply {
                id: format!("draft-{}", Uuid::new_v4().as_simple()),
                in_reply_to: mail.id.clone(),
                to: mail.from.clone(),
                subject: subject.clone(),
                body: body.clone(),
                tier,
                card_id: String::new(),
                sent: false,
            };
            let payload = serde_json::json!({
                "draft": draft.id,
                "to": draft.to,
                "subject": draft.subject,
                "body": draft.body,
                "tier": draft.tier.as_str(),
            });
            let card = cards
                .file(NewCard::new(
                    site,
                    CardKind::Draft,
                    "Chief of Staff",
                    format!("Chief of Staff: reply to {} ({:?})", mail.from, tier),
                    payload,
                ))
                .await?;
            let mut draft = draft;
            draft.card_id = card.id;
            sqlx::query(
                "INSERT INTO drafts (id, in_reply_to, recipient, subject, body, tier, card_id, sent)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
            )
            .bind(&draft.id)
            .bind(&draft.in_reply_to)
            .bind(&draft.to)
            .bind(&draft.subject)
            .bind(&draft.body)
            .bind(draft.tier.as_str())
            .bind(&draft.card_id)
            .execute(&self.pool)
            .await?;
            filed.push(draft);
        }
        Ok(filed)
    }

    /// Sends a Public draft with no card. Anything above Public is refused — and
    /// the refusal files the send card the draft needs, so the attempt is carded,
    /// never dropped.
    pub async fn send_public(
        &self,
        site: &str,
        cards: &CardStore,
        draft_id: &str,
    ) -> Result<SentMail, SendError> {
        let draft = self.draft(draft_id).await?;
        if draft.sent {
            return Err(SendError::AlreadySent(draft_id.to_string()));
        }
        if draft.tier != Tier::Public {
            let card = cards
                .file(NewCard::new(
                    site,
                    CardKind::Approval,
                    "Chief of Staff",
                    format!(
                        "Chief of Staff: approve {:?} reply to {}",
                        draft.tier, draft.to
                    ),
                    serde_json::json!({ "draft": draft.id, "tier": draft.tier.as_str() }),
                ))
                .await?;
            return Err(SendError::NeedsApproval {
                draft: draft.id,
                tier: draft.tier,
                card: Box::new(card),
            });
        }
        self.deliver(&draft).await
    }

    /// Sends any draft with an approved send card naming it: the card must be an
    /// approved `Approval` whose payload names this draft. The only path for
    /// tier-above sends — without that card, they cannot go.
    pub async fn send_approved(
        &self,
        cards: &CardStore,
        draft_id: &str,
        approval_card: &str,
    ) -> Result<SentMail, SendError> {
        let draft = self.draft(draft_id).await?;
        if draft.sent {
            return Err(SendError::AlreadySent(draft_id.to_string()));
        }
        let card = cards
            .get(approval_card)
            .await?
            .ok_or_else(|| SendError::BadApproval(format!("no card {approval_card}")))?;
        if card.kind != CardKind::Approval {
            return Err(SendError::BadApproval(format!(
                "card {approval_card} is not a send approval"
            )));
        }
        if card.state != CardState::Approved {
            return Err(SendError::BadApproval(format!(
                "card {approval_card} is not approved"
            )));
        }
        let named = card
            .payload
            .get("draft")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if named != draft.id {
            return Err(SendError::BadApproval(format!(
                "card {approval_card} names a different draft"
            )));
        }
        self.deliver(&draft).await
    }

    /// Every unsent draft, oldest first.
    pub async fn pending(&self) -> Result<Vec<DraftReply>, SendError> {
        let rows: Vec<Row> = sqlx::query_as("SELECT * FROM drafts WHERE sent = 0 ORDER BY rowid")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(DraftReply::from).collect())
    }

    async fn draft(&self, id: &str) -> Result<DraftReply, SendError> {
        let row: Option<Row> = sqlx::query_as("SELECT * FROM drafts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(DraftReply::from)
            .ok_or_else(|| SendError::UnknownDraft(id.to_string()))
    }

    async fn deliver(&self, draft: &DraftReply) -> Result<SentMail, SendError> {
        let mut sent = self.sender.send(&draft.to, &draft.subject, &draft.body)?;
        sent.draft = draft.id.clone();
        sqlx::query("UPDATE drafts SET sent = 1 WHERE id = ?")
            .bind(&draft.id)
            .execute(&self.pool)
            .await?;
        Ok(sent)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    in_reply_to: String,
    recipient: String,
    subject: String,
    body: String,
    tier: String,
    card_id: String,
    sent: i64,
}

impl From<Row> for DraftReply {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            in_reply_to: row.in_reply_to,
            to: row.recipient,
            subject: row.subject,
            body: row.body,
            tier: Tier::parse(&row.tier),
            card_id: row.card_id,
            sent: row.sent != 0,
        }
    }
}
