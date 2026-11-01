//! Phase 6 gate: a breached KPI demotes its area before the next wake; the cap
//! halts spend and holds posts; ads record zero spend; the Analyst files.

use agentrix_site_concierge::{LeadSignal, ReviewSignal};
use agentrix_site_growth::{
    Direction, Growth, InstagramSocial, JobLevel, LogSocial, MastodonSocial, PostOutcome,
    SocialPoster, nurture, publish_reply, record_ad, weekly_review,
};
use agentrix_site_inbox::{CardKind, CardStore};

async fn setup(dir: &tempfile::TempDir) -> (Growth, CardStore) {
    let growth_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("growth.db").display()
    );
    let cards_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    (
        Growth::open(&growth_db).await.unwrap(),
        CardStore::open(&cards_db).await.unwrap(),
    )
}

#[tokio::test]
async fn breach_demotes_its_area_only() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, _) = setup(&dir).await;
    growth
        .set_target("demo", "qualified_leads", 100.0, Direction::Up, "revenue")
        .await
        .unwrap();
    growth
        .set_target("demo", "review_rating", 4.0, Direction::Up, "growth")
        .await
        .unwrap();
    growth
        .record("demo", "qualified_leads", 40.0)
        .await
        .unwrap();
    growth.record("demo", "review_rating", 4.8).await.unwrap();

    let breaches = growth.breaches("demo").await.unwrap();
    assert_eq!(breaches.len(), 1);
    assert_eq!(breaches[0].kpi, "qualified_leads");

    let moved = growth.apply_breaches("demo").await.unwrap();
    assert_eq!(moved.len(), 1, "revenue holds only nurture");
    assert_eq!(moved[0].0, "nurture");
    assert_eq!(moved[0].1, JobLevel::ActWithinBudget);
    assert_eq!(moved[0].2, JobLevel::ActWithApproval);
    // Still breached: a second wake demotes again, other areas untouched.
    let moved = growth.apply_breaches("demo").await.unwrap();
    assert_eq!(moved[0].2, JobLevel::ActReversibleOnly);
    assert_eq!(
        growth.level("demo", "freshness").await.unwrap(),
        JobLevel::ActWithinBudget
    );
    assert_eq!(
        growth.level("demo", "protect").await.unwrap(),
        JobLevel::ActReversibleOnly
    );
}

#[tokio::test]
async fn cap_halts_spend_and_holds_posts() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, cards) = setup(&dir).await;
    growth.set_cap("demo", 100).await.unwrap();
    growth
        .set_level("demo", "social", JobLevel::ActWithinBudget)
        .await
        .unwrap();
    let social = LogSocial::default();
    let review = ReviewSignal {
        platform: "maps".into(),
        rating: 2,
        excerpt: "slow".into(),
    };

    // Two posts fit (2 x 50); the third is held, not half-sent.
    for _ in 0..2 {
        let outcome = publish_reply(
            &growth,
            "demo",
            &cards,
            &social,
            &review,
            "Thanks — we hear you.",
        )
        .await
        .unwrap();
        assert!(matches!(outcome, PostOutcome::Posted(_)));
    }
    let held = publish_reply(&growth, "demo", &cards, &social, &review, "One more.")
        .await
        .unwrap();
    let card = match held {
        PostOutcome::Held(card) => card,
        PostOutcome::Posted(_) => panic!("the cap must hold the third post"),
    };
    assert_eq!(social.posts().len(), 2);
    let (_, spent, _) = growth.budget("demo").await.unwrap();
    assert_eq!(spent, 100);
    assert_eq!(card.kind, CardKind::Draft);

    // A past-cap reservation names its shortfall and holds nothing.
    let refused = growth.reserve("demo", 1, "anything").await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn sales_rep_drafts_and_ads_never_spend() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, cards) = setup(&dir).await;
    growth.set_cap("demo", 1_000_000).await.unwrap();
    let leads = vec![LeadSignal {
        channel: "email".into(),
        contact_ref: "c-1".into(),
    }];
    let filed = nurture("demo", &cards, &leads).await.unwrap();
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0].1.role, "Sales Rep");

    record_ad(
        &growth,
        "demo",
        "spring",
        "recorded-social",
        "Spring beans are here.",
        50_000,
    )
    .await
    .unwrap();
    let ads = growth.ads("demo").await.unwrap();
    assert_eq!(ads.len(), 1);
    assert_eq!(ads[0].spent_micro, 0);
    let (_, spent, _) = growth.budget("demo").await.unwrap();
    assert_eq!(spent, 0, "recording an ad moves no money");
}

