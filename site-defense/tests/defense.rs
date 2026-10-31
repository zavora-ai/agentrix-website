//! Phase 8 gate: the incident drill runs probe wake to rollback; freeze and quiet
//! hours gate every target; cross-site attempts are denied and audited; the probe
//! answers inside its budget.

use agentrix_site_defense::{
    AbuseHit, Denial, Freeze, Guard, GuardDecision, Isolation, QuietHours, WatchInput,
    file_incidents, file_proposals, probe, watch,
};
use agentrix_site_edge::{Current, build_release, business_site};
use agentrix_site_inbox::{Card, CardKind, CardStore, Decision, Execute, NewCard};
use agentrix_site_release::{LaunchGate, ReleaseStore};
use chrono::{TimeZone, Utc};

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

async fn setup(dir: &tempfile::TempDir) -> (ReleaseStore, CardStore, Freeze) {
    let releases = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("releases.db").display()
    );
    let cards = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    let defense = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("defense.db").display()
    );
    (
        ReleaseStore::open(&releases).await.unwrap(),
        CardStore::open(&cards).await.unwrap(),
        Freeze::open(&defense).await.unwrap(),
    )
}

async fn admit(cards: &CardStore, store: &ReleaseStore, site: &str, fact: &str) -> String {
    let mut pages = business_site();
    pages[0].facts[0].text = fact.to_string();
    let release = build_release(pages, "business", vec!["fixture".into()]);
    let card = cards
        .file(NewCard::new(
            site,
            CardKind::Approval,
            "Chief of Staff",
            "release approval",
            serde_json::json!({ "release": release.digest }),
        ))
        .await
        .unwrap();
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    LaunchGate::new(site)
        .admit(store, cards, &release, &card.id)
        .await
        .unwrap();
    release.digest
}

#[tokio::test]
async fn incident_drill_runs_probe_wake_to_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let (store, cards, freeze) = setup(&dir).await;
    let digest_n = admit(&cards, &store, "demo", "Founded in 2019.").await;
    let digest_n1 = admit(&cards, &store, "demo", "Founded in 2020.").await;
    let current = Current::new(store.current("demo").await.unwrap().unwrap());
    assert_eq!(current.get().digest, digest_n1);

    // The wake: a tampered serve. The probe finds drift, model-free, in budget.
    let mut tampered = (*current.get()).clone();
    tampered.pages[0].facts[0].text = "Founded in 1725.".into();
    let started = std::time::Instant::now();
    let report = probe(&store, &tampered, &[]).await.unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "probe within budget"
    );
    assert!(!report.clean);
    assert!(
        report
            .incidents
            .iter()
            .any(|incident| incident.kind == "drift")
    );

    // Card the incident, roll back to N, prove clean.
    let filed = file_incidents(&cards, "demo", &report).await.unwrap();
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0].role, "Security Guard");
    let rolled = store.set_current("demo", &digest_n).await.unwrap();
    current.set(rolled);
    let clean = probe(&store, &current.get(), &[]).await.unwrap();
    assert!(clean.clean, "{clean:?}");

    // Abuse throttles by arithmetic, not models.
    let abuse = probe(
        &store,
        &current.get(),
        &[AbuseHit {
            ip: "203.0.113.9".into(),
            per_minute: 400,
        }],
    )
    .await
    .unwrap();
    assert!(
        abuse
            .incidents
            .iter()
            .any(|incident| incident.kind == "abuse")
    );

    // The freeze audit remembers the drill's context lines only — no freeze here.
    let _ = freeze;
}

