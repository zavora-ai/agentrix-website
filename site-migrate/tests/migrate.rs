//! Phase 9 gate: migrate the fixture legacy site end to end — crawl, score 1.0,
//! go-live card with the report attached, approve, admit past the gate — and prove
//! a below-threshold candidate is refused and never served.

use agentrix_site_edge::build_release;
use agentrix_site_inbox::{Card, CardState, CardStore, Decision, Execute};
use agentrix_site_migrate::Fetch;
use agentrix_site_migrate::{
    FileFetch, HttpFetch, MigrationParity, crawl, file_go_live, migrate, score,
};
use agentrix_site_release::{GateRefusal, LaunchGate, ReleaseStore};

const HOME_HTML: &str = r#"<!DOCTYPE html><html><head><title>Bean Scene Roastery</title>
<meta name="description" content="Small-batch coffee roastery.">
</head><body><h1>Bean Scene</h1>
<p>We roast small batches every Tuesday.</p>
<p>Our cafe pours single origins all week.</p>
<a href="https://beans.example/menu.html">Menu</a>
<div data-product="House Blend" data-price="KES 1,800">House Blend</div>
</body></html>"#;

const MENU_HTML: &str = r#"<!DOCTYPE html><html><head><title>Menu — Bean Scene</title></head>
<body><h1>Menu</h1>
<p>Espresso, filter and cold brew on the bar.</p>
<a href="https://beans.example/">Home</a>
<div data-product="Espresso" data-price="KES 350">Espresso</div>
<div data-product="Cold Brew" data-price="KES 450">Cold Brew</div>
</body></html>"#;

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

fn fixture() -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), HOME_HTML).unwrap();
    std::fs::write(dir.path().join("menu.html"), MENU_HTML).unwrap();
    let urls = vec![
        "https://beans.example/".to_string(),
        "https://beans.example/menu.html".to_string(),
    ];
    (dir, urls)
}

#[tokio::test]
async fn full_loop_migrates_scores_cards_and_serves() {
    let (dir, urls) = fixture();
    let fetch = FileFetch::new(dir.path());
    let evidence = crawl(&fetch, &urls).unwrap();
    assert_eq!(evidence.pages.len(), 2);
    assert_eq!(evidence.log.len(), 2);
    assert!(evidence.log.iter().all(|entry| entry.bytes > 0));

    let (candidate, report) = migrate(&evidence);
    assert_eq!(report.overall, 1.0, "{report:?}");
    assert!(report.gaps.is_empty(), "{report:?}");
    assert_eq!(candidate.release.provenance.sources, urls);

    let cards_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    let cards = CardStore::open(&cards_db).await.unwrap();
    let card = file_go_live(&cards, "demo", &candidate, &report)
        .await
        .unwrap();
    assert_eq!(
        card.payload["release"],
        serde_json::json!(candidate.release.digest)
    );
    assert_eq!(card.payload["report"]["overall"], serde_json::json!(1.0));

    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    let decided = cards.get(&card.id).await.unwrap().unwrap();
    assert_eq!(decided.state, CardState::Approved);

    let releases_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("releases.db").display()
    );
    let store = ReleaseStore::open(&releases_db).await.unwrap();
    let gate = LaunchGate::new("demo").with_parity(Box::new(MigrationParity::over(report)));
    let served = gate
        .admit(&store, &cards, &candidate.release, &card.id)
        .await
        .unwrap();
    assert_eq!(served.digest, candidate.release.digest);
    let current = store.current("demo").await.unwrap().unwrap();
    assert_eq!(current.digest, candidate.release.digest);
    assert_eq!(current.pages.len(), 2);
}

#[tokio::test]
async fn below_threshold_candidate_is_refused_and_never_served() {
    let (dir, urls) = fixture();
    let evidence = crawl(&FileFetch::new(dir.path()), &urls).unwrap();
    let (mut candidate, _) = migrate(&evidence);
    // Lose content, a product and a link — then re-digest, so integrity passes
    // and only parity refuses.
    let mut pages = candidate.release.pages.clone();
    pages
        .iter_mut()
        .find(|p| p.slug == "menu")
        .unwrap()
        .facts
        .pop();
    candidate.release = build_release(pages, "migrated", urls.clone());
    candidate.products.get_mut("menu").unwrap().pop();
    candidate.links.get_mut("home").unwrap().pop();
    let report = score(&evidence, &candidate);
    assert!(report.overall < 0.95, "{report:?}");
    assert!(!report.gaps.is_empty());

    let cards_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    let cards = CardStore::open(&cards_db).await.unwrap();
    let card = file_go_live(&cards, "demo", &candidate, &report)
        .await
        .unwrap();
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();

    let releases_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("releases.db").display()
    );
    let store = ReleaseStore::open(&releases_db).await.unwrap();
    let gate = LaunchGate::new("demo").with_parity(Box::new(MigrationParity::over(report)));
    let refused = gate
        .admit(&store, &cards, &candidate.release, &card.id)
        .await;
    assert!(
        matches!(refused, Err(GateRefusal::Parity(_))),
        "{refused:?}"
    );
    assert!(store.current("demo").await.unwrap().is_none());
    assert!(store.list().await.unwrap().is_empty());
}

/// Serves `body` once as `text/html` on loopback, returning the URL.
fn serve_once(body: &'static str) -> String {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = [0_u8; 1024];
        let _ = stream.read(&mut head);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
}

#[test]
fn http_fetch_reads_a_live_loopback_page() {
    let url = serve_once(HOME_HTML);
    let html = HttpFetch::default().fetch(&url).unwrap();
    assert!(html.contains("Bean Scene Roastery"), "{html}");
}

#[test]
fn http_fetch_refuses_errors() {
    let result = HttpFetch::default().fetch("http://127.0.0.1:1/nothing-here");
    assert!(result.is_err());
}

#[test]
fn open_fetcher_fails_fast_without_network() {
    let fetcher = HttpFetch::default().with_breaker(std::sync::Arc::new(
        agentrix_site_defense::Breaker::new("fetch-test", 1, std::time::Duration::from_secs(60)),
    ));
    assert!(fetcher.fetch("http://127.0.0.1:1/nothing").is_err());
    let err = fetcher
        .fetch("http://127.0.0.1:1/nothing")
        .unwrap_err()
        .to_string();
    assert!(err.contains("circuit open"), "{err}");
}
