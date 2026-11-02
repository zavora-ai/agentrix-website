//! Candidates, parity scoring, and the go-live card.
//!
//! [`migrate`] turns [`Evidence`] into a [`Candidate`]: a release built only from
//! the evidence (its sources are the evidence URLs), plus the carried links,
//! products and metadata the release model does not hold. [`ParityReport`] scores
//! the candidate against the legacy site on four axes; [`PARITY_THRESHOLD`]
//! gates go-live. [`MigrationParity`] carries a scored report into the LaunchGate,
//! and [`file_go_live`] files the owner's card with the report attached.

use std::collections::HashMap;

use agentrix_site_edge::{Page, Release, build_release};
use agentrix_site_inbox::{Card, CardKind, CardStore, NewCard, StoreError};
use agentrix_site_release::ParityCheck;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crawl::{Evidence, Product};

/// The overall parity a candidate needs for go-live.
pub const PARITY_THRESHOLD: f64 = 0.95;

/// What migration can get wrong.
#[derive(Debug, Error)]
pub enum MigrateError {
    /// The inbox failed under the go-live card.
    #[error("go-live card: {0}")]
    Cards(#[from] StoreError),
    /// The card payload did not serialize.
    #[error("go-live payload: {0}")]
    Json(#[from] serde_json::Error),
}

/// A migration candidate: the release plus what the release model does not hold.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The release built from the evidence.
    pub release: Release,
    /// Carried links, per page slug.
    pub links: HashMap<String, Vec<String>>,
    /// Carried products, per page slug.
    pub products: HashMap<String, Vec<Product>>,
    /// Carried metadata, per page slug.
    pub metadata: HashMap<String, HashMap<String, String>>,
}

/// Parity per axis, 0.0 to 1.0, with the gaps that cost points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParityReport {
    /// Share of legacy content tokens the candidate covers, averaged per page.
    pub content: f64,
    /// Share of legacy links carried.
    pub links: f64,
    /// Share of legacy products carried with their prices.
    pub products: f64,
    /// Share of legacy metadata pairs carried.
    pub metadata: f64,
    /// Mean of the four axes.
    pub overall: f64,
    /// What is missing, most costly first, capped at ten.
    pub gaps: Vec<String>,
}

impl ParityReport {
    /// Whether the candidate may go live.
    pub fn passes(&self) -> bool {
        self.overall >= PARITY_THRESHOLD
    }
}

/// Builds the candidate from `evidence` and scores it: pages become release pages
/// with one fact per sentence, links, products and metadata carry verbatim, and
/// the release sources are the evidence URLs.
pub fn migrate(evidence: &Evidence) -> (Candidate, ParityReport) {
    let mut pages = Vec::new();
    let mut links = HashMap::new();
    let mut products = HashMap::new();
    let mut metadata = HashMap::new();
    let mut sources = Vec::new();
    for page in &evidence.pages {
        sources.push(page.url.clone());
        let facts = page
            .text
            .split(". ")
            .map(str::trim)
            .filter(|sentence| !sentence.is_empty())
            .enumerate()
            .map(|(n, sentence)| agentrix_site_edge::Fact {
                id: format!("fact-{n}"),
                text: sentence.trim_end_matches('.').to_string() + ".",
            })
            .collect();
        let kind = if !page.products.is_empty() {
            awp_types::ContentKind::Product
        } else if page.url.contains("/blog/") || page.url.contains("/post/") {
            awp_types::ContentKind::Post
        } else if page.url.contains("/news/") || page.url.contains("/article/") {
            awp_types::ContentKind::Article
        } else {
            awp_types::ContentKind::Page
        };
        pages.push(Page {
            kind,
            slug: page.slug.clone(),
            title: page.title.clone(),
            intro: format!("Migrated from {}.", page.url),
            facts,
        });
        links.insert(page.slug.clone(), page.links.clone());
        products.insert(page.slug.clone(), page.products.clone());
        metadata.insert(page.slug.clone(), page.metadata.clone());
    }
    let release = build_release(pages, "migrated", sources);
    let candidate = Candidate {
        release,
        links,
        products,
        metadata,
    };
    let report = score(evidence, &candidate);
    (candidate, report)
}

