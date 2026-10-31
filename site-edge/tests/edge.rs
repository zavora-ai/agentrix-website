//! Phase 1 gate: the digest match passes on the business site, twins serve within
//! budget, the CLI answers reads and refuses writes, and the rate limit bites.

use agentrix_site_edge::{
    RateLimit, build_release, business_site, digest_match, router, run_command,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

fn release() -> agentrix_site_edge::Release {
    build_release(business_site(), "business", vec!["fixture".into()])
}

#[test]
fn digest_is_stable_and_match_passes() {
    let first = release();
    let second = release();
    assert_eq!(first.digest, second.digest, "same inputs, same digest");
    assert_eq!(digest_match(&first), Ok(()));
}

#[test]
fn tampered_twin_fails_loudly() {
    let mut release = release();
    release.pages[0].facts[0].text = "Founded in 1725.".into();
    // The digest no longer matches the pages, so the gate must fail.
    assert!(digest_match(&release).is_err());
}

#[test]
fn cli_reads_and_refuses() {
    let release = release();
    let list = run_command(&release, "list");
    assert!(list.ok);
    assert!(list.text.contains("home"));
    let get = run_command(&release, "get services");
    assert!(get.ok);
    assert!(get.text.contains("Services and fees"));
    let twin = run_command(&release, "twin contact");
    assert!(twin.ok);
    assert_eq!(twin.data["slug"], serde_json::json!("contact"));
    for refused in ["publish home", "delete contact", "approve 42", "drop table"] {
        let answer = run_command(&release, refused);
        assert!(!answer.ok, "{refused} must be refused");
        assert!(
            answer.text.contains("Refused"),
            "{refused}: {}",
            answer.text
        );
    }
    let unknown = run_command(&release, "get nowhere");
    assert!(!unknown.ok);
}

#[tokio::test]
async fn edge_serves_pages_twins_and_cli() {
    let app = router(release(), RateLimit::default());
    let page = app
        .clone()
        .oneshot(Request::get("/p/services").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.headers().contains_key("x-release-digest"));

    let started = std::time::Instant::now();
    let twin = app
        .clone()
        .oneshot(Request::get("/twin/services").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "twin within budget"
    );
    assert_eq!(twin.status(), StatusCode::OK);
    let body = axum::body::to_bytes(twin.into_body(), 64 * 1024)
        .await
        .unwrap();
    let twin: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(twin["facts"].as_array().unwrap().len(), 3);

    let cli = app
        .oneshot(
            Request::post("/cli")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"twin home"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cli.status(), StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_bites() {
    let app = router(release(), RateLimit { max_per_minute: 3 });
    for _ in 0..3 {
        let answer = app
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(answer.status(), StatusCode::OK);
    }
    let limited = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key("retry-after"));
}
