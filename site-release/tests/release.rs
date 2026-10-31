//! Phase 3 gate: publish N+1 past the LaunchGate, roll back to N, and prove both
//! audiences serve N from the next request on. Refusals name their reason and
//! move nothing.

use agentrix_site_edge::{Current, RateLimit, build_release, business_site, router_on};
use agentrix_site_inbox::{Card, CardKind, CardStore, Decision, Execute, NewCard};
use agentrix_site_release::{GateRefusal, LaunchGate, ReleaseStore};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

async fn stores(dir: &tempfile::TempDir) -> (ReleaseStore, CardStore) {
    let releases = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("releases.db").display()
    );
    let cards = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    (
        ReleaseStore::open(&releases).await.unwrap(),
        CardStore::open(&cards).await.unwrap(),
    )
}

async fn approved_card(cards: &CardStore, digest: &str) -> String {
    let card = cards
        .file(NewCard::new(
            "demo",
            CardKind::Approval,
            "Chief of Staff",
            format!("Release {}", &digest[..12]),
            serde_json::json!({ "release": digest }),
        ))
        .await
        .unwrap();
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    card.id
}

async fn body_text(app: axum::Router, uri: &str) -> (StatusCode, String) {
    let answer = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = answer.status();
    let bytes = axum::body::to_bytes(answer.into_body(), 256 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn publish_then_rollback_moves_both_audiences() {
    let dir = tempfile::tempdir().unwrap();
    let (store, cards) = stores(&dir).await;
    let gate = LaunchGate::new("demo");

    let mut pages_n1 = business_site();
    pages_n1[0].facts[0].text = "Founded in 2020.".into();
    let release_n = build_release(business_site(), "business", vec!["fixture".into()]);
    let release_n1 = build_release(pages_n1, "business", vec!["fixture".into()]);
    assert_ne!(release_n.digest, release_n1.digest);

    for release in [&release_n, &release_n1] {
        let card = approved_card(&cards, &release.digest).await;
        gate.admit(&store, &cards, release, &card).await.unwrap();
    }
    assert_eq!(store.list().await.unwrap().len(), 2);

    // Both audiences serve N+1.
    let current = Current::new(store.current("demo").await.unwrap().unwrap());
    let app = router_on(&current, RateLimit::default());
    let (status, page) = body_text(app.clone(), "/p/home").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Founded in 2020."), "{page}");
    let (_, twin) = body_text(app.clone(), "/twin/home").await;
    assert!(twin.contains("Founded in 2020."), "{twin}");

    // Roll back to N: the pointer moves, both audiences follow.
    let rolled = store.set_current("demo", &release_n.digest).await.unwrap();
    current.set(rolled);
    let (status, page) = body_text(app.clone(), "/p/home").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Founded in 2019."), "{page}");
    assert!(!page.contains("Founded in 2020."), "{page}");
    let (_, twin) = body_text(app, "/twin/home").await;
    assert!(twin.contains("Founded in 2019."), "{twin}");

    let events = store.events().await.unwrap();
    assert!(events.iter().any(|e| e.kind == "admitted"));
    assert!(events.iter().any(|e| e.kind == "current"));
    assert!(!events.iter().any(|e| e.kind == "refused"));
}

#[tokio::test]
async fn refusals_name_their_reason_and_move_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (store, cards) = stores(&dir).await;
    let release = build_release(business_site(), "business", vec!["fixture".into()]);

    // No approval card.
    let gate = LaunchGate::new("demo");
    let refused = gate.admit(&store, &cards, &release, "card-missing").await;
    assert!(
        matches!(refused, Err(GateRefusal::Approval(_))),
        "{refused:?}"
    );

    // Tampered digest.
    let mut tampered = build_release(business_site(), "business", vec!["fixture".into()]);
    tampered.digest = "0".repeat(64);
    let card = approved_card(&cards, &tampered.digest).await;
    let refused = gate.admit(&store, &cards, &tampered, &card).await;
    assert_eq!(refused, Err(GateRefusal::Integrity));

    // Frozen.
    let card = approved_card(&cards, &release.digest).await;
    let refused = LaunchGate::new("demo")
        .frozen()
        .admit(&store, &cards, &release, &card)
        .await;
    assert_eq!(refused, Err(GateRefusal::Frozen));

    assert!(store.current("demo").await.unwrap().is_none());
    assert!(store.list().await.unwrap().is_empty());
    let events = store.events().await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == "refused").count(), 3);
}

#[tokio::test]
async fn rollback_to_unknown_digest_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = stores(&dir).await;
    let missing = "f".repeat(64);
    assert!(store.set_current("demo", &missing).await.is_err());
}
