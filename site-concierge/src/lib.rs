//! Agentrix Website Phase 4: the concierge and its signals.
//!
//! Each site owns a [`Corpus`] of text chunks keyed by `(site, chunk)`: every
//! retrieval filters by site in SQL, so a site structurally cannot see another's
//! chunks. The concierge answers a [`Visitor`]'s question extractively from the
//! top chunks and cites each quote ([`Answer`]); anything it cannot ground
//! becomes a gap signal instead of a guess. [`signals`] turns conversation events
//! into the four typed signals — content gaps, review sightings, lead captures
//! and support escalations — that wake the charter jobs.

pub mod answer;
pub mod corpus;
pub mod signals;
pub mod visitor;

pub use answer::{Answer, Citation, answer};
pub use corpus::{Chunk, Corpus, ScoredChunk, retrieve};
pub use signals::{EscalationSignal, LeadSignal, ReviewSignal, Signal, VisitorEvent, emit};
pub use visitor::Visitor;
