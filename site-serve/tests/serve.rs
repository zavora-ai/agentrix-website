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
        &config.data,
        &kernel_stores,
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
