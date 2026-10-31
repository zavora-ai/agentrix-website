//! Agentrix Website Phase 3: publish plus rollback.
//!
//! Admitted releases live in a SQLite [`ReleaseStore`] with one `current` pointer
//! per site and an audit log of every admission, publish and refusal. The
//! [`LaunchGate`] admits a candidate only when four checks pass — integrity,
//! parity, an approved release card, and policy — and [`ReleaseStore::set_current`]
//! moves the pointer, which is what publish and rollback both are. The edge's
//! [`Current`](agentrix_site_edge::Current) pointer follows the store, so both
//! audiences move together from the next request on.

pub mod gate;
pub mod store;

pub use gate::{GateRefusal, LaunchGate, ParityCheck, TwinParity};
pub use store::{ReleaseEvent, ReleaseMeta, ReleaseStore};
