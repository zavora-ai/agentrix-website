//! The Security Guard and Facilities Manager targets: pure, model-free checks over
//! explicit inputs. The guard probe verifies the served release is digest-clean
//! and admitted, and throttles abusive IPs; the facilities watch turns
//! certificate, domain, backup and advisory inputs into proposals. Findings file
//! as incident and approval cards under the targets' role names.

use agentrix_site_edge::{Release, build_release};
use agentrix_site_inbox::{Card, CardKind, CardStore, NewCard, StoreError};
use agentrix_site_release::ReleaseStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What the probe can get wrong.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// The release store failed.
    #[error("probe releases: {0}")]
    Releases(String),
    /// The inbox failed.
    #[error("{0}")]
    Cards(#[from] StoreError),
}

/// Requests per minute that mark an IP abusive.
pub const ABUSE_THRESHOLD: u32 = 100;
/// Days of certificate life that raise an urgent renewal.
pub const CERT_URGENT_DAYS: u32 = 14;
/// Days of domain life that raise a renewal.
pub const DOMAIN_RENEW_DAYS: u32 = 30;
/// Days since a backup that raise a backup proposal.
pub const BACKUP_STALE_DAYS: u32 = 1;

/// Something the probe found wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    /// `drift` or `abuse`.
    pub kind: String,
    /// What is wrong.
    pub detail: String,
}

/// The probe's verdict: clean, or the incidents to card and roll back on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// Whether the served release is clean.
    pub clean: bool,
    /// The served digest checked.
    pub digest: String,
    /// What was wrong, empty when clean.
    pub incidents: Vec<Incident>,
}

/// One IP's request rate for the abuse check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbuseHit {
    /// The client IP.
    pub ip: String,
    /// Requests in the last minute.
    pub per_minute: u32,
}

/// Probes `served` against the admitted `store`: the digest must recompute from
/// the bundle, the digest must be admitted, and no IP may pass [`ABUSE_THRESHOLD`].
/// Model-free: hashing, one lookup, arithmetic.
pub async fn probe(
    store: &ReleaseStore,
    served: &Release,
    abuse: &[AbuseHit],
) -> Result<ProbeReport, ProbeError> {
    let mut incidents = Vec::new();
    let rebuilt = build_release(
        served.pages.clone(),
        &served.provenance.template,
        served.provenance.sources.clone(),
    );
    if rebuilt.digest != served.digest {
        incidents.push(Incident {
            kind: "drift".into(),
            detail: format!("served digest {} does not recompute", short(&served.digest)),
        });
    } else if store
        .get(&served.digest)
        .await
        .map_err(|e| ProbeError::Releases(e.to_string()))?
        .is_none()
    {
        incidents.push(Incident {
            kind: "drift".into(),
            detail: format!("served digest {} is not admitted", short(&served.digest)),
        });
    }
    for hit in abuse {
        if hit.per_minute > ABUSE_THRESHOLD {
            incidents.push(Incident {
                kind: "abuse".into(),
                detail: format!("{} at {}/min: throttle", hit.ip, hit.per_minute),
            });
        }
    }
    Ok(ProbeReport {
        clean: incidents.is_empty(),
        digest: served.digest.clone(),
        incidents,
    })
}

/// Files each incident as a Security Guard incident card, returning the cards.
pub async fn file_incidents(
    cards: &CardStore,
    site: &str,
    report: &ProbeReport,
) -> Result<Vec<Card>, StoreError> {
    let mut filed = Vec::new();
    for incident in &report.incidents {
        let card = cards
            .file(NewCard::new(
                site,
                CardKind::Incident,
                "Security Guard",
                format!("Security Guard: {} — {}", incident.kind, incident.detail),
                serde_json::to_value(incident).unwrap(),
            ))
            .await?;
        filed.push(card);
    }
    Ok(filed)
}

/// The facilities inputs: expiries, backup age, and open advisories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchInput {
    /// Days until the TLS certificate expires.
    pub cert_expiry_days: u32,
    /// Days until the domain renews or lapses.
    pub domain_expiry_days: u32,
    /// Days since the last verified backup.
    pub backup_age_days: u32,
    /// Open advisories against the edge lockfile.
    pub advisories: Vec<String>,
}

/// One facilities proposal: what to approve, and how urgently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// `cert-renewal`, `domain-renewal`, `backup`, or `patch`.
    pub kind: String,
    /// What to do.
    pub detail: String,
    /// `urgent` or `routine`.
    pub urgency: String,
}

/// Watches the facilities inputs and proposes: urgent cert renewal inside
/// [`CERT_URGENT_DAYS`], domain renewal inside [`DOMAIN_RENEW_DAYS`], a backup
/// past [`BACKUP_STALE_DAYS`], and a patch per advisory. Model-free thresholds.
pub fn watch(input: &WatchInput) -> Vec<Proposal> {
    let mut proposals = Vec::new();
    if input.cert_expiry_days <= CERT_URGENT_DAYS {
        proposals.push(Proposal {
            kind: "cert-renewal".into(),
            detail: format!(
                "certificate expires in {} day(s): renew",
                input.cert_expiry_days
            ),
            urgency: "urgent".into(),
        });
    }
    if input.domain_expiry_days <= DOMAIN_RENEW_DAYS {
        proposals.push(Proposal {
            kind: "domain-renewal".into(),
            detail: format!(
                "domain lapses in {} day(s): renew",
                input.domain_expiry_days
            ),
            urgency: "routine".into(),
        });
    }
    if input.backup_age_days > BACKUP_STALE_DAYS {
        proposals.push(Proposal {
            kind: "backup".into(),
            detail: format!("last backup {} day(s) ago: back up", input.backup_age_days),
            urgency: "routine".into(),
        });
    }
    for advisory in &input.advisories {
        proposals.push(Proposal {
            kind: "patch".into(),
            detail: format!("advisory {advisory}: patch"),
            urgency: "routine".into(),
        });
    }
    proposals
}

/// Files each proposal as a Facilities Manager approval card, returning the cards.
pub async fn file_proposals(
    cards: &CardStore,
    site: &str,
    proposals: &[Proposal],
) -> Result<Vec<Card>, StoreError> {
    let mut filed = Vec::new();
    for proposal in proposals {
        let card = cards
            .file(NewCard::new(
                site,
                CardKind::Approval,
                "Facilities Manager",
                format!(
                    "Facilities Manager: {} ({})",
                    proposal.detail, proposal.urgency
                ),
                serde_json::to_value(proposal).unwrap(),
            ))
            .await?;
        filed.push(card);
    }
    Ok(filed)
}

fn short(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}
