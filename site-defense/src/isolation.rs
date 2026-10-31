//! Tenant isolation at the product run layer: every cross-store read and serve
//! names its acting site, and anything reaching for another site's resource is
//! denied with an audit line. Per-site stores scope the honest paths; this closes
//! the direct ones — a card id or release digest from another site.

use agentrix_site_edge::Release;
use agentrix_site_inbox::{Card, CardStore};
use agentrix_site_release::ReleaseStore;
use thiserror::Error;

use crate::guard::{Freeze, GuardError};

/// What isolation can get wrong.
#[derive(Debug, Error)]
pub enum Denial {
    /// The resource belongs to another site: denied and audited.
    #[error("site {acting} may not {action} site {owner}'s {resource}")]
    CrossSite {
        /// The acting site.
        acting: String,
        /// The owning site.
        owner: String,
        /// The resource id.
        resource: String,
        /// What was attempted.
        action: String,
    },
    /// No such resource.
    #[error("no {0}")]
    Missing(String),
    /// A store failed.
    #[error("isolation: {0}")]
    Store(String),
}

impl From<GuardError> for Denial {
    fn from(error: GuardError) -> Self {
        Self::Store(error.to_string())
    }
}

/// The isolation check over the shared product stores, auditing to `freeze`.
pub struct Isolation<'a> {
    freeze: &'a Freeze,
    cards: &'a CardStore,
    releases: &'a ReleaseStore,
}

impl<'a> Isolation<'a> {
    /// An isolation check over the three shared stores.
    pub fn new(freeze: &'a Freeze, cards: &'a CardStore, releases: &'a ReleaseStore) -> Self {
        Self {
            freeze,
            cards,
            releases,
        }
    }

    /// Reads card `id` for `acting_site`: another site's card is denied and the
    /// denial is audited under the acting site.
    pub async fn read_card(&self, acting_site: &str, id: &str) -> Result<Card, Denial> {
        let card = self
            .cards
            .get(id)
            .await
            .map_err(|e| Denial::Store(e.to_string()))?
            .ok_or_else(|| Denial::Missing(format!("card {id}")))?;
        if card.site != acting_site {
            let detail = format!("read card {id} owned by site {}", card.site);
            self.freeze.audit(acting_site, "denied", &detail).await?;
            return Err(Denial::CrossSite {
                acting: acting_site.to_string(),
                owner: card.site.clone(),
                resource: id.to_string(),
                action: "read".into(),
            });
        }
        Ok(card)
    }

    /// Serves release `digest` for `acting_site`: another site's release is denied
    /// and audited. Content addresses are global, but serving is per site.
    pub async fn serve_release(&self, acting_site: &str, digest: &str) -> Result<Release, Denial> {
        let release = self
            .releases
            .get(digest)
            .await
            .map_err(|e| Denial::Store(e.to_string()))?
            .ok_or_else(|| Denial::Missing(format!("release {digest}")))?;
        let owner = self
            .releases
            .owner(digest)
            .await
            .map_err(|e| Denial::Store(e.to_string()))?
            .unwrap_or_default();
        if owner != acting_site {
            let detail = format!("serve release {} owned by site {owner}", short(digest));
            self.freeze.audit(acting_site, "denied", &detail).await?;
            return Err(Denial::CrossSite {
                acting: acting_site.to_string(),
                owner,
                resource: digest.to_string(),
                action: "serve".into(),
            });
        }
        Ok(release)
    }
}

fn short(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}
