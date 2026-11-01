//! Composition gates, all offline: seed admits and serves, workflows execute and
//! guard, runners build, and the kernel builds with every target bound.

use std::collections::HashMap;
use std::sync::Arc;

use agentrix_site_edge::Current;
use agentrix_site_serve::workflows::SiteWorkflows;
use agentrix_site_serve::{ProductStores, ServeConfig, agents, seed_demo, workflows};
use tokio_util::sync::CancellationToken;

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

#[tokio::test]
async fn seed_admits_serves_and_stocks_the_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    let digest = seed_demo(&stores, &config.site).await.unwrap();
    let current = stores
        .releases
        .current(&config.site)
        .await
        .unwrap()
        .expect("a current release");
    assert_eq!(current.digest, digest);
    assert!(agentrix_site_edge::digest_match(&current).is_ok());
    let hits =
        agentrix_site_concierge::retrieve(&stores.corpus, &config.site, "bookkeeping charges", 3)
            .await
            .unwrap();
    assert!(!hits.is_empty(), "seeded facts retrieve");
    // Seeding twice keeps the first release: idempotent.
    let again = seed_demo(&stores, &config.site).await.unwrap();
    assert_eq!(again, digest);
}

#[tokio::test]
async fn workflows_execute_and_guard() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let current = Current::new(
        stores
            .releases
            .current(&config.site)
            .await
            .unwrap()
            .unwrap(),
    );
    let workflows = SiteWorkflows::new(
        config.site.clone(),
        current,
        stores.releases.clone(),
        stores.cards.clone(),
        stores.growth.clone(),
        stores.freeze.clone(),
    );
    assert!(adk_server::background::WorkflowExecutor::has_workflow(
        &workflows,
        "website.guardian.probe"
    ));
    assert!(!adk_server::background::WorkflowExecutor::has_workflow(
        &workflows, "nope"
    ));

    let token = CancellationToken::new();
    let probe = adk_server::background::WorkflowExecutor::execute(
        &workflows,
        "website.guardian.probe",
        HashMap::new(),
        token.clone(),
    )
    .await
    .unwrap();
    assert_eq!(probe["clean"], serde_json::json!(true));
    let watch = adk_server::background::WorkflowExecutor::execute(
        &workflows,
        "website.guardian.watch",
        HashMap::new(),
        token.clone(),
    )
    .await
    .unwrap();
    assert!(watch.get("proposals").is_some());
    let digest = adk_server::background::WorkflowExecutor::execute(
        &workflows,
        "website.steward.digest",
        HashMap::new(),
        token,
    )
    .await
    .unwrap();
    assert!(digest.get("card").is_some());

    stores
        .freeze
        .freeze(&config.site, "test freeze")
        .await
        .unwrap();
    let skipped = adk_server::background::WorkflowExecutor::execute(
        &workflows,
        "website.guardian.probe",
        HashMap::new(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(skipped["skipped"], serde_json::json!("frozen"));
}

#[tokio::test]
async fn runners_and_kernel_build_with_every_target() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let sessions: Arc<dyn adk_session::SessionService> =
        Arc::new(adk_session::InMemorySessionService::new());
    let model = agents::build_model(&config).unwrap();
    let kernel_stores = workflows::open_kernel_stores(&config.data).await.unwrap();
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        &stores,
        sessions,
        model,
        kernel_stores.signals.clone(),
        None,
    )
    .unwrap();
    let targets: Vec<&str> = runners.iter().map(|(target, _)| target.as_str()).collect();
    assert_eq!(targets, vec!["growth", "revenue", "steward"]);

    let current = Current::new(
        stores
            .releases
            .current(&config.site)
            .await
            .unwrap()
            .unwrap(),
    );
    let workflows = Arc::new(SiteWorkflows::new(
        config.site.clone(),
        current,
        stores.releases.clone(),
        stores.cards.clone(),
        stores.growth.clone(),
        stores.freeze.clone(),
    ));
    let kernel = workflows::build_kernel(
        &config.app,
        &config.site,
        &config.data,
        &kernel_stores,
        stores.growth.clone(),
        runners,
        workflows,
    )
    .await
    .unwrap();
    drop(kernel);
}

