//! The LaunchGate: a candidate becomes servable only when four checks pass —
//! integrity, parity, an approved release card, and policy. Every refusal is
//! audited with its reason; silence never admits.

use agentrix_site_edge::{Release, build_release, digest_match};
use agentrix_site_inbox::{CardKind, CardState, CardStore};
use thiserror::Error;

use crate::store::ReleaseStore;

/// Why the gate refused a candidate.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GateRefusal {
    /// The digest does not recompute from the bundle.
    #[error("integrity: the digest does not match the bundle")]
    Integrity,
    /// A parity check failed.
    #[error("parity: {0}")]
    Parity(String),
    /// No approved release card names the digest.
    #[error("approval: {0}")]
    Approval(String),
    /// Policy refuses: the site is frozen.
    #[error("policy: the site is frozen")]
    Frozen,
    /// A store failed mid-admission.
    #[error("gate stores: {0}")]
    Stores(String),
}

/// A parity check over a candidate: twin-vs-human today, migration parity (Phase
/// 9) plugs in beside it behind the same trait.
pub trait ParityCheck: Send + Sync {
    /// The check's name for the audit log.
    fn name(&self) -> &'static str;
    /// Passes, or the reason it does not.
    fn check(&self, candidate: &Release) -> Result<(), String>;
}

/// Twin parity: every twin carries the same facts as its human page.
#[derive(Debug, Default)]
pub struct TwinParity;

impl ParityCheck for TwinParity {
    fn name(&self) -> &'static str {
        "twin-parity"
    }

    fn check(&self, candidate: &Release) -> Result<(), String> {
        digest_match(candidate).map_err(|problems| problems.join("; "))
    }
}

/// The gate over one site: frozen refuses everything, otherwise the checks run in
/// order and the first failure refuses with its reason.
pub struct LaunchGate {
    site: String,
    frozen: bool,
    parity: Vec<Box<dyn ParityCheck>>,
}

impl LaunchGate {
    /// A gate for `site` with twin parity and no freeze.
    pub fn new(site: impl Into<String>) -> Self {
        Self {
            site: site.into(),
            frozen: false,
            parity: vec![Box::new(TwinParity)],
        }
    }

    /// A frozen gate refuses every candidate with [`GateRefusal::Frozen`].
    pub fn frozen(mut self) -> Self {
        self.frozen = true;
        self
    }

    /// Adds a parity check beside twin parity.
    pub fn with_parity(mut self, check: Box<dyn ParityCheck>) -> Self {
        self.parity.push(check);
        self
    }

    /// Admits `candidate` when integrity, every parity check, the approval card,
    /// and policy all pass. The caller presents `approval_card`: the gate verifies
    /// it is an approved `Approval` card whose payload names the digest. On
    /// success the release is stored as admitted and served as current; on refusal
    /// the reason is audited and nothing moves.
    pub async fn admit(
        &self,
        store: &ReleaseStore,
        cards: &CardStore,
        candidate: &Release,
        approval_card: &str,
    ) -> Result<Release, GateRefusal> {
        if self.frozen {
            return self
                .refuse(store, Some(&candidate.digest), GateRefusal::Frozen)
                .await;
        }
        let rebuilt = build_release(
            candidate.pages.clone(),
            &candidate.provenance.template,
            candidate.provenance.sources.clone(),
        );
        if rebuilt.digest != candidate.digest {
            return self
                .refuse(store, Some(&candidate.digest), GateRefusal::Integrity)
                .await;
        }
        for check in &self.parity {
            if let Err(reason) = check.check(candidate) {
                let refusal = GateRefusal::Parity(format!("{}: {reason}", check.name()));
                return self.refuse(store, Some(&candidate.digest), refusal).await;
            }
        }
        if let Err(refusal) = self
            .verify_approval(cards, approval_card, &candidate.digest)
            .await
        {
            return self.refuse(store, Some(&candidate.digest), refusal).await;
        }
        store
            .admit(candidate)
            .await
            .map_err(|e| GateRefusal::Stores(e.to_string()))?;
        let served = store
            .set_current(&self.site, &candidate.digest)
            .await
            .map_err(|e| GateRefusal::Stores(e.to_string()))?;
        Ok(served)
    }

    /// The approval card exists, is an approved `Approval`, and names the digest —
    /// anything else refuses with what was wrong with it.
    async fn verify_approval(
        &self,
        cards: &CardStore,
        approval_card: &str,
        digest: &str,
    ) -> Result<(), GateRefusal> {
        let card = cards
            .get(approval_card)
            .await
            .map_err(|e| GateRefusal::Stores(e.to_string()))?
            .ok_or_else(|| GateRefusal::Approval(format!("no card {approval_card}")))?;
        if card.kind != CardKind::Approval {
            return Err(GateRefusal::Approval(format!(
                "card {approval_card} is not a release approval"
            )));
        }
        if card.state != CardState::Approved {
            return Err(GateRefusal::Approval(format!(
                "card {approval_card} is not approved"
            )));
        }
        let named = card
            .payload
            .get("release")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if named != digest {
            return Err(GateRefusal::Approval(format!(
                "card {approval_card} names a different release"
            )));
        }
        Ok(())
    }

    async fn refuse(
        &self,
        store: &ReleaseStore,
        digest: Option<&str>,
        refusal: GateRefusal,
    ) -> Result<Release, GateRefusal> {
        let _ = store.log_refusal(digest, refusal.to_string()).await;
        Err(refusal)
    }
}
