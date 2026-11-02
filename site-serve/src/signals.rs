//! Signal intake: visitor events and review webhooks in, kernel wakes out.
//!
//! [`translate`] maps the concierge's product signals onto `awp_types` signal
//! kinds, and [`emit_event`] files them through the kernel-attached
//! [`SignalSink`](adk_kernel::charter::SignalSink), whose router submit wakes
//! the Content, Reviews and Support triggers. [`intake_routes`] exposes the two
//! production producers: visitor chat and the review webhook.

use std::collections::BTreeMap;
use std::sync::Arc;

use adk_kernel::charter::{SignalOutcome, SignalSink};
use agentrix_site_concierge::{
    Corpus, Visitor, VisitorEvent, answer, emit, signals::Signal as ProductSignal,
};
use awp_types::{Bounded, Channel, ContactRef, LeadRef, ReasonCode, Signal, SignalKind};
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};

/// Translates a product signal into its kernel kind. The kind names match the
/// [`Wake::Signal`](adk_kernel::Wake::Signal) triggers of the Content, Reviews
/// and Support jobs, so emitting through an attached sink wakes them.
pub fn translate(signal: &ProductSignal, visitor: &str) -> SignalKind {
    match signal {
        ProductSignal::ContentGap(gap) => SignalKind::ContentGap {
            topic: Bounded::truncated(&gap.topic, 120),
            question_count: gap.question_count,
            samples: gap
                .samples
                .iter()
                .take(5)
                .map(|sample| Bounded::truncated(sample, 200))
                .collect(),
        },
        ProductSignal::ReviewSeen(review) => SignalKind::ReviewSeen {
            platform: review.platform.clone(),
            rating: review.rating.min(5),
            excerpt: Bounded::truncated(&review.excerpt, 500),
        },
        ProductSignal::LeadCaptured(lead) => SignalKind::LeadCaptured {
            lead_ref: LeadRef(lead.contact_ref.clone()),
            source: Channel::from(lead.channel.as_str()),
            consent: lead
                .consent
                .iter()
                .filter_map(|receipt| uuid::Uuid::parse_str(receipt).ok())
                .map(awp_types::ConsentReceiptRef)
                .collect(),
            answers: BTreeMap::new(),
        },
        ProductSignal::SupportEscalation(escalation) => SignalKind::SupportEscalation {
            reason: ReasonCode(slug(&escalation.reason)),
            contact_ref: ContactRef(format!("visitor:{visitor}")),
        },
    }
}

/// A snake_case reason code from free text: lowercase alphanumerics, the rest
/// underscores, capped at 80 characters.
fn slug(reason: &str) -> String {
    reason
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>()
        .split('_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Files every signal `event` warrants through `sink`, attributed to the
/// site's concierge agent. With a kernel-attached sink each file also submits
/// the wake that starts the signal's job.
pub async fn emit_event(
    sink: &SignalSink,
    site: &str,
    visitor: &str,
    event: &VisitorEvent,
) -> Result<Vec<SignalOutcome>, adk_kernel::KernelError> {
    let mut outcomes = Vec::new();
    for product in emit(event) {
        let signal = Signal {
            id: uuid::Uuid::new_v4(),
            kind: translate(&product, visitor),
            emitted_by: format!("urn:adk:agent:{site}/concierge"),
            emitted_at: chrono::Utc::now(),
        };
        outcomes.push(sink.emit(signal).await?);
    }
    Ok(outcomes)
}

/// A review sighting for the intake webhook: the platform's push, normalized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIntake {
    /// Where the review appeared.
    pub platform: String,
    /// Stars, 1 to 5.
    pub rating: u8,
    /// Quoted excerpt.
    pub excerpt: String,
}

/// Files a review sighting through `sink` as the review watcher.
pub async fn emit_review(
    sink: &SignalSink,
    site: &str,
    intake: &ReviewIntake,
) -> Result<SignalOutcome, adk_kernel::KernelError> {
    sink.emit(Signal {
        id: uuid::Uuid::new_v4(),
        kind: SignalKind::ReviewSeen {
            platform: intake.platform.clone(),
            rating: intake.rating.min(5),
            excerpt: Bounded::truncated(&intake.excerpt, 500),
        },
        emitted_by: format!("urn:adk:agent:{site}/review-watcher"),
        emitted_at: chrono::Utc::now(),
    })
    .await
}

/// The intake state: the corpus for answers, the sink for wakes.
#[derive(Clone)]
pub struct Intake {
    /// The site served.
    pub site: String,
    /// Chunks answers quote.
    pub corpus: Arc<Corpus>,
    /// The kernel-attached sink; every file wakes its job.
    pub sink: SignalSink,
}

/// A visitor chat turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitorMessage {
    /// The visitor's id, or empty at first contact.
    pub visitor: String,
    /// The question.
    pub question: String,
    /// Consecutive misses before this turn; three escalate.
    #[serde(default)]
    pub miss_streak: u32,
}

/// The answered turn: quotes or an honest miss, plus what it woke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitorReply {
    /// The answer text.
    pub text: String,
    /// Whether the corpus grounded it.
    pub grounded: bool,
    /// The visitor, minted when the turn did not name one.
    pub visitor: String,
    /// How many signals the turn filed.
    pub signals: usize,
}

