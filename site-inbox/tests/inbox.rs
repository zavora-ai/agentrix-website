//! Phase 2 gate: file a draft, survive a restart, approve once; failures recover;
//! cards name roles.

use std::sync::Mutex;

use agentrix_site_inbox::{
    Card, CardKind, CardState, CardStore, Decision, Execute, GapSignal, NewCard, StoreError,
    draft_from_gaps, role_of,
};

fn gaps() -> Vec<GapSignal> {
    vec![
        GapSignal {
            id: "gap-1".into(),
            topic: "Weekend opening hours".into(),
            question_count: 4,
            samples: vec!["Are you open Saturday?".into()],
        },
        GapSignal {
            id: "gap-2".into(),
            topic: "Card payments".into(),
            question_count: 2,
            samples: vec!["Do you take cards?".into()],
        },
    ]
}

#[derive(Default)]
struct Applied {
    log: Mutex<Vec<String>>,
    fail_first: Mutex<bool>,
}

impl Execute for Applied {
    fn execute(&self, card: &Card) -> Result<(), String> {
        let mut fail = self.fail_first.lock().unwrap();
        if *fail {
            *fail = false;
            return Err("boom".into());
        }
        self.log.lock().unwrap().push(card.idempotency_key.clone());
        Ok(())
    }
}

async fn url(dir: &tempfile::TempDir, name: &str) -> String {
    let db = dir.path().join(name);
    format!("sqlite://{}?mode=rwc", db.display())
}

#[tokio::test]
async fn draft_round_trip_approves_exactly_once_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = url(&dir, "cards.db").await;
    let filed = {
        let store = CardStore::open(&db).await.unwrap();
        let (_, card) = draft_from_gaps(&store, "demo", "contact", &gaps())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(card.state, CardState::Open);
        assert_eq!(card.role, "Content Editor");
        assert_eq!(card.kind, CardKind::Draft);
        card.id
    };
    // The restart: a new handle on the same file.
    let store = CardStore::open(&db).await.unwrap();
    let open = store.list_open("demo").await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, filed);

    let applied = Applied::default();
    let decided = store
        .decide(&filed, Decision::Approve, &applied)
        .await
        .unwrap();
    assert_eq!(decided.state, CardState::Approved);
    assert!(decided.executed);
    assert_eq!(applied.log.lock().unwrap().len(), 1);

    // A second decision is refused and runs nothing.
    let again = store.decide(&filed, Decision::Approve, &applied).await;
    assert!(
        matches!(again, Err(StoreError::AlreadyDecided(_))),
        "{again:?}"
    );
    assert_eq!(applied.log.lock().unwrap().len(), 1);
    assert!(store.list_open("demo").await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_execution_recovers_and_runs_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = url(&dir, "cards.db").await;
    let store = CardStore::open(&db).await.unwrap();
    let card = store
        .file(NewCard::new(
            "demo",
            CardKind::Approval,
            "Steward",
            "test",
            serde_json::json!({}),
        ))
        .await
        .unwrap();

    let applied = Applied {
        fail_first: Mutex::new(true),
        ..Applied::default()
    };
    let failed = store.decide(&card.id, Decision::Approve, &applied).await;
    assert!(
        matches!(failed, Err(StoreError::Executor { .. })),
        "{failed:?}"
    );
    let stuck = store.get(&card.id).await.unwrap().unwrap();
    assert_eq!(stuck.state, CardState::Approved);
    assert!(!stuck.executed);

    let recovered = store.recover(&applied).await.unwrap();
    assert_eq!(recovered, vec![card.id.clone()]);
    assert_eq!(applied.log.lock().unwrap().len(), 1);
    assert!(store.recover(&applied).await.unwrap().is_empty());
}

#[tokio::test]
async fn rejection_runs_no_effect() {
    let dir = tempfile::tempdir().unwrap();
    let db = url(&dir, "cards.db").await;
    let store = CardStore::open(&db).await.unwrap();
    let card = store
        .file(NewCard::new(
            "demo",
            CardKind::Draft,
            "Content Editor",
            "test",
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    let applied = Applied::default();
    let decided = store
        .decide(&card.id, Decision::Reject, &applied)
        .await
        .unwrap();
    assert_eq!(decided.state, CardState::Rejected);
    assert!(!decided.executed);
    assert!(applied.log.lock().unwrap().is_empty());
}

#[test]
fn cards_name_roles_not_jobs() {
    assert_eq!(role_of("freshness"), Some("Content Editor"));
    assert_eq!(role_of("weekly-review"), Some("Analyst"));
    assert_eq!(role_of("nurture"), Some("Sales Rep"));
}
