//! Signal-to-card loops, all offline: a filed signal wakes its job through the
//! started kernel, the scripted agent pulls the pending work and files the held
//! card, and filing closes the record. Each loop must complete within the
//! probe's 60-second budget.

use std::sync::Arc;
use std::time::{Duration, Instant};

use adk_kernel::charter::{SignalFilter, SignalOutcome, SignalSink, SignalStore};
use agentrix_site_serve::{ProductStores, ServeConfig, agents, seed_demo, signals, workflows};

const BUDGET: Duration = Duration::from_secs(60);

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

fn partition(app: &str) -> adk_kernel::PartitionId {
    format!("{app}.back-office").into()
}

fn gap_kind() -> awp_types::SignalKind {
    awp_types::SignalKind::ContentGap {
        topic: awp_types::Bounded::truncated("Weekend opening hours", 120),
        question_count: 2,
        samples: vec![awp_types::Bounded::truncated("Are you open Saturday?", 200)],
    }
}

fn review_kind() -> awp_types::SignalKind {
    awp_types::SignalKind::ReviewSeen {
        platform: "mastodon".into(),
        rating: 5,
        excerpt: awp_types::Bounded::truncated("Fixed our books in a week.", 500),
    }
}

fn escalation_kind() -> awp_types::SignalKind {
    awp_types::SignalKind::SupportEscalation {
        reason: awp_types::ReasonCode("asked_for_a_human".into()),
        contact_ref: awp_types::ContactRef("visitor:v9".into()),
    }
}

fn lead_kind() -> awp_types::SignalKind {
    awp_types::SignalKind::LeadCaptured {
        lead_ref: awp_types::LeadRef("lead-1".into()),
        source: awp_types::Channel::Email,
        consent: Vec::new(),
        answers: Default::default(),
    }
}

fn envelope(site: &str, kind: awp_types::SignalKind, by: &str) -> awp_types::Signal {
    awp_types::Signal {
        id: uuid::Uuid::new_v4(),
        kind,
        emitted_by: format!("urn:adk:agent:{site}/{by}"),
        emitted_at: chrono::Utc::now(),
    }
}

/// Pre-files `kind` through an unattached sink: the record exists, nothing
/// wakes. Returns its id for the script to acknowledge.
async fn prefile(
    kernel_stores: &workflows::KernelStores,
    config: &ServeConfig,
    kind: awp_types::SignalKind,
) -> String {
    let sink = SignalSink::new(
        kernel_stores.signals.clone(),
        kernel_stores.agenda.clone(),
        partition(&config.app),
    );
    match sink
        .emit(envelope(&config.site, kind, "concierge"))
        .await
        .unwrap()
    {
        SignalOutcome::Filed { item, .. } => item,
        other => panic!("prefile must file, got {other:?}"),
    }
}

/// The open records of one kind: empty once the loop acknowledges its work.
async fn open_of(
    kernel_stores: &workflows::KernelStores,
    app: &str,
    kind: &str,
) -> Vec<adk_kernel::charter::SignalRecord> {
    kernel_stores
        .signals
        .list(
            &SignalFilter::default()
                .partition(partition(app))
                .kind(kind)
                .open(true),
        )
        .await
        .unwrap()
}