/// Answers a visitor turn and files its signals: a miss becomes a content gap,
/// three running also escalate, shared contacts become leads.
async fn visitor_message(
    State(intake): State<Arc<IntakeState>>,
    Json(turn): Json<VisitorMessage>,
) -> Result<Json<VisitorReply>, (StatusCode, String)> {
    let intake = &intake.visitor;
    let visitor = if turn.visitor.is_empty() {
        Visitor::mint(&intake.site)
    } else {
        Visitor {
            site: intake.site.clone(),
            id: turn.visitor.clone(),
        }
    };
    let reply = answer(&intake.corpus, &intake.site, &visitor, &turn.question)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let streak = if reply.grounded {
        0
    } else {
        turn.miss_streak.saturating_add(1)
    };
    let event = VisitorEvent::Message {
        text: turn.question.clone(),
        grounded: reply.grounded,
        miss_streak: streak,
    };
    let filed = emit_event(&intake.sink, &intake.site, &visitor.id, &event)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(VisitorReply {
        text: reply.text,
        grounded: reply.grounded,
        visitor: visitor.id,
        signals: filed.len(),
    }))
}

/// The review webhook: files the sighting and wakes the Reputation Manager.
async fn review_webhook(
    State(intake): State<Arc<IntakeState>>,
    Json(sighting): Json<ReviewIntake>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let intake = &intake.visitor;
    let outcome = emit_review(&intake.sink, &intake.site, &sighting)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "filed": format!("{outcome:?}") })))
}

/// What the migration intake touches.
#[derive(Clone)]
pub struct MigrateState {
    /// The site served.
    pub site: String,
    /// Go-live cards.
    pub cards: Arc<agentrix_site_inbox::CardStore>,
}

/// A migration request: public URLs in, no credentials needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateIntake {
    /// Up to 20 public page URLs to crawl.
    pub urls: Vec<String>,
}

/// The migration door: crawls the public URLs, scores parity, and files the
/// go-live card for approval. Credential-free by construction — the fetch
/// carries no auth.
async fn migrate_webhook(
    State(intake): State<Arc<IntakeState>>,
    Json(request): Json<MigrateIntake>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let migrate = &intake.migrate;
    if request.urls.is_empty() || request.urls.len() > 20 {
        return Err((StatusCode::BAD_REQUEST, "send 1 to 20 urls".into()));
    }
    // The crawl is blocking I/O over a synchronous client: running it on a
    // runtime thread panics, so it gets its own blocking thread.
    let urls = request.urls.clone();
    let (candidate, report) = tokio::task::spawn_blocking(move || {
        let fetch = agentrix_site_migrate::HttpFetch::new()?;
        let evidence = agentrix_site_migrate::crawl(&fetch, &urls)?;
        Ok::<_, agentrix_site_migrate::crawl::CrawlError>(agentrix_site_migrate::migrate(&evidence))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    let card =
        agentrix_site_migrate::file_go_live(&migrate.cards, &migrate.site, &candidate, &report)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({
        "parity": report.overall,
        "card": card.id,
        "digest": candidate.release.digest,
    })))
}

/// Everything the intake routes touch.
#[derive(Clone)]
pub struct IntakeState {
    /// Visitor chat and review sightings.
    pub visitor: Arc<Intake>,
    /// The migration door.
    pub migrate: Arc<MigrateState>,
}

/// The intake routes: visitor chat, the review webhook and the migration door.
pub fn intake_routes(state: Arc<IntakeState>) -> Router {
    Router::new()
        .route("/api/visitor/message", post(visitor_message))
        .route("/api/intake/review", post(review_webhook))
        .route("/api/intake/migrate", post(migrate_webhook))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentrix_site_inbox::GapSignal;

    #[test]
    fn transitions_carry_the_trigger_kind_names() {
        let gap = ProductSignal::ContentGap(GapSignal {
            id: "g1".into(),
            topic: "weekend hours".into(),
            question_count: 2,
            samples: vec!["open saturday?".into()],
        });
        assert_eq!(translate(&gap, "v").name(), "content_gap");
        let review = ProductSignal::ReviewSeen(agentrix_site_concierge::ReviewSignal {
            platform: "mastodon".into(),
            rating: 9,
            excerpt: "great".into(),
        });
        let SignalKind::ReviewSeen { rating, .. } = translate(&review, "v") else {
            panic!("must translate to review_seen");
        };
        assert_eq!(rating, 5);
        let lead = ProductSignal::LeadCaptured(agentrix_site_concierge::LeadSignal {
            channel: "carrier-pigeon".into(),
            contact_ref: "c1".into(),
            consent: Vec::new(),
        });
        assert_eq!(translate(&lead, "v").name(), "lead_captured");
        let escalation =
            ProductSignal::SupportEscalation(agentrix_site_concierge::EscalationSignal {
                reason: "Asked for a HUMAN twice!".into(),
            });
        let SignalKind::SupportEscalation {
            reason,
            contact_ref,
            ..
        } = translate(&escalation, "v9")
        else {
            panic!("must translate to support_escalation");
        };
        assert_eq!(reason, ReasonCode("asked_for_a_human_twice".into()));
        assert_eq!(contact_ref, ContactRef("visitor:v9".into()));
    }
}
