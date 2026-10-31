//! The Content Editor target: content-gap signals in, held drafts out.
//!
//! [`draft_from_gaps`] turns a batch of gap signals into one [`Draft`] and files it
//! as a draft-approval card naming the Content Editor role. Draft-first is
//! structural: this module has no publish path — a draft applies only when the
//! owner approves its card and the caller's executor runs it.

use serde::{Deserialize, Serialize};

use crate::cards::{Card, CardKind, CardStore, NewCard, StoreError};
use crate::roles::role_of;

/// A question the site could not answer: one gap the editor should close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapSignal {
    /// The gap's id.
    pub id: String,
    /// What visitors asked about.
    pub topic: String,
    /// How many times it came up.
    pub question_count: u32,
    /// Sample visitor questions.
    pub samples: Vec<String>,
}

/// A held draft: proposed facts for one page, waiting on its card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    /// The draft's id (the card's idempotency key scope).
    pub id: String,
    /// The page the draft improves.
    pub page_slug: String,
    /// Proposed `(fact id, text)` pairs.
    pub facts: Vec<(String, String)>,
    /// Why the draft exists, in the editor's words.
    pub rationale: String,
    /// The gap signals behind it.
    pub gap_ids: Vec<String>,
}

/// Drafts one held draft for `page_slug` from `gaps` and files its approval card.
/// Empty gaps file nothing and return `None`. The card's role is always the
/// Content Editor; its payload is the draft.
pub async fn draft_from_gaps(
    store: &CardStore,
    site: &str,
    page_slug: &str,
    gaps: &[GapSignal],
) -> Result<Option<(Draft, Card)>, StoreError> {
    if gaps.is_empty() {
        return Ok(None);
    }
    let role = role_of("freshness").expect("the freshness role exists");
    let topics: Vec<&str> = gaps.iter().map(|gap| gap.topic.as_str()).collect();
    let draft = Draft {
        id: format!("draft-{}", uuid::Uuid::new_v4().as_simple()),
        page_slug: page_slug.to_string(),
        facts: gaps
            .iter()
            .map(|gap| {
                let slug: String = gap
                    .topic
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect();
                (
                    format!("gap-{slug}"),
                    format!("{} ({} visitor questions).", gap.topic, gap.question_count),
                )
            })
            .collect(),
        rationale: format!("Visitors asked about {}.", topics.join(", ")),
        gap_ids: gaps.iter().map(|gap| gap.id.clone()).collect(),
    };
    let title = format!(
        "Content Editor: draft for `{page_slug}` from {} gap(s)",
        gaps.len()
    );
    let payload = serde_json::to_value(&draft)?;
    let card = store
        .file(NewCard::new(site, CardKind::Draft, role, title, payload))
        .await?;
    Ok(Some((draft, card)))
}
