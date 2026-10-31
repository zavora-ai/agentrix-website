//! Agentrix Website Phase 2: the unified inbox.
//!
//! Every pending human decision is one [`Card`] in a SQLite [`CardStore`]: draft
//! approvals, incidents, digests and generic approvals. A card is decided exactly
//! once — a second decision is refused — and an approved card's effect runs through
//! an executor the caller supplies, retried by [`CardStore::recover`] until it
//! reports success, so a crash between decision and effect loses neither. Cards
//! name jobs by charter role ([`role_of`]), never by job id. The [`editor`]
//! module turns content-gap signals into held drafts; nothing in this crate can
//! publish.

pub mod cards;
pub mod editor;
pub mod roles;

pub use cards::{Card, CardKind, CardState, CardStore, Decision, Execute, NewCard, StoreError};
pub use editor::{Draft, GapSignal, draft_from_gaps};
pub use roles::role_of;
