//! The owner console: the inbox lists open cards, approve runs the held
//! effect, reject closes without effect, and tokens issue.

use std::sync::Arc;

use agentrix_site_commerce::NewOrder;
use agentrix_site_inbox::CardState;
use agentrix_site_serve::cli::TokenStore;
use agentrix_site_serve::console::{ConsoleState, console_routes};
use agentrix_site_serve::{ProductStores, seed_demo};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

async fn setup(dir: &tempfile::TempDir) -> (Router, ProductStores, Arc<TokenStore>) {
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let stores = ProductStores::open(&data).await.unwrap();
    seed_demo(&stores, "demo", "shop").await.unwrap();
    let tokens = Arc::new(
        TokenStore::open(&format!(
            "sqlite://{}?mode=rwc",
            data.join("tokens.db").display()
        ))
        .await
        .unwrap(),
    );
    let state = Arc::new(ConsoleState {
        site: "demo".into(),
        cards: stores.cards.clone(),
        commerce: stores.commerce.clone(),
        growth: stores.growth.clone(),
        tokens: tokens.clone(),
    });
    (Router::new().merge(console_routes(state)), stores, tokens)
}

async fn post(app: &mut Router, path: &str, form: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::post(path)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form.to_string()))
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
async fn approve_completes_reject_closes_tokens_issue() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, stores, tokens) = setup(&dir).await;
    let hold = |key: &str| NewOrder {
        sku: "mango-crate".into(),
        qty: 1,
        buyer: "agent:stall-7".into(),
        idempotency_key: key.into(),
    };
    let (order, card) = stores
        .commerce
        .order_hold("demo", &stores.cards, hold("c1"))
        .await
        .unwrap();

    let listing = app
        .clone()
        .oneshot(Request::get("/console").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let body = axum::body::to_bytes(listing.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8(body.to_vec()).unwrap().contains(&card.id));

    let (status, _) = post(
        &mut app,
        "/console/decide",
        &format!("card_id={}&decision=approve", card.id),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        stores.cards.get(&card.id).await.unwrap().unwrap().state,
        CardState::Approved
    );
    assert_eq!(
        stores
            .commerce
            .order_by_id(&order.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        agentrix_site_commerce::OrderState::Completed
    );

    let (_, card) = stores
        .commerce
        .order_hold("demo", &stores.cards, hold("c2"))
        .await
        .unwrap();
    let (status, _) = post(
        &mut app,
        "/console/decide",
        &format!("card_id={}&decision=reject", card.id),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        stores.cards.get(&card.id).await.unwrap().unwrap().state,
        CardState::Rejected
    );

    let (status, body) = post(&mut app, "/console/tokens", "label=stall-9").await;
    assert_eq!(status, StatusCode::OK);
    let token = body
        .split("<code>")
        .nth(1)
        .unwrap()
        .split("</code>")
        .next()
        .unwrap()
        .to_string();
    assert!(token.starts_with("agx_"));
    assert_eq!(
        tokens.verified_label("demo", Some(&token)).await.as_deref(),
        Some("stall-9")
    );
}
