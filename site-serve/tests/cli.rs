//! The agent CLI's two tiers: anonymous callers read, verified callers hold
//! orders, open support threads and capture consented leads.

use std::sync::Arc;

use adk_kernel::charter::SignalStore;
use agentrix_site_edge::Current;
use agentrix_site_serve::cli::{CliState, TokenStore, cli_routes};
use agentrix_site_serve::{ProductStores, seed_demo, workflows};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

async fn setup(dir: &tempfile::TempDir) -> (Arc<CliState>, ProductStores, workflows::KernelStores) {
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let stores = ProductStores::open(&data).await.unwrap();
    seed_demo(&stores, "demo", "shop").await.unwrap();
    let kernel_stores = workflows::open_kernel_stores(&data).await.unwrap();
    let partition: adk_kernel::PartitionId = "demo.back-office".into();
    let sink = adk_kernel::charter::SignalSink::new(
        kernel_stores.signals.clone(),
        kernel_stores.agenda.clone(),
        partition,
    );
    let tokens = Arc::new(
        TokenStore::open(&format!(
            "sqlite://{}?mode=rwc",
            data.join("tokens.db").display()
        ))
        .await
        .unwrap(),
    );
    let release = stores.releases.current("demo").await.unwrap().unwrap();
    let state = Arc::new(CliState {
        site: "demo".into(),
        current: Current::new(release),
        tokens,
        commerce: stores.commerce.clone(),
        cards: stores.cards.clone(),
        corpus: stores.corpus.clone(),
        sink,
    });
    (state, stores, kernel_stores)
}

async fn call(
    state: &Arc<CliState>,
    bearer: Option<&str>,
    command: &str,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::post("/cli").header("content-type", "application/json");
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    let response = cli_routes(state.clone())
        .oneshot(
            request
                .body(Body::from(
                    serde_json::json!({ "command": command }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn anonymous_callers_read_but_cannot_act() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _, _) = setup(&dir).await;
    let (status, body) = call(&state, None, "list").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("sukuma-bunch"));
    let (status, _) = call(&state, None, "stock arabica-250g").await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(&state, None, "ask arabica-250g roast").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("Medium roast"));
    for command in [
        "order arabica-250g 1",
        "support hello?",
        "lead email c1 consent",
    ] {
        let (status, body) = call(&state, None, command).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{command}");
        assert!(
            body["text"].as_str().unwrap().contains("verified"),
            "{command}"
        );
    }
}

#[tokio::test]
async fn verified_agents_hold_orders_and_capture_consented_leads() {
    let dir = tempfile::tempdir().unwrap();
    let (state, stores, kernel_stores) = setup(&dir).await;
    let token = state.tokens.issue("demo", "stall-7").await.unwrap();
    let (status, body) = call(&state, Some(&token), "token status").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("stall-7"));

    let (status, body) = call(&state, Some(&token), "order arabica-250g 2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("held ord-"));
    assert!(body["text"].as_str().unwrap().contains("approval card"));
    assert_eq!(
        stores
            .commerce
            .stock_of("demo", "arabica-250g")
            .await
            .unwrap()
            .unwrap()
            .qty,
        8
    );

    let (status, _) = call(&state, Some(&token), "support my beans never arrived").await;
    assert_eq!(status, StatusCode::OK);

    let receipt = uuid::Uuid::new_v4();
    let (status, body) = call(
        &state,
        Some(&token),
        &format!("lead email buyer-9 consent {receipt}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("captured"));
    let records = kernel_stores
        .signals
        .list(
            &adk_kernel::charter::SignalFilter::default()
                .partition("demo.back-office")
                .kind("lead_captured")
                .open(true),
        )
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    let awp_types::SignalKind::LeadCaptured { consent, .. } = &records[0].signal.kind else {
        panic!("a lead record");
    };
    assert_eq!(
        consent.clone(),
        vec![awp_types::ConsentReceiptRef(receipt)],
        "the asserted receipt rides the signal"
    );

    let (status, _) = call(
        &state,
        Some(&token),
        "lead email buyer-9 consent not-a-uuid",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert!(state.tokens.revoke("demo", &token).await.unwrap());
    let (status, _) = call(&state, Some(&token), "order arabica-250g 1").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// The price watch files gaps for drifts and stays silent on match; orders
/// report their own state.
#[tokio::test]
async fn pricecheck_files_drifts_and_orders_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let (state, stores, kernel_stores) = setup(&dir).await;
    let token = state.tokens.issue("demo", "stall-7").await.unwrap();

    let (status, body) = call(&state, Some(&token), "pricecheck").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("match"));

    // Move the list price: the next check files the drift.
    stores
        .commerce
        .set_stock("demo", "arabica-250g", 8, 900_000_000, "KES")
        .await
        .unwrap();
    let (status, body) = call(&state, Some(&token), "pricecheck").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("1 drift"));
    let gaps = kernel_stores
        .signals
        .list(
            &adk_kernel::charter::SignalFilter::default()
                .partition("demo.back-office")
                .kind("content_gap")
                .open(true),
        )
        .await
        .unwrap();
    assert_eq!(gaps.len(), 1);
    assert!(format!("{:?}", gaps[0].signal.kind).contains("arabica-250g"));

    let (_, order) = call(&state, Some(&token), "order mango-crate 1").await;
    let id = order["text"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let (status, body) = call(&state, Some(&token), &format!("order-status {id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["text"].as_str().unwrap().contains("Held"));
}