#[tokio::test]
async fn analyst_files_the_weekly_review() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, cards) = setup(&dir).await;
    growth.set_cap("demo", 100).await.unwrap();
    growth
        .set_target("demo", "bookings", 20.0, Direction::Up, "revenue")
        .await
        .unwrap();
    growth.record("demo", "bookings", 9.0).await.unwrap();
    let reservation = growth.reserve("demo", 60, "posts").await.unwrap();
    growth.commit(&reservation.id).await.unwrap();

    let (doc, card) = weekly_review(&growth, "demo", &cards).await.unwrap();
    assert_eq!(card.kind, CardKind::Digest);
    assert_eq!(card.role, "Analyst");
    assert_eq!((doc.spent_micro, doc.cap_micro), (60, 100));
    assert_eq!(doc.breaches.len(), 1);
    assert!(doc.levels.iter().any(|(job, _)| job == "nurture"));
    assert_eq!(card.payload["spent_micro"], serde_json::json!(60));
}

#[test]
fn live_poster_endpoints_are_pinned() {
    let instagram =
        InstagramSocial::new("12345", "token", "https://cdn.example/brand.png").unwrap();
    assert_eq!(
        instagram.media_url(),
        "https://graph.facebook.com/v21.0/12345/media"
    );
    assert_eq!(
        instagram.publish_url(),
        "https://graph.facebook.com/v21.0/12345/media_publish"
    );
    let mastodon = MastodonSocial::new("https://mastodon.social/", "token").unwrap();
    assert_eq!(
        mastodon.status_url(),
        "https://mastodon.social/api/v1/statuses"
    );
}

#[test]
#[ignore = "needs META_IG_* credentials and publishes a real post"]
fn instagram_live_post() {
    let poster = InstagramSocial::from_env().unwrap();
    let id = poster
        .post("Agentrix validation post — delete me.")
        .unwrap();
    assert!(!id.is_empty());
}

#[test]
#[ignore = "needs MASTODON_* credentials and publishes a real post"]
fn mastodon_live_post() {
    let poster = MastodonSocial::from_env().unwrap();
    let id = poster
        .post("Agentrix validation post — delete me.")
        .unwrap();
    assert!(!id.is_empty());
}

#[test]
fn open_poster_fails_fast_without_network() {
    use agentrix_site_defense::Breaker;
    use std::sync::Arc;
    use std::time::Duration;
    let poster = MastodonSocial::new("http://127.0.0.1:1", "token")
        .unwrap()
        .with_breaker(Arc::new(Breaker::new(
            "mastodon-test",
            1,
            Duration::from_secs(60),
        )));
    assert!(poster.post("hi").is_err());
    let err = poster.post("hi").unwrap_err().to_string();
    assert!(err.contains("circuit open"), "{err}");
}

