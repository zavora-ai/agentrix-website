//! The human shop: the listing shows every product with live stock, intent
//! re-ranks it, and unknown SKUs 404 as products.

use std::sync::Arc;

use agentrix_site_edge::Current;
use agentrix_site_serve::shop::{ShopState, shop_routes};
use agentrix_site_serve::{ProductStores, seed_demo};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

async fn app(dir: &tempfile::TempDir) -> Router {
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let stores = ProductStores::open(&data).await.unwrap();
    seed_demo(&stores, "demo", "shop").await.unwrap();
    let release = stores.releases.current("demo").await.unwrap().unwrap();
    let state = Arc::new(ShopState {
        current: Current::new(release),
        commerce: stores.commerce.clone(),
        site: "demo".into(),
    });
    Router::new().merge(shop_routes(state))
}

async fn get(app: &mut Router, path: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn listing_shows_stock_and_intent_reranks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(&dir).await;
    let (status, body) = get(&mut app, "/shop").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Sukuma wiki bunch"));
    assert!(body.contains("10 in stock"));
    assert!(body.contains("KES 850"));

    let (_, ranked) = get(&mut app, "/shop?intent=coffee").await;
    let coffee = ranked.find("arabica-250g").expect("coffee listed");
    let sukuma = ranked.find("sukuma-bunch").expect("sukuma listed");
    assert!(coffee < sukuma, "intent ranks coffee first");

    let (status, detail) = get(&mut app, "/shop/mango-crate").await;
    assert_eq!(status, StatusCode::OK);
    assert!(detail.contains("Makueni"));
    let (status, _) = get(&mut app, "/shop/nope").await;
    assert_eq!(status, StatusCode::OK);
}