/// Scores `candidate` against the legacy `evidence` per axis. Missing pages,
/// links, products and pairs each cost their axis; every cost is named in `gaps`.
/// Silence never scores: an empty legacy axis scores 1.0 only when the candidate
/// is empty on it too, and any evidence gap is listed.
pub fn score(evidence: &Evidence, candidate: &Candidate) -> ParityReport {
    let mut content_sum = 0.0;
    let mut content_pages = 0_usize;
    let mut gaps = Vec::new();
    for legacy in &evidence.pages {
        let Some(page) = candidate
            .release
            .pages
            .iter()
            .find(|p| p.slug == legacy.slug)
        else {
            gaps.push(format!("page {} is missing", legacy.slug));
            content_pages += 1;
            continue;
        };
        let want = tokens(&legacy.text);
        let have = tokens(
            &page
                .facts
                .iter()
                .map(|f| f.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let covered = want.iter().filter(|tok| have.contains(tok)).count();
        let axis = if want.is_empty() {
            1.0
        } else {
            covered as f64 / want.len() as f64
        };
        if axis < 1.0 {
            gaps.push(format!(
                "page {} covers {axis:.2} of its content",
                legacy.slug
            ));
        }
        content_sum += axis;
        content_pages += 1;
    }
    let content = if content_pages == 0 {
        1.0
    } else {
        content_sum / content_pages as f64
    };

    let (mut link_have, mut link_want) = (0_usize, 0_usize);
    let (mut prod_have, mut prod_want) = (0_usize, 0_usize);
    let (mut meta_have, mut meta_want) = (0_usize, 0_usize);
    for legacy in &evidence.pages {
        let carried_links = candidate.links.get(&legacy.slug);
        link_want += legacy.links.len();
        if let Some(carried) = carried_links {
            link_have += legacy
                .links
                .iter()
                .filter(|link| carried.contains(link))
                .count();
            for link in &legacy.links {
                if !carried.contains(link) {
                    gaps.push(format!("page {} lost link {link}", legacy.slug));
                }
            }
        } else {
            gaps.push(format!(
                "page {} lost all {} link(s)",
                legacy.slug,
                legacy.links.len()
            ));
        }
        let carried_products = candidate.products.get(&legacy.slug);
        prod_want += legacy.products.len();
        if let Some(carried) = carried_products {
            prod_have += legacy
                .products
                .iter()
                .filter(|prod| carried.contains(prod))
                .count();
            for prod in &legacy.products {
                if !carried.contains(prod) {
                    gaps.push(format!("page {} lost product {}", legacy.slug, prod.name));
                }
            }
        } else if !legacy.products.is_empty() {
            gaps.push(format!("page {} lost all products", legacy.slug));
        }
        let carried_meta = candidate.metadata.get(&legacy.slug);
        meta_want += legacy.metadata.len();
        if let Some(carried) = carried_meta {
            for (key, value) in &legacy.metadata {
                if carried.get(key) == Some(value) {
                    meta_have += 1;
                } else {
                    gaps.push(format!("page {} lost metadata {key}", legacy.slug));
                }
            }
        } else if !legacy.metadata.is_empty() {
            gaps.push(format!("page {} lost all metadata", legacy.slug));
        }
    }
    let ratio = |have: usize, want: usize| {
        if want == 0 {
            1.0
        } else {
            have as f64 / want as f64
        }
    };
    let links = ratio(link_have, link_want);
    let products = ratio(prod_have, prod_want);
    let metadata = ratio(meta_have, meta_want);
    let overall = (content + links + products + metadata) / 4.0;
    gaps.truncate(10);
    ParityReport {
        content,
        links,
        products,
        metadata,
        overall,
        gaps,
    }
}

/// A scored report as a LaunchGate parity check: passes at or above
/// [`PARITY_THRESHOLD`], refuses with the gaps below it.
pub struct MigrationParity {
    report: ParityReport,
}

impl MigrationParity {
    /// The check over a scored `report`.
    pub fn over(report: ParityReport) -> Self {
        Self { report }
    }
}

impl ParityCheck for MigrationParity {
    fn name(&self) -> &'static str {
        "migration-parity"
    }

    fn check(&self, _candidate: &Release) -> Result<(), String> {
        if self.report.passes() {
            Ok(())
        } else {
            Err(format!(
                "overall {:.2} below {:.2}: {}",
                self.report.overall,
                PARITY_THRESHOLD,
                self.report.gaps.join("; ")
            ))
        }
    }
}

/// Files the go-live card for `candidate` with the parity `report` attached: the
/// payload carries the release digest and the full report, so the owner approves
/// evidence, not a promise. Filed by the Chief of Staff role.
pub async fn file_go_live(
    cards: &CardStore,
    site: &str,
    candidate: &Candidate,
    report: &ParityReport,
) -> Result<Card, MigrateError> {
    let title = format!(
        "Go-live: migrated release {} at parity {:.2}",
        &candidate.release.digest[..12.min(candidate.release.digest.len())],
        report.overall
    );
    let payload = serde_json::json!({
        "release": candidate.release.digest,
        "report": report,
    });
    let card = cards
        .file(NewCard::new(
            site,
            CardKind::Approval,
            "Chief of Staff",
            title,
            payload,
        ))
        .await?;
    Ok(card)
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}