/// Live proof: the Growth runner closes a gap end to end through Baseten — the
/// agent calls the schemad tool and a held Draft card exists afterwards.
#[tokio::test]
#[ignore = "needs BASETEN_API_KEY and spends model calls"]
async fn growth_runner_files_a_draft_live() {
    use adk_core::{Content, SessionId, UserId};
    use adk_session::CreateRequest;
    use futures::StreamExt as _;

    let dir = tempfile::tempdir().unwrap();
    let mut config = config(&dir);
    config.dry_run = false;
    config.baseten_key = Some(std::env::var("BASETEN_API_KEY").expect("BASETEN_API_KEY"));
    config.baseten_model = std::env::var("BASETEN_MODEL")
        .unwrap_or_else(|_| "meta-models/muse-spark-open-1026".into());
    config.baseten_base_url = std::env::var("BASETEN_BASE_URL")
        .unwrap_or_else(|_| "https://inference.baseten.co/v1".into());
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let sessions: Arc<dyn adk_session::SessionService> =
        Arc::new(adk_session::InMemorySessionService::new());
    let session = sessions
        .create(CreateRequest {
            app_name: config.app.clone(),
            user_id: "owner".to_string(),
            session_id: None,
            state: Default::default(),
        })
        .await
        .unwrap();
    let model = agents::build_model(&config).unwrap();
    let kernel_stores = workflows::open_kernel_stores(&config.data).await.unwrap();
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        &stores,
        sessions,
        model,
        kernel_stores.signals.clone(),
        None,
    )
    .unwrap();
    let growth = runners
        .iter()
        .find(|(target, _)| target == "growth")
        .expect("growth runner")
        .1
        .clone();

    let partition: adk_kernel::PartitionId = format!("{}.back-office", config.app).into();
    let sink = adk_kernel::charter::SignalSink::new(
        kernel_stores.signals.clone(),
        kernel_stores.agenda.clone(),
        partition,
    );
    let gap_signal = awp_types::Signal {
        id: uuid::Uuid::new_v4(),
        kind: awp_types::SignalKind::ContentGap {
            topic: awp_types::Bounded::truncated("Weekend opening hours", 120),
            question_count: 1,
            samples: vec![awp_types::Bounded::truncated("Are you open Saturday?", 200)],
        },
        emitted_by: format!("urn:adk:agent:{}/concierge", config.site),
        emitted_at: chrono::Utc::now(),
    };
    assert!(matches!(
        sink.emit(gap_signal).await.unwrap(),
        adk_kernel::charter::SignalOutcome::Filed { .. }
    ));
    let gap = serde_json::json!({
        "job": "freshness",
        "playbook": "freshness",
        "page_slug": "contact",
    });
    let mut events = growth
        .run(
            UserId::new("owner").unwrap(),
            SessionId::new(session.id().to_string()).unwrap(),
            Content::new("user").with_text(gap.to_string()),
        )
        .await
        .unwrap();
    let mut calls = 0;
    while let Some(event) = events.next().await {
        let event = event.unwrap();
        if let Some(content) = &event.llm_response.content {
            for part in &content.parts {
                if matches!(part, adk_core::Part::FunctionCall { .. }) {
                    calls += 1;
                }
            }
        }
    }
    assert!(calls >= 1, "the agent must call file_content_draft");
    let open = stores.cards.list_open(&config.site).await.unwrap();
    assert!(
        open.iter()
            .any(|card| card.role == "Content Editor" && card.title.contains("contact")),
        "a held contact draft exists"
    );
}

/// The served kernel registers the host's single table, and the weekly digest
/// fires a missed tick once instead of skipping it.
#[tokio::test]
async fn standing_triggers_share_the_host_table_and_fire_the_digest_once() {
    let partition: adk_kernel::PartitionId = "demo.back-office".into();
    let standing = workflows::standing_triggers("demo", workflows::site_id(), &partition);
    assert_eq!(standing.len(), 7);
    let mut check = agentrix_site_host::SelfCheck::new();
    check.processes(&standing).expect("all seven kinds");
    let digest = standing
        .iter()
        .find(|process| process.job == "weekly-review")
        .expect("the digest");
    match &digest.trigger.wake {
        adk_kernel::Wake::Cron { missed, .. } => assert_eq!(
            missed,
            &adk_core::schedule::MissedFirePolicy::FireOnce,
            "a down Monday still reports"
        ),
        other => panic!("the digest must stay cron, got {other:?}"),
    }
    let probe = standing
        .iter()
        .find(|process| process.job == "protect")
        .expect("the probe");
    match &probe.trigger.wake {
        adk_kernel::Wake::Cron { missed, .. } => assert_eq!(
            missed,
            &adk_core::schedule::MissedFirePolicy::Skip,
            "the five-minute probe skips catch-up"
        ),
        other => panic!("the probe must stay cron, got {other:?}"),
    }
}

