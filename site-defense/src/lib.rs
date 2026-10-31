//! Agentrix Website Phase 8: self-defense plus costs.
//!
//! The Security Guard target probes the served release model-free — digest
//! integrity, admission, abuse — and the Facilities Manager target watches
//! certificates, domains, backups and advisories ([`probe`]). Every target run
//! passes the [`Guard`]: frozen sites skip, quiet hours defer, and every skip,
//! deferral, denial and incident lands in the audit log. [`Isolation`] denies
//! cross-site reads and serves with an audit trail, closing the loop the
//! per-site stores leave open.

pub mod guard;
pub mod isolation;
pub mod probe;

pub use guard::{Freeze, Guard, GuardDecision, GuardError, QuietHours};
pub use isolation::{Denial, Isolation};
pub use probe::{
    AbuseHit, Incident, ProbeError, ProbeReport, Proposal, WatchInput, file_incidents,
    file_proposals, probe, watch,
};
