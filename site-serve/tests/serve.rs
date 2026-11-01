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
    let runners =
        agents::build_runners(&config.app, &config.site, &stores, sessions, model).unwrap();
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
    let kernel = workflows::build_kernel(&config.app, &config.data, runners, workflows)
        .await
        .unwrap();
    drop(kernel);
}
