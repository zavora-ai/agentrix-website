//! Visitor principals: random, server-issued ids, never derived from the network.
//!
//! A visitor is `visitor-<uuid>` minted at first contact. The id carries no IP,
//! no fingerprint, no account: linking it to a person is a later, consented act
//! (Requirement 5, upstream #1283), not something the shape gives away.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Someone the concierge talks to: a site plus a random visitor id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visitor {
    /// The site visited.
    pub site: String,
    /// `visitor-<uuid>`, minted at first contact.
    pub id: String,
}

impl Visitor {
    /// Mints a visitor on `site`.
    pub fn mint(site: impl Into<String>) -> Self {
        Self {
            site: site.into(),
            id: format!("visitor-{}", Uuid::new_v4().as_simple()),
        }
    }
}
