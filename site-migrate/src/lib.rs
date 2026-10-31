//! Agentrix Website Phase 9: credential-free migration with a parity gate.
//!
//! The pipeline never touches the legacy stack's credentials: it fetches public
//! pages through [`Fetch`], parses text, links, products and metadata into
//! [`Evidence`], builds a [`Candidate`] release from that evidence alone, and
//! scores the candidate against the legacy site per axis ([`ParityReport`]).
//! Below [`PARITY_THRESHOLD`] overall the candidate never reaches the LaunchGate;
//! at or above it, [`file_go_live`] files the owner's decision card with the
//! report attached, and [`MigrationParity`] carries the verdict into the gate
//! beside twin parity.

pub mod crawl;
pub mod live;
pub mod parity;

pub use crawl::{Evidence, EvidenceEntry, EvidencePage, Fetch, FileFetch, Product, crawl};
pub use live::HttpFetch;
pub use parity::{
    Candidate, MigrationParity, PARITY_THRESHOLD, ParityReport, file_go_live, migrate, score,
};
