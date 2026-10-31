//! Phase 4 gate: zero cross-site chunks under a two-site fuzz run, grounded answers
//! with resolving citations, typed signals per event, answers within budget.

use agentrix_site_concierge::{Chunk, Corpus};
use agentrix_site_concierge::{Signal, Visitor, VisitorEvent, answer, emit, retrieve};

async fn corpus() -> Corpus {
    let dir = tempfile::tempdir().unwrap();
    let db = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("corpus.db").display()
    );
    // The TempDir must outlive the test; leak it deliberately — the OS reclaims /tmp.
    std::mem::forget(dir);
    let corpus = Corpus::open(&db).await.unwrap();
    // Two sites with deliberately confusable texts: same words, different facts.
    for (site, city, fee) in [
        ("harbor", "Mombasa", "15,000"),
        ("northwind", "Nairobi", "22,000"),
    ] {
        for (n, text) in [
            format!("The practice in {city} charges monthly bookkeeping from KES {fee}."),
            format!("Payroll in {city} covers up to 20 staff with filings included."),
            format!("Visit the {city} office Monday to Friday for a consultation."),
        ]
        .into_iter()
        .enumerate()
        {
            corpus
                .put(&Chunk {
                    site: site.into(),
                    doc: format!("page-{n}"),
                    chunk_id: format!("chunk-{n}"),
                    source_uri: format!("https://{site}.example/page-{n}"),
                    text,
                })
                .await
                .unwrap();
        }
    }
    corpus
}

/// Deterministic fuzz: token triples over the shared vocabulary.
fn queries() -> Vec<String> {
    let vocab = [
        "practice",
        "mombasa",
        "nairobi",
        "bookkeeping",
        "payroll",
        "filings",
        "consultation",
        "office",
        "staff",
        "charges",
        "monday",
        "visit",
        "monthly",
    ];
    let mut state: u64 = 0x1234_5678;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    (0..300)
        .map(|_| {
            (0..3)
                .map(|_| vocab[next() % vocab.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[tokio::test]
async fn no_cross_site_chunks_under_fuzz() {
    let corpus = corpus().await;
    let mut hits = 0;
    for query in queries() {
        for site in ["harbor", "northwind"] {
            for hit in retrieve(&corpus, site, &query, 3).await.unwrap() {
                hits += 1;
                assert_eq!(hit.chunk.site, site, "query {query:?} leaked a chunk");
            }
        }
    }
    assert!(hits > 100, "retrieval must actually hit, got {hits}");
}

#[tokio::test]
async fn answers_are_grounded_and_cited() {
    let corpus = corpus().await;
    let visitor = Visitor::mint("harbor");
    let started = std::time::Instant::now();
    let found = answer(&corpus, "harbor", &visitor, "bookkeeping charges practice")
        .await
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "answer within budget"
    );
    assert!(found.grounded);
    assert!(!found.citations.is_empty());
    for citation in &found.citations {
        assert!(found.text.contains(&citation.quote), "quote cited verbatim");
        assert!(
            citation.source_uri.contains("harbor.example"),
            "harbor source only"
        );
    }
    let missed = answer(
        &corpus,
        "harbor",
        &visitor,
        "quantum entanglement Hamiltonian",
    )
    .await
    .unwrap();
    assert!(!missed.grounded);
    assert!(missed.citations.is_empty());
}

#[test]
fn visitors_are_random_and_siteless() {
    let a = Visitor::mint("harbor");
    let b = Visitor::mint("harbor");
    assert_ne!(a.id, b.id);
    assert!(a.id.starts_with("visitor-"));
    assert_eq!(a.site, "harbor");
}

#[test]
fn events_emit_the_right_signals() {
    let answered = VisitorEvent::Message {
        text: "hours?".into(),
        grounded: true,
        miss_streak: 0,
    };
    assert!(emit(&answered).is_empty());

    let miss = VisitorEvent::Message {
        text: "refunds?".into(),
        grounded: false,
        miss_streak: 1,
    };
    let signals = emit(&miss);
    assert_eq!(signals.len(), 1);
    assert!(matches!(signals[0], Signal::ContentGap(_)));

    let third = VisitorEvent::Message {
        text: "refunds?".into(),
        grounded: false,
        miss_streak: 3,
    };
    let signals = emit(&third);
    assert_eq!(signals.len(), 2);
    assert!(matches!(signals[0], Signal::ContentGap(_)));
    assert!(matches!(signals[1], Signal::SupportEscalation(_)));

    let review = VisitorEvent::ReviewSeen {
        platform: "maps".into(),
        rating: 2,
        excerpt: "slow".into(),
    };
    assert!(matches!(emit(&review)[..], [Signal::ReviewSeen(_)]));
    let lead = VisitorEvent::ContactShared {
        channel: "email".into(),
        contact_ref: "c-1".into(),
    };
    assert!(matches!(emit(&lead)[..], [Signal::LeadCaptured(_)]));
    let handoff = VisitorEvent::HandoffAsked {
        reason: "human please".into(),
    };
    assert!(matches!(emit(&handoff)[..], [Signal::SupportEscalation(_)]));
}
