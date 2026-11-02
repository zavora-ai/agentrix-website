//! The migration door guards its inputs: 1 to 20 urls, no credentials taken.

use std::sync::Arc;

use agentrix_site_serve::signals::{Intake, IntakeState, MigrateState, intake_routes};
use agentrix_site_serve::{ProductStores, seed_demo, workflows};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

async fn app(dir: &tempfile::TempDir) -> Router {
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let stores = ProductStores::open(&data).await.unwrap();
    seed_demo(&stores, "demo", "business").await.unwrap();
    let kernel_stores = workflows::open_kernel_stores(&data).await.unwrap();
    let partition: adk_kernel::PartitionId = "demo.back-office".into();
    let sink = adk_kernel::charter::SignalSink::new(
        kernel_stores.signals.clone(),
        kernel_stores.agenda.clone(),
        partition,
    );
    let intake = Arc::new(Intake {
        site: "demo".into(),
        corpus: stores.corpus.clone(),
        sink,
    });
    let state = Arc::new(IntakeState {
        visitor: intake,
        migrate: Arc::new(MigrateState {
            site: "demo".into(),
            cards: stores.cards.clone(),
        }),
    });
    Router::new().merge(intake_routes(state))
}

async fn post(app: &mut Router, urls: Vec<String>) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/intake/migrate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({ "urls": urls }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn migrate_intake_guards_its_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(&dir).await;
    let (status, body) = post(&mut app, vec![]).await;
    assert_ne!(status, StatusCode::OK);
    assert!(body.contains("1 to 20"));
    let many: Vec<String> = (0..21)
        .map(|n| format!("https://example.com/{n}"))
        .collect();
    let (status, _) = post(&mut app, many).await;
    assert_ne!(status, StatusCode::OK);
}
