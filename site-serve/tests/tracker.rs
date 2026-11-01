//! The nightly KPI loop, all offline: the charter seeds once, the tick samples
//! the growth gauges, breached guardrails pause their jobs, and pauses restore
//! after a restart. Resume takes a person's decision, never an agent's.

use adk_kernel::charter::{CharterStore, JobControl, KpiStore, OwnerDecision};
use agentrix_site_serve::{ProductStores, ServeConfig, seed_demo, tracker, workflows};
use uuid::Uuid;

fn config(dir: &tempfile::TempDir) -> ServeConfig {
    ServeConfig {
        data: dir.path().join("data"),
        app: "demo".into(),
        site: "demo".into(),
        addr: "127.0.0.1:0".into(),
        dry_run: true,
        baseten_key: None,
        baseten_model: "scripted".into(),
        baseten_base_url: "http://localhost/".into(),
        max_tokens: 16_384,
    }
}

async fn setup(dir: &tempfile::TempDir) -> (ServeConfig, ProductStores, workflows::KernelStores) {
    let config = config(dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let kernel_stores = workflows::open_kernel_stores(&config.data).await.unwrap();
    (config, stores, kernel_stores)
}

fn site_tracker(
    config: &ServeConfig,
    stores: &ProductStores,
    kernel_stores: &workflows::KernelStores,
) -> adk_kernel::charter::KpiTracker {
    tracker::site_tracker(
        Uuid::nil(),
        &config.site,
        stores.growth.clone(),
        kernel_stores.charters.clone(),
        kernel_stores.kpis.clone(),
    )
}

#[tokio::test]
async fn charter_seeds_once_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, kernel_stores) = setup(&dir).await;
    assert!(
        tracker::seed_charter(kernel_stores.charters.as_ref(), Uuid::nil())
            .await
            .unwrap(),
        "first seed commits"
    );
    assert!(
        !tracker::seed_charter(kernel_stores.charters.as_ref(), Uuid::nil())
            .await
            .unwrap(),
        "second seed commits nothing"
    );
    let current = kernel_stores
        .charters
        .current(Uuid::nil())
        .await
        .unwrap()
        .expect("a charter in force");
    assert_eq!(current.version(), 1);
    assert_eq!(current.charter.kpis.len(), 2);
    assert_eq!(current.charter.guardrails.len(), 2);
}

#[tokio::test]
async fn spend_breach_pauses_the_spending_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    tracker::seed_charter(kernel_stores.charters.as_ref(), Uuid::nil())
        .await
        .unwrap();
    stores.growth.set_cap("demo", 100).await.unwrap();
    let reservation = stores.growth.reserve("demo", 95, "ads").await.unwrap();
    stores.growth.commit(&reservation.id).await.unwrap();

    let report = site_tracker(&config, &stores, &kernel_stores)
        .tick()
        .await
        .unwrap();
    assert_eq!(report.charter_version, Some(1));
    assert_eq!(report.breached, vec!["spend_cap".to_string()]);
    assert!(report.paused.contains(&"nurture".to_string()));
    assert!(report.paused.contains(&"social".to_string()));
    assert!(report.unmeasured.is_empty(), "{:?}", report.unmeasured);
    assert!(
        stores
            .growth
            .paused_reason("demo", "nurture")
            .await
            .unwrap()
            .is_some_and(|reason| reason.contains("spend_cap")),
        "the pause names its guardrail"
    );
    assert!(
        stores
            .growth
            .paused_reason("demo", "owner-questions")
            .await
            .unwrap()
            .is_none(),
        "answering keeps running under breach"
    );

    // The tick recorded the sample it judged.
    let now = chrono::Utc::now();
    let samples = kernel_stores
        .kpis
        .samples(
            Uuid::nil(),
            "spend_ratio",
            now - chrono::Duration::hours(1),
            now + chrono::Duration::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert!((samples[0].value - 0.95).abs() < f64::EPSILON);

    // A fresh tracker on the same stores restores the pauses, as after a restart.
    let restored = site_tracker(&config, &stores, &kernel_stores)
        .restore_pauses()
        .await
        .unwrap();
    assert!(restored.contains(&"nurture".to_string()));
    assert!(restored.contains(&"social".to_string()));
}

#[tokio::test]
async fn open_breaches_pause_outward_acts_but_not_reports() {
    use agentrix_site_growth::Direction;

    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    tracker::seed_charter(kernel_stores.charters.as_ref(), Uuid::nil())
        .await
        .unwrap();
    stores
        .growth
        .set_target("demo", "qualified_leads", 100.0, Direction::Up, "revenue")
        .await
        .unwrap();
    stores
        .growth
        .record("demo", "qualified_leads", 40.0)
        .await
        .unwrap();

    let report = site_tracker(&config, &stores, &kernel_stores)
        .tick()
        .await
        .unwrap();
    assert_eq!(report.breached, vec!["breaches_open".to_string()]);
    for job in ["freshness", "social", "nurture"] {
        assert!(
            stores
                .growth
                .paused_reason("demo", job)
                .await
                .unwrap()
                .is_some(),
            "{job} pauses"
        );
    }
    for job in ["owner-questions", "weekly-review"] {
        assert!(
            stores
                .growth
                .paused_reason("demo", job)
                .await
                .unwrap()
                .is_none(),
            "{job} keeps running"
        );
    }
}

#[tokio::test]
async fn resume_takes_a_persons_decision() {
    use adk_core::principal::{SubjectKind, SubjectRef};

    let dir = tempfile::tempdir().unwrap();
    let (config, stores, _) = setup(&dir).await;
    let control =
        tracker::GrowthControl::new(Uuid::nil(), config.site.clone(), stores.growth.clone());
    control
        .pause(Uuid::nil(), "nurture", "test pause")
        .await
        .unwrap();
    assert_eq!(
        control
            .paused(Uuid::nil(), "nurture")
            .await
            .unwrap()
            .as_deref(),
        Some("test pause")
    );

    let agent = OwnerDecision::new(
        "agent-1",
        SubjectRef::new(SubjectKind::Agent, "site/revenue"),
        chrono::Utc::now(),
    );
    assert!(
        control
            .resume(Uuid::nil(), "nurture", &agent)
            .await
            .is_err()
    );
    assert!(
        control
            .paused(Uuid::nil(), "nurture")
            .await
            .unwrap()
            .is_some(),
        "an agent's decision resumes nothing"
    );

    let owner = OwnerDecision::new("owner-1", SubjectRef::human("owner"), chrono::Utc::now());
    assert!(
        control
            .resume(Uuid::nil(), "nurture", &owner)
            .await
            .unwrap()
    );
    assert!(
        !control
            .resume(Uuid::nil(), "nurture", &owner)
            .await
            .unwrap()
    );
    assert!(
        control
            .paused(Uuid::nil(), "nurture")
            .await
            .unwrap()
            .is_none()
    );

    assert!(
        control
            .pause(Uuid::new_v4(), "nurture", "foreign site")
            .await
            .is_err(),
        "one control serves one site"
    );
}
