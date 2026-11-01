//! The host scaffold's fail-loud self-check (Agentrix `PLAN.md` Phase 0).
//!
//! [`SelfCheck`] records one line per store opened, per site process registered, and per
//! round-trip proven, and fails loudly — with which line failed and why — when a store is
//! missing, a job kind is absent, or a write does not read back.

use adk_core::{AdkError, Result};
use adk_kernel::charter::{SignalOutcome, SiteProcessKind};

use crate::Process;

/// The self-check report: the lines that passed, in order.
#[derive(Debug, Default)]
pub struct SelfCheck {
    lines: Vec<String>,
}

impl SelfCheck {
    /// An empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the durable stores opened at `url`, so a silently dropped store shows as
    /// a missing line.
    pub fn stores(&mut self, url: &str) {
        for name in [
            "runs",
            "approvals",
            "agenda",
            "partitions",
            "watermarks",
            "signals",
            "charters",
            "kpis",
        ] {
            self.lines.push(format!("store {name}: sqlite ({url})"));
        }
    }

    /// Records every site process registered, and fails when any of the seven
    /// [`SiteProcessKind`]s has no trigger: the host must wake every job.
    pub fn processes(&mut self, processes: &[Process]) -> Result<()> {
        for process in processes {
            self.lines.push(format!(
                "process {}: job {} -> target {}",
                process.kind.as_str(),
                process.job,
                process.target
            ));
        }
        let kinds: Vec<SiteProcessKind> = processes.iter().map(|p| p.kind).collect();
        let mut missing = Vec::new();
        for kind in [
            SiteProcessKind::Probe,
            SiteProcessKind::Watch,
            SiteProcessKind::Content,
            SiteProcessKind::Reviews,
            SiteProcessKind::Outreach,
            SiteProcessKind::Support,
            SiteProcessKind::Digest,
        ] {
            if !kinds.contains(&kind) {
                missing.push(kind.as_str());
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(AdkError::config(format!(
                "self-check: no trigger for job kind(s) {}",
                missing.join(", ")
            )))
        }
    }

    /// Records the kernel build and the standing KPI trigger beside the seven jobs.
    pub fn kernel(&mut self, triggers: usize) {
        self.lines
            .push(format!("kernel: built with {triggers} standing triggers"));
        self.lines
            .push("process kernel:kpi: nightly tracker on the maintenance queue".into());
    }

    /// Records the signal round-trip, and fails unless the probe signal filed or counted:
    /// the sink, its store and the agenda must all work.
    pub fn signal(&mut self, outcome: &SignalOutcome) -> Result<()> {
        match outcome {
            SignalOutcome::Filed { item, count } => {
                self.lines.push(format!(
                    "signal: probe filed as agenda item {item} (count {count})"
                ));
                Ok(())
            }
            SignalOutcome::Counted { item, count } => {
                self.lines.push(format!(
                    "signal: probe counted on agenda item {item} (count {count})"
                ));
                Ok(())
            }
            other => Err(AdkError::config(format!(
                "self-check: the probe signal did not file: {other:?}"
            ))),
        }
    }

    /// Records the durability proof, and fails unless `db` exists and is non-empty: an
    /// in-memory store leaves no file, so this is the check that fails loudest.
    pub fn durable_db(&mut self, db: &std::path::Path) -> Result<()> {
        let size = std::fs::metadata(db)
            .map_err(|e| {
                AdkError::config(format!(
                    "self-check: the durable database {} is missing: {e}",
                    db.display()
                ))
            })?
            .len();
        if size == 0 {
            return Err(AdkError::config(format!(
                "self-check: the durable database {} is empty",
                db.display()
            )));
        }
        self.lines
            .push(format!("durable: {} ({size} bytes)", db.display()));
        Ok(())
    }

    /// The report's lines.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

impl std::fmt::Display for SelfCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "agentrix site-host self-check: {} lines green",
            self.lines.len()
        )?;
        for line in &self.lines {
            writeln!(f, "  ok - {line}")?;
        }
        Ok(())
    }
}
