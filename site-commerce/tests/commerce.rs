//! Phase 7 gate: 202 proposes and holds; approve then complete charges once with
//! the fee on exactly one site; repeats are idempotent; failures release.

use std::sync::Arc;

use agentrix_site_commerce::{
    Commerce, IntentState, LogProvider, NewPayment, PaymentError, PaymentIntent, StripeProvider,
};
use agentrix_site_growth::Growth;
use agentrix_site_inbox::{Card, CardStore, Decision, Execute};

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

struct FailOnce {
    failed: std::sync::Mutex<bool>,
    log: std::sync::Mutex<Vec<String>>,
}

impl agentrix_site_commerce::PaymentProvider for FailOnce {
    fn charge(&self, intent: &PaymentIntent) -> Result<String, PaymentError> {
        let mut failed = self.failed.lock().unwrap();
        if !*failed {
            *failed = true;
            return Err(PaymentError::Provider("declined".into()));
        }
        self.log.lock().unwrap().push(intent.id.clone());
        Ok("ch-retry".into())
    }
}

async fn setup(
    dir: &tempfile::TempDir,
    provider: Arc<dyn agentrix_site_commerce::PaymentProvider>,
) -> (Commerce, Growth, CardStore) {
    let commerce_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("commerce.db").display()
    );
    let growth_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("growth.db").display()
    );
    let cards_db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("cards.db").display()
    );
    (
        Commerce::open(&commerce_db, provider).await.unwrap(),
        Growth::open(&growth_db).await.unwrap(),
        CardStore::open(&cards_db).await.unwrap(),
    )
}

#[test]
fn minor_units_convert_and_floor() {
    assert_eq!(StripeProvider::minor_units(100_000).unwrap(), "10");
    assert_eq!(StripeProvider::minor_units(10_000).unwrap(), "1");
    assert!(StripeProvider::minor_units(9_999).is_err());
}

#[test]
#[ignore = "needs STRIPE_SECRET_KEY (test mode) and STRIPE_TEST_PM; creates a real test charge"]
fn stripe_live_charge() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let provider = std::sync::Arc::new(StripeProvider::from_env().unwrap());
        let commerce_db = format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("commerce.db").display()
        );
        let growth_db = format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("growth.db").display()
        );
        let cards_db = format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("cards.db").display()
        );
        let commerce = Commerce::open(&commerce_db, provider).await.unwrap();
        let growth = Growth::open(&growth_db).await.unwrap();
        let cards = CardStore::open(&cards_db).await.unwrap();
        growth.set_cap("demo", 1_000_000).await.unwrap();
        let method = std::env::var("STRIPE_TEST_PM").unwrap();
        let payment = NewPayment {
            amount_micro: 50_000,
            currency: "USD".into(),
            purpose: "Agentrix validation".into(),
            payer_ref: "test".into(),
            idempotency_key: "live-1".into(),
            payment_method: Some(method),
        };
        let (intent, card) = commerce.propose("demo", &cards, payment).await.unwrap();
        cards
            .decide(&card.id, Decision::Approve, &AutoApprove)
            .await
            .unwrap();
        let receipt = commerce
            .complete(&growth, &cards, &intent.id, &card.id)
            .await
            .unwrap();
        assert!(
            receipt.charge_id.starts_with("pi_"),
            "{}",
            receipt.charge_id
        );
    });
}

#[tokio::test]
async fn propose_approve_complete_runs_once_and_repeats_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let provider = Arc::new(LogProvider::default());
    let (commerce, growth, cards) = setup(&dir, provider.clone()).await;
    growth.set_cap("demo", 1_000_000).await.unwrap();
    growth.set_cap("other", 1_000_000).await.unwrap();

    let spring = || NewPayment {
        amount_micro: 100_000,
        currency: "USD".into(),
        purpose: "spring order".into(),
        payer_ref: "payer-1".into(),
        idempotency_key: "key-1".into(),
        payment_method: None,
    };
    let (intent, card) = commerce.propose("demo", &cards, spring()).await.unwrap();
    assert_eq!(intent.state, IntentState::Held);
    assert!(provider.charges().is_empty(), "proposing moves no money");

    // Same key twice: one intent, one card.
    let (again, same_card) = commerce.propose("demo", &cards, spring()).await.unwrap();
    assert_eq!(again.id, intent.id);
    assert_eq!(same_card.id, card.id);

    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    let receipt = commerce
        .complete(&growth, &cards, &intent.id, &card.id)
        .await
        .unwrap();
    assert_eq!(receipt.amount_micro, 100_000);
    assert_eq!(receipt.fee_micro, 3_000);
    assert_eq!(provider.charges(), vec![intent.id.clone()]);
    let (_, spent, reserved) = growth.budget("demo").await.unwrap();
    assert_eq!((spent, reserved), (3_000, 0));
    let (_, other_spent, _) = growth.budget("other").await.unwrap();
    assert_eq!(other_spent, 0, "exactly one site is attributed");

    // Completing again charges nothing new.
    let receipt2 = commerce
        .complete(&growth, &cards, &intent.id, &card.id)
        .await
        .unwrap();
    assert_eq!(receipt2, receipt);
    assert_eq!(provider.charges().len(), 1);
    let (_, spent, _) = growth.budget("demo").await.unwrap();
    assert_eq!(spent, 3_000);
}