#[tokio::test]
async fn below_level_posts_hold_on_a_card() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, cards) = setup(&dir).await;
    growth.set_cap("demo", 1_000_000).await.unwrap();
    let social = LogSocial::default();
    let review = ReviewSignal {
        platform: "maps".into(),
        rating: 2,
        excerpt: "slow".into(),
    };

    // Social starts at Propose: the live post refuses and cards instead.
    assert_eq!(
        growth.level("demo", "social").await.unwrap(),
        JobLevel::Propose
    );
    let held = publish_reply(&growth, "demo", &cards, &social, &review, "Thanks.")
        .await
        .unwrap();
    let card = match held {
        PostOutcome::Held(card) => card,
        PostOutcome::Posted(_) => panic!("below-level posting must hold"),
    };
    assert!(card.title.contains("ladder"), "{}", card.title);
    assert!(social.posts().is_empty(), "nothing sent");
    let (_, spent, _) = growth.budget("demo").await.unwrap();
    assert_eq!(spent, 0, "nothing reserved");

    // Promoted, the same call posts.
    growth
        .set_level("demo", "social", JobLevel::ActWithinBudget)
        .await
        .unwrap();
    let posted = publish_reply(&growth, "demo", &cards, &social, &review, "Thanks.")
        .await
        .unwrap();
    assert!(matches!(posted, PostOutcome::Posted(_)));
    assert_eq!(social.posts().len(), 1);
}

#[tokio::test]
async fn paused_jobs_hold_until_resumed() {
    let dir = tempfile::tempdir().unwrap();
    let (growth, cards) = setup(&dir).await;
    growth.set_cap("demo", 1_000_000).await.unwrap();
    growth
        .set_level("demo", "social", JobLevel::ActWithinBudget)
        .await
        .unwrap();
    let social = LogSocial::default();
    let review = ReviewSignal {
        platform: "maps".into(),
        rating: 4,
        excerpt: "fine".into(),
    };

    growth
        .pause_job("demo", "social", "the rating guardrail breached")
        .await
        .unwrap();
    assert_eq!(
        growth
            .paused_reason("demo", "social")
            .await
            .unwrap()
            .as_deref(),
        Some("the rating guardrail breached")
    );
    let held = publish_reply(&growth, "demo", &cards, &social, &review, "Thanks.")
        .await
        .unwrap();
    let card = match held {
        PostOutcome::Held(card) => card,
        PostOutcome::Posted(_) => panic!("a paused job must hold"),
    };
    assert!(card.title.contains("paused"), "{}", card.title);
    assert!(social.posts().is_empty());

    assert!(growth.resume_job("demo", "social").await.unwrap());
    assert!(!growth.resume_job("demo", "social").await.unwrap());
    let posted = publish_reply(&growth, "demo", &cards, &social, &review, "Thanks.")
        .await
        .unwrap();
    assert!(matches!(posted, PostOutcome::Posted(_)));
}

#[tokio::test]
async fn enforce_demotes_before_it_refuses() {
    use agentrix_site_growth::BudgetError;

    let dir = tempfile::tempdir().unwrap();
    let (growth, _) = setup(&dir).await;
    growth
        .set_target("demo", "qualified_leads", 100.0, Direction::Up, "revenue")
        .await
        .unwrap();
    growth
        .record("demo", "qualified_leads", 40.0)
        .await
        .unwrap();
    assert_eq!(
        growth.level("demo", "nurture").await.unwrap(),
        JobLevel::ActWithinBudget
    );

    // The breach demotes first; the lowered level then refuses the act.
    let refused = growth
        .enforce("demo", "nurture", JobLevel::ActWithinBudget, "act")
        .await;
    assert!(
        matches!(refused, Err(BudgetError::BelowLevel { .. })),
        "{refused:?}"
    );
    assert_eq!(
        growth.level("demo", "nurture").await.unwrap(),
        JobLevel::ActWithApproval
    );
    // Still breached: the next action demotes again.
    let _ = growth
        .enforce("demo", "nurture", JobLevel::ActWithinBudget, "act")
        .await;
    assert_eq!(
        growth.level("demo", "nurture").await.unwrap(),
        JobLevel::ActReversibleOnly
    );
    // Unbreached areas never move.
    assert_eq!(
        growth.level("demo", "freshness").await.unwrap(),
        JobLevel::ActWithinBudget
    );
}