async fn await_card(
    stores: &ProductStores,
    site: &str,
    role: &str,
    since: Instant,
) -> agentrix_site_inbox::Card {
    loop {
        let open = stores.cards.list_open(site).await.unwrap();
        if let Some(card) = open.iter().find(|card| card.role == role) {
            assert!(
                since.elapsed() < BUDGET,
                "the {role} loop must close within {BUDGET:?}"
            );
            return card.clone();
        }
        assert!(
            since.elapsed() < BUDGET,
            "timed out waiting for a {role} card"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn setup(dir: &tempfile::TempDir) -> (ServeConfig, ProductStores, workflows::KernelStores) {
    let config = config(dir);
    std::fs::create_dir_all(&config.data).unwrap();
    let stores = ProductStores::open(&config.data).await.unwrap();
    seed_demo(&stores, &config.site).await.unwrap();
    let kernel_stores = workflows::open_kernel_stores(&config.data).await.unwrap();
    (config, stores, kernel_stores)
}

fn scripted(replies: Vec<adk_model::ScriptedResponse>) -> Arc<dyn adk_core::Llm> {
    Arc::new(adk_model::ScriptedLlm::new("e2e", replies))
}

async fn start_kernel(
    config: &ServeConfig,
    stores: &ProductStores,
    kernel_stores: &workflows::KernelStores,
    model: Arc<dyn adk_core::Llm>,
) -> (adk_kernel::Kernel, SignalSink) {
    let sessions: Arc<dyn adk_session::SessionService> =
        Arc::new(adk_session::InMemorySessionService::new());
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        stores,
        sessions,
        model,
        kernel_stores.signals.clone(),
    )
    .unwrap();
    let current = agentrix_site_edge::Current::new(
        stores
            .releases
            .current(&config.site)
            .await
            .unwrap()
            .unwrap(),
    );
    let workflow_runs = Arc::new(workflows::SiteWorkflows::new(
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
        kernel_stores,
        runners,
        workflow_runs,
    )
    .await
    .unwrap();
    kernel.start().await.unwrap();
    let sink = SignalSink::for_kernel(
        &kernel,
        kernel_stores.signals.clone(),
        partition(&config.app),
    );
    (kernel, sink)
}

/// A content gap wakes the Content Editor: the draft lands held and the record
/// closes.
#[tokio::test]
async fn content_gap_wakes_the_editor_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    let record = prefile(&kernel_stores, &config, gap_kind()).await;
    let model = scripted(vec![
        adk_model::ScriptedResponse::function_call("next_gap", serde_json::json!({})),
        adk_model::ScriptedResponse::function_call(
            "file_content_draft",
            serde_json::json!({
                "page_slug": "contact",
                "facts": [{"id": "hours", "text": "Open Saturday 9 to 1."}],
                "rationale": "visitors keep asking",
                "gap_record": record,
            }),
        ),
        adk_model::ScriptedResponse::text("draft filed"),
    ]);
    let (kernel, sink) = start_kernel(&config, &stores, &kernel_stores, model).await;
    let since = Instant::now();
    match sink
        .emit(envelope(&config.site, gap_kind(), "concierge"))
        .await
        .unwrap()
    {
        SignalOutcome::Counted { item, count } => {
            assert_eq!(item, record);
            assert_eq!(count, 2);
        }
        other => panic!("the wake emit must count on the prefiled record, got {other:?}"),
    }
    let card = await_card(&stores, &config.site, "Content Editor", since).await;
    assert!(card.title.contains("contact"), "{}", card.title);
    assert!(
        open_of(&kernel_stores, &config.app, "content_gap")
            .await
            .is_empty(),
        "filing acknowledges the gap"
    );
    kernel.stop().await.unwrap();
}

/// A review sighting wakes the Reputation Manager down the shared runner's
/// social branch.
#[tokio::test]
async fn review_seen_wakes_replies_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    let record = prefile(&kernel_stores, &config, review_kind()).await;
    let model = scripted(vec![
        adk_model::ScriptedResponse::function_call("next_review", serde_json::json!({})),
        adk_model::ScriptedResponse::function_call(
            "file_reply_draft",
            serde_json::json!({
                "platform": "mastodon",
                "reply": "Thank you — we are glad the books balance.",
                "review_record": record,
            }),
        ),
        adk_model::ScriptedResponse::text("reply drafted"),
    ]);
    let (kernel, sink) = start_kernel(&config, &stores, &kernel_stores, model).await;
    let since = Instant::now();
    signals::emit_review(
        &sink,
        &config.site,
        &signals::ReviewIntake {
            platform: "mastodon".into(),
            rating: 5,
            excerpt: "Fixed our books in a week.".into(),
        },
    )
    .await
    .unwrap();
    let card = await_card(&stores, &config.site, "Reputation Manager", since).await;
    assert!(card.title.contains("mastodon"), "{}", card.title);
    assert!(
        open_of(&kernel_stores, &config.app, "review_seen")
            .await
            .is_empty(),
        "filing acknowledges the sighting"
    );
    kernel.stop().await.unwrap();
}

/// An escalation wakes the Chief of Staff: retrieve, then the held answer.
#[tokio::test]
async fn escalation_wakes_the_steward_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    let record = prefile(&kernel_stores, &config, escalation_kind()).await;
    let model = scripted(vec![
        adk_model::ScriptedResponse::function_call("next_escalation", serde_json::json!({})),
        adk_model::ScriptedResponse::function_call(
            "retrieve_chunks",
            serde_json::json!({"query": "weekend hours"}),
        ),
        adk_model::ScriptedResponse::function_call(
            "file_answer_draft",
            serde_json::json!({
                "answer": "We are open Saturday 9 to 1.",
                "escalation_record": record,
            }),
        ),
        adk_model::ScriptedResponse::text("answer drafted"),
    ]);
    let (kernel, sink) = start_kernel(&config, &stores, &kernel_stores, model).await;
    let since = Instant::now();
    match sink
        .emit(envelope(&config.site, escalation_kind(), "concierge"))
        .await
        .unwrap()
    {
        SignalOutcome::Counted { item, .. } => assert_eq!(item, record),
        other => panic!("the wake emit must count on the prefiled record, got {other:?}"),
    }
    let card = await_card(&stores, &config.site, "Chief of Staff", since).await;
    assert!(card.title.contains("escalation"), "{}", card.title);
    assert!(
        open_of(&kernel_stores, &config.app, "support_escalation")
            .await
            .is_empty(),
        "filing acknowledges the escalation"
    );
    kernel.stop().await.unwrap();
}

