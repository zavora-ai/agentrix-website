//! Phase 5 gate: intake holds every reply; a tier-above send without approval is
//! refused and carded; approving the card sends exactly once.

use std::sync::Arc;

use agentrix_site_inbox::{Card, CardState, CardStore, Decision, Execute};
use agentrix_site_mail::{FileMailbox, LogSender, Outbox, SendError, Tier};

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

fn mailbox(dir: &tempfile::TempDir) -> FileMailbox {
    let box_dir = dir.path().join("mailbox");
    std::fs::create_dir(&box_dir).unwrap();
    std::fs::write(
        box_dir.join("01.json"),
        r#"{"id":"m1","from":"visitor@example.com","subject":"Opening hours","body":"What time do you open on Saturdays?"}"#,
    )
    .unwrap();
    std::fs::write(
        box_dir.join("02.json"),
        r#"{"id":"m2","from":"client@example.com","subject":"Quote for monthly books","body":"Please send a quote for monthly bookkeeping."}"#,
    )
    .unwrap();
    std::fs::write(
        box_dir.join("03.json"),
        r#"{"id":"m3","from":"staff@example.com","subject":"Contract salary","body":"What salary does the new contract offer?"}"#,
    )
    .unwrap();
    FileMailbox::new(box_dir)
}

async fn setup(dir: &tempfile::TempDir) -> (Outbox, CardStore, Arc<LogSender>) {
    let sender = Arc::new(LogSender::default());
    let outbox_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("outbox.db").display()
    );
    let cards_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    let outbox = Outbox::open(&outbox_db, sender.clone()).await.unwrap();
    let cards = CardStore::open(&cards_db).await.unwrap();
    (outbox, cards, sender)
}

#[tokio::test]
async fn intake_holds_everything_and_tiers_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let (outbox, cards, sender) = setup(&dir).await;
    let filed = outbox.intake("demo", &cards, &mailbox(&dir)).await.unwrap();
    assert_eq!(filed.len(), 3);
    assert_eq!(filed[0].tier, Tier::Public);
    assert_eq!(filed[1].tier, Tier::Standard);
    assert_eq!(filed[2].tier, Tier::Sensitive);
    assert!(filed.iter().all(|draft| draft.card_id.starts_with("card-")));
    assert!(sender.sent().is_empty(), "intake sends nothing");
    assert_eq!(cards.list_open("demo").await.unwrap().len(), 3);
    assert_eq!(outbox.pending().await.unwrap().len(), 3);
}

#[tokio::test]
async fn tier_above_send_is_refused_carded_then_approved_once() {
    let dir = tempfile::tempdir().unwrap();
    let (outbox, cards, sender) = setup(&dir).await;
    let filed = outbox.intake("demo", &cards, &mailbox(&dir)).await.unwrap();
    let quote = filed
        .iter()
        .find(|draft| draft.tier == Tier::Standard)
        .unwrap();

    // No approval: refused, and the refusal files the send card.
    let refused = outbox.send_public("demo", &cards, &quote.id).await;
    let card = match refused {
        Err(SendError::NeedsApproval {
            tier: Tier::Standard,
            card,
            ..
        }) => card,
        other => panic!("must refuse tier-above, got {other:?}"),
    };
    assert!(sender.sent().is_empty());
    let open = cards.list_open("demo").await.unwrap();
    assert!(open.iter().any(|open| open.id == card.id));

    // The wrong card does not send.
    let wrong = outbox
        .send_approved(&cards, &quote.id, &quote.card_id)
        .await;
    assert!(matches!(wrong, Err(SendError::BadApproval(_))), "{wrong:?}");

    // Approve the send card: sends exactly once.
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    let sent = outbox
        .send_approved(&cards, &quote.id, &card.id)
        .await
        .unwrap();
    assert_eq!(sent.to, "client@example.com");
    assert!(sent.subject.contains("Quote"));
    assert_eq!(sender.sent().len(), 1);

    let again = outbox.send_approved(&cards, &quote.id, &card.id).await;
    assert!(matches!(again, Err(SendError::AlreadySent(_))), "{again:?}");
    assert_eq!(sender.sent().len(), 1);
}

#[tokio::test]
async fn public_sends_directly() {
    let dir = tempfile::tempdir().unwrap();
    let (outbox, cards, sender) = setup(&dir).await;
    let filed = outbox.intake("demo", &cards, &mailbox(&dir)).await.unwrap();
    let public = filed
        .iter()
        .find(|draft| draft.tier == Tier::Public)
        .unwrap();
    outbox
        .send_public("demo", &cards, &public.id)
        .await
        .unwrap();
    assert_eq!(sender.sent().len(), 1);
    assert_eq!(outbox.pending().await.unwrap().len(), 2);
    // Its draft card stays open: sending is not deciding.
    let draft_card = cards.get(&public.card_id).await.unwrap().unwrap();
    assert_eq!(draft_card.state, CardState::Open);
}
