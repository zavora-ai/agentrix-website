//! Agentrix Website Phase 0: the host scaffold as a library.
//!
//! [`processes`] is the single standing-trigger table: the `site-host` binary
//! checks it and the served `site-serve` registers it, so the seven jobs
//! cannot drift between the checked scaffold and the running site.

pub mod selfcheck;

pub use selfcheck::SelfCheck;

use adk_core::NewRun;
use adk_kernel::PartitionId;
use adk_kernel::charter::SiteProcessKind;
use serde_json::json;
use uuid::Uuid;

/// A site process: its kind, its job, the agent or workflow that runs it, and
/// its standing trigger. Mirrors `adk_website::jobs::processes`.
pub struct Process {
    /// The process kind.
    pub kind: SiteProcessKind,
    /// The charter job.
    pub job: &'static str,
    /// The agent or workflow target.
    pub target: &'static str,
    /// The standing trigger.
    pub trigger: adk_kernel::Trigger,
}

/// The seven processes a business site wakes for `app`: the probe, the watch
/// and the digest run as workflows, the others run their job's agent with the
/// playbook.
pub fn processes(app: &str, site: Uuid, partition: &PartitionId) -> Vec<Process> {
    const TABLE: [(SiteProcessKind, &str, &str, &str); 7] = [
        (
            SiteProcessKind::Probe,
            "protect",
            "protect",
            "website.guardian.probe",
        ),
        (
            SiteProcessKind::Watch,
            "hosting",
            "hosting",
            "website.guardian.watch",
        ),
        (SiteProcessKind::Content, "freshness", "freshness", "growth"),
        (
            SiteProcessKind::Reviews,
            "social",
            "review-replies",
            "growth",
        ),
        (SiteProcessKind::Outreach, "nurture", "nurture", "revenue"),
        (
            SiteProcessKind::Support,
            "owner-questions",
            "owner-questions",
            "steward",
        ),
        (
            SiteProcessKind::Digest,
            "weekly-review",
            "weekly-review",
            "website.steward.digest",
        ),
    ];
    TABLE
        .into_iter()
        .map(|(kind, job, playbook, target)| {
            let run = NewRun::new(
                kind.queue().as_str(),
                app,
                format!("site:{app}"),
                target,
                json!({
                    "site": site,
                    "job": job,
                    "playbook": playbook,
                    "text": format!("Run the {playbook} playbook for job {job}."),
                }),
            );
            let trigger = kind.trigger(kind.default_wake(partition), run, partition.clone());
            Process {
                kind,
                job,
                target,
                trigger,
            }
        })
        .collect()
}