/// A captured lead runs the outreach team: qualify, then the held follow-up.
/// The Outreach trigger is cron, so the team runs directly over the open lead.
#[tokio::test]
async fn captured_lead_runs_the_outreach_team_end_to_end() {
    use adk_core::{Content, SessionId, UserId};
    use futures::StreamExt as _;

    let dir = tempfile::tempdir().unwrap();
    let (config, stores, kernel_stores) = setup(&dir).await;
    let record = prefile(&kernel_stores, &config, lead_kind()).await;
    let model = scripted(vec![
        adk_model::ScriptedResponse::function_call("next_lead", serde_json::json!({})),
        adk_model::ScriptedResponse::text(format!(
            "QUALIFIED yes lead-1 email {record} — asked for pricing."
        )),
        adk_model::ScriptedResponse::function_call(
            "file_followup",
            serde_json::json!({
                "lead_ref": "lead-1",
                "channel": "email",
                "message": "Following up on your pricing enquiry.",
                "lead_record": record,
            }),
        ),
        adk_model::ScriptedResponse::text("follow-up drafted"),
    ]);
    let sessions: Arc<dyn adk_session::SessionService> =
        Arc::new(adk_session::InMemorySessionService::new());
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        &stores,
        sessions.clone(),
        model,
        kernel_stores.signals.clone(),
    )
    .unwrap();
    let revenue = runners
        .iter()
        .find(|(target, _)| target == "revenue")
        .expect("revenue runner")
        .1
        .clone();
    let session = sessions
        .create(adk_session::CreateRequest {
            app_name: config.app.clone(),
            user_id: "owner".to_string(),
            session_id: None,
            state: Default::default(),
        })
        .await
        .unwrap();
    let since = Instant::now();
    let mut events = revenue
        .run(
            UserId::new("owner").unwrap(),
            SessionId::new(session.id().to_string()).unwrap(),
            Content::new("user").with_text("{\"job\": \"nurture\"}"),
        )
        .await
        .unwrap();
    while let Some(event) = events.next().await {
        event.unwrap();
    }
    assert!(since.elapsed() < BUDGET, "the nurture loop must close fast");
    let open = stores.cards.list_open(&config.site).await.unwrap();
    assert!(
        open.iter()
            .any(|card| card.role == "Sales Rep" && card.title.contains("lead-1")),
        "a held follow-up exists"
    );
    assert!(
        open_of(&kernel_stores, &config.app, "lead_captured")
            .await
            .is_empty(),
        "filing acknowledges the lead"
    );
}