#[tokio::test]
async fn wrong_card_refuses_and_failure_releases_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let failing: Arc<FailOnce> = Arc::new(FailOnce {
        failed: std::sync::Mutex::new(false),
        log: std::sync::Mutex::new(Vec::new()),
    });
    let (commerce, growth, cards) = setup(&dir, failing.clone()).await;
    growth.set_cap("demo", 1_000_000).await.unwrap();

    let (intent, card) = commerce
        .propose(
            "demo",
            &cards,
            NewPayment {
                amount_micro: 50_000,
                currency: "USD".into(),
                purpose: "beans".into(),
                payer_ref: "payer-2".into(),
                idempotency_key: "key-2".into(),
                payment_method: None,
            },
        )
        .await
        .unwrap();
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();

    let wrong = commerce
        .complete(&growth, &cards, &intent.id, "card-missing")
        .await;
    assert!(
        matches!(wrong, Err(PaymentError::BadApproval(_))),
        "{wrong:?}"
    );

    let failed = commerce
        .complete(&growth, &cards, &intent.id, &card.id)
        .await;
    assert!(
        matches!(failed, Err(PaymentError::Provider(_))),
        "{failed:?}"
    );
    let stuck = commerce.get(&intent.id).await.unwrap().unwrap();
    assert_eq!(stuck.state, IntentState::Failed);
    let (_, spent, reserved) = growth.budget("demo").await.unwrap();
    assert_eq!((spent, reserved), (0, 0), "the failed fee is released");

    let receipt = commerce
        .complete(&growth, &cards, &intent.id, &card.id)
        .await
        .unwrap();
    assert_eq!(receipt.charge_id, "ch-retry");
    let (_, spent, _) = growth.budget("demo").await.unwrap();
    assert_eq!(spent, 1_500);
}

#[test]
fn open_provider_fails_fast_without_network() {
    use agentrix_site_commerce::PaymentProvider as _;
    use agentrix_site_defense::Breaker;
    let provider = StripeProvider::new("test")
        .unwrap()
        .with_api_base("http://127.0.0.1:1")
        .with_breaker(std::sync::Arc::new(Breaker::new(
            "stripe-test",
            1,
            std::time::Duration::from_secs(60),
        )));
    let intent = PaymentIntent {
        id: "pi-test".into(),
        site: "demo".into(),
        amount_micro: 50_000,
        currency: "USD".into(),
        purpose: "test".into(),
        payer_ref: "p".into(),
        idempotency_key: "k".into(),
        payment_method: Some("pm_test".into()),
        state: agentrix_site_commerce::IntentState::Held,
        card_id: "card-test".into(),
        charge_id: None,
    };
    assert!(provider.charge(&intent).is_err());
    let err = provider.charge(&intent).unwrap_err().to_string();
    assert!(err.contains("circuit open"), "{err}");
}

/// Orders hold against stock, complete on approval, cancel by restoring, and
/// never oversell: the reservation is atomic.
#[tokio::test]
async fn orders_hold_complete_and_never_oversell() {
    use agentrix_site_commerce::{NewOrder, OrderError, OrderState};

    let dir = tempfile::tempdir().unwrap();
    let (commerce, _, cards) = setup(&dir, Arc::new(LogProvider::default())).await;
    commerce
        .set_stock("demo", "arabica-250g", 2, 850_000_000, "KES")
        .await
        .unwrap();

    let hold = |key: &str, qty: i64| NewOrder {
        sku: "arabica-250g".into(),
        qty,
        buyer: "agent:stall-7".into(),
        idempotency_key: key.into(),
    };
    let (order, card) = commerce
        .order_hold("demo", &cards, hold("k1", 2))
        .await
        .unwrap();
    assert_eq!(order.state, OrderState::Held);
    assert_eq!(order.total_micro, 1_700_000_000);
    assert_eq!(
        commerce
            .stock_of("demo", "arabica-250g")
            .await
            .unwrap()
            .unwrap()
            .qty,
        0
    );

    // No units left: the second hold refuses without touching the card file.
    let refused = commerce.order_hold("demo", &cards, hold("k2", 1)).await;
    assert!(
        matches!(refused, Err(OrderError::OutOfStock { .. })),
        "{refused:?}"
    );

    // Same key: the first order back, no second card.
    let (again, _) = commerce
        .order_hold("demo", &cards, hold("k1", 2))
        .await
        .unwrap();
    assert_eq!(again.id, order.id);

    // Completion needs the approval.
    let early = commerce.order_complete(&cards, &order.id, &card.id).await;
    assert!(
        matches!(early, Err(OrderError::BadApproval(_))),
        "{early:?}"
    );
    cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .unwrap();
    let done = commerce
        .order_complete(&cards, &order.id, &card.id)
        .await
        .unwrap();
    assert_eq!(done.state, OrderState::Completed);
    let again = commerce
        .order_complete(&cards, &order.id, &card.id)
        .await
        .unwrap();
    assert_eq!(again.state, OrderState::Completed);

    // Cancel restores.
    commerce
        .set_stock("demo", "sukuma-bunch", 5, 50_000_000, "KES")
        .await
        .unwrap();
    let cancel_hold = NewOrder {
        sku: "sukuma-bunch".into(),
        qty: 3,
        buyer: "agent:stall-7".into(),
        idempotency_key: "k3".into(),
    };
    let (order, _) = commerce
        .order_hold("demo", &cards, cancel_hold)
        .await
        .unwrap();
    commerce.order_cancel(&order.id).await.unwrap();
    assert_eq!(
        commerce
            .stock_of("demo", "sukuma-bunch")
            .await
            .unwrap()
            .unwrap()
            .qty,
        5
    );
}