#[tokio::test]
async fn freeze_skips_and_quiet_hours_defer_with_audit() {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, freeze) = setup(&dir).await;
    let quiet = QuietHours {
        start_min: 120,
        end_min: 240,
    };
    let guard = Guard::new(&freeze).with_quiet(quiet);

    let day = Utc.with_ymd_and_hms(2026, 10, 31, 10, 0, 0).unwrap();
    assert_eq!(
        guard.check("demo", "site.probe", day).await.unwrap(),
        GuardDecision::Run
    );

    let night = Utc.with_ymd_and_hms(2026, 10, 31, 3, 0, 0).unwrap();
    let deferred = guard.check("demo", "site.probe", night).await.unwrap();
    match deferred {
        GuardDecision::Deferred(wake) => {
            assert_eq!(wake, Utc.with_ymd_and_hms(2026, 10, 31, 4, 0, 0).unwrap());
        }
        other => panic!("must defer, got {other:?}"),
    }

    freeze.freeze("demo", "incident drill").await.unwrap();
    let frozen = guard.check("demo", "site.probe", day).await.unwrap();
    assert!(matches!(frozen, GuardDecision::Frozen(_)));
    freeze.unfreeze("demo").await.unwrap();
    assert_eq!(
        guard.check("demo", "site.probe", day).await.unwrap(),
        GuardDecision::Run
    );

    let events = freeze.events("demo").await.unwrap();
    let kinds: Vec<&str> = events.iter().map(|(kind, _)| kind.as_str()).collect();
    assert!(kinds.contains(&"deferred"), "{kinds:?}");
    assert!(kinds.contains(&"frozen-skip"), "{kinds:?}");
    assert!(kinds.contains(&"frozen"), "{kinds:?}");
    assert!(kinds.contains(&"unfrozen"), "{kinds:?}");
}

#[tokio::test]
async fn watch_proposes_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let (_, cards, _) = setup(&dir).await;
    let proposals = watch(&WatchInput {
        cert_expiry_days: 9,
        domain_expiry_days: 400,
        backup_age_days: 3,
        advisories: vec!["CVE-2026-1".into()],
    });
    assert!(
        proposals
            .iter()
            .any(|proposal| proposal.kind == "cert-renewal")
    );
    assert!(proposals.iter().any(|proposal| proposal.kind == "backup"));
    assert!(proposals.iter().any(|proposal| proposal.kind == "patch"));
    assert!(
        !proposals
            .iter()
            .any(|proposal| proposal.kind == "domain-renewal")
    );
    let filed = file_proposals(&cards, "demo", &proposals).await.unwrap();
    assert_eq!(filed.len(), 3);
    assert!(filed.iter().all(|card| card.role == "Facilities Manager"));
    let quiet = watch(&WatchInput {
        cert_expiry_days: 90,
        domain_expiry_days: 400,
        backup_age_days: 0,
        advisories: Vec::new(),
    });
    assert!(quiet.is_empty());
}

#[tokio::test]
async fn cross_site_attempts_are_denied_and_audited() {
    let dir = tempfile::tempdir().unwrap();
    let (store, cards, freeze) = setup(&dir).await;
    let digest = admit(&cards, &store, "harbor", "Founded in 2019.").await;
    let card = cards
        .file(NewCard::new(
            "harbor",
            CardKind::Approval,
            "Chief of Staff",
            "harbor card",
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    let isolation = Isolation::new(&freeze, &cards, &store);

    // Own resources read and serve.
    assert!(isolation.read_card("harbor", &card.id).await.is_ok());
    assert!(isolation.serve_release("harbor", &digest).await.is_ok());

    // Foreign attempts deny with audit lines under the acting site.
    let denied = isolation.read_card("northwind", &card.id).await;
    assert!(
        matches!(denied, Err(Denial::CrossSite { .. })),
        "{denied:?}"
    );
    let denied = isolation.serve_release("northwind", &digest).await;
    assert!(
        matches!(denied, Err(Denial::CrossSite { .. })),
        "{denied:?}"
    );
    let events = freeze.events("northwind").await.unwrap();
    assert_eq!(
        events.iter().filter(|(kind, _)| kind == "denied").count(),
        2
    );

    // The store itself refuses a foreign serve pointer.
    assert!(store.set_current("northwind", &digest).await.is_err());
}
