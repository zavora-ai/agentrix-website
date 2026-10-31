//! Signal emitters: conversation events in, typed wake signals out.
//!
//! Four emitters cover the four signal kinds: unanswered questions become content
//! gaps, seen reviews become reputation work, shared contact details become leads,
//! and handoff asks or repeated misses become support escalations. Emitting is
//! pure over the event and its context — the kernel decides what wakes.

use agentrix_site_inbox::GapSignal;
use serde::{Deserialize, Serialize};

/// Something that happened in a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum VisitorEvent {
    /// The visitor asked; the answer may have missed.
    Message {
        /// The question.
        text: String,
        /// Whether the concierge grounded its answer.
        grounded: bool,
        /// Consecutive misses including this one.
        miss_streak: u32,
    },
    /// A review was seen on a platform.
    ReviewSeen {
        /// Where.
        platform: String,
        /// Stars, 1 to 5.
        rating: u8,
        /// Quoted excerpt.
        excerpt: String,
    },
    /// The visitor shared a way back.
    ContactShared {
        /// The channel, such as `email` or `phone`.
        channel: String,
        /// Opaque reference, never the address itself in the signal.
        contact_ref: String,
    },
    /// The visitor asked for a human.
    HandoffAsked {
        /// Why, in the visitor's words.
        reason: String,
    },
}

/// A review sighting for the Reputation Manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSignal {
    /// The platform.
    pub platform: String,
    /// Stars, 1 to 5.
    pub rating: u8,
    /// Quoted excerpt.
    pub excerpt: String,
}

/// A captured lead for the Sales Rep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadSignal {
    /// The channel the details came over.
    pub channel: String,
    /// Opaque contact reference.
    pub contact_ref: String,
}

/// A support escalation for the Chief of Staff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationSignal {
    /// Why it escalated.
    pub reason: String,
}

/// One of the four typed signals. Content gaps reuse the inbox's [`GapSignal`],
/// which the Content Editor target already drafts from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signal {
    /// An unanswered question.
    ContentGap(GapSignal),
    /// A seen review.
    ReviewSeen(ReviewSignal),
    /// A captured lead.
    LeadCaptured(LeadSignal),
    /// A support escalation.
    SupportEscalation(EscalationSignal),
}

/// Emits the signals `event` warrants: a miss becomes a content gap; three misses
/// running also escalate; reviews, contacts and handoffs emit directly. Answered
/// questions emit nothing.
pub fn emit(event: &VisitorEvent) -> Vec<Signal> {
    match event {
        VisitorEvent::Message {
            text,
            grounded: true,
            ..
        } => {
            let _ = text;
            Vec::new()
        }
        VisitorEvent::Message {
            text,
            grounded: false,
            miss_streak,
        } => {
            let mut signals = vec![Signal::ContentGap(GapSignal {
                id: format!("gap-{}", uuid::Uuid::new_v4().as_simple()),
                topic: text.clone(),
                question_count: 1,
                samples: vec![text.clone()],
            })];
            if *miss_streak >= 3 {
                signals.push(Signal::SupportEscalation(EscalationSignal {
                    reason: format!("three unanswered questions in a row, latest: {text}"),
                }));
            }
            signals
        }
        VisitorEvent::ReviewSeen {
            platform,
            rating,
            excerpt,
        } => {
            vec![Signal::ReviewSeen(ReviewSignal {
                platform: platform.clone(),
                rating: *rating,
                excerpt: excerpt.clone(),
            })]
        }
        VisitorEvent::ContactShared {
            channel,
            contact_ref,
        } => {
            vec![Signal::LeadCaptured(LeadSignal {
                channel: channel.clone(),
                contact_ref: contact_ref.clone(),
            })]
        }
        VisitorEvent::HandoffAsked { reason } => {
            vec![Signal::SupportEscalation(EscalationSignal {
                reason: reason.clone(),
            })]
        }
    }
}