/// The kernel build registers the back-office partition with its quotas.
#[tokio::test]
async fn back_office_partition_carries_quotas() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let sessions: Arc<dyn adk_session::SessionService> =
        Arc::new(adk_session::InMemorySessionService::new());
    let model = agents::build_model(&config).unwrap();
    let kernel_stores = workflows::open_kernel_stores(&config.data).await.unwrap();
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        &stores,
        sessions,
        model,
        kernel_stores.signals.clone(),
        None,
    )
    .unwrap();
    let current = Current::new(
        stores
            .releases
            .current(&config.site)
            .await
            .unwrap()
            .unwrap(),
    );
    let workflow_runs = Arc::new(SiteWorkflows::new(
        config.site.clone(),
        current,
        stores.releases.clone(),
        stores.cards.clone(),
        stores.growth.clone(),
        stores.freeze.clone(),
    ));
    let kernel = workflows::build_kernel(
        &config.app,
        &config.site,
        &config.data,
        &kernel_stores,
        stores.growth.clone(),
        runners,
        workflow_runs,
    )
    .await
    .unwrap();
    let partitions = adk_kernel::Partitions::new(kernel_stores.partitions.clone());
    let partition: adk_kernel::PartitionId = "demo.back-office".into();
    let registered = partitions
        .get(&partition)
        .await
        .unwrap()
        .expect("the back-office partition");
    assert_eq!(registered.quotas.max_running, Some(4));
    assert_eq!(registered.quotas.max_queued, Some(64));
    drop(kernel);
}

/// The served binary self-checks green before serving: spawn dry-run, await
/// the report line on a reader thread with a deadline, then stop it.
#[test]
fn serve_binary_self_checks_then_serves() {
    use std::io::BufRead as _;
    use std::time::{Duration, Instant};

    let exe = std::env::var("CARGO_BIN_EXE_agentrix-site-serve").expect("the binary under test");
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(exe)
        .arg("--data")
        .arg(dir.path().join("data"))
        .arg("--addr")
        .arg("127.0.0.1:0")
        .arg("--dry-run")
        .env("RUST_LOG", "info")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("the server spawns");
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            if tx.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut green = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) if line.contains("self-check green") => {
                green = true;
                break;
            }
            Ok(_) => {}
            Err(_) => {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
            }
        }
    }
    child.kill().ok();
    child.wait().ok();
    assert!(green, "the served path must self-check green");
}

/// Failed runs file Security Guard incident cards; completed runs file
/// nothing.
#[tokio::test]
async fn failed_runs_file_incident_cards() {
    use std::time::{Duration, Instant};

    use agentrix_site_inbox::CardKind;
    use agentrix_site_serve::incident_watch;

    let dir = tempfile::tempdir().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    let bus = adk_kernel::EventBus::new(16);
    let watcher = incident_watch(bus.subscribe(), stores.cards.clone(), config.site.clone());
    bus.publish(adk_kernel::KernelEvent::Process {
        pid: "run-1".to_string(),
        from: adk_core::RunState::Running,
        to: adk_core::RunState::Failed,
        reason: Some("tool blew up".to_string()),
    });
    let since = Instant::now();
    loop {
        let open = stores.cards.list_open(&config.site).await.unwrap();
        if let Some(card) = open
            .iter()
            .find(|card| card.role == "Security Guard" && card.kind == CardKind::Incident)
        {
            assert!(card.title.contains("run-1"), "{}", card.title);
            break;
        }
        assert!(
            since.elapsed() < Duration::from_secs(10),
            "the failure must card promptly"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bus.publish(adk_kernel::KernelEvent::Process {
        pid: "run-2".to_string(),
        from: adk_core::RunState::Running,
        to: adk_core::RunState::Completed,
        reason: None,
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let open = stores.cards.list_open(&config.site).await.unwrap();
    assert_eq!(
        open.iter()
            .filter(|card| card.kind == CardKind::Incident)
            .count(),
        1,
        "completed runs file nothing"
    );
    watcher.abort();
}
