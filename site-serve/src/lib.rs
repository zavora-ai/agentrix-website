//! Agentrix Website composition: one server over every phase.
//!
//! [`serve`] opens the product stores and the kernel stores in one data dir, seeds
//! the demo site through the LaunchGate when nothing is served yet, hosts the
//! agent teams on the kernel ([`agents`]), registers the guarded workflows
//! ([`workflows`]), attaches the signal intake ([`signals`]), starts the kernel,
//! and serves the edge from the release the
//! store calls current. The agents run on Muse through Baseten; `--dry-run`
//! scripts the model for offline runs.

pub mod agents;
pub mod signals;
pub mod tracker;
pub mod workflows;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentrix_site_concierge::Corpus;
use agentrix_site_commerce::{Commerce, LogProvider};
use agentrix_site_defense::{Freeze, QuietHours};
use agentrix_site_edge::{
    Current, RateLimit, blog_site, build_release, business_site, news_site, router_on, shop_site,
};
use agentrix_site_growth::Growth;
use agentrix_site_inbox::{Card, CardKind, CardStore, Decision, Execute, NewCard};
use agentrix_site_release::{LaunchGate, ReleaseStore};
use awp_types::{Bounded, Signal, SignalKind};

/// What serving is for: the site, its data, its address, its model.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// Data directory holding every database.
    pub data: PathBuf,
    /// Kernel application name.
    pub app: String,
    /// Site name.
    pub site: String,
    /// Template seeded on first boot: business, shop, blog or news.
    pub template: String,
    /// Edge bind address.
    pub addr: String,
    /// Script the model instead of calling Baseten.
    pub dry_run: bool,
    /// Baseten credentials; required unless `dry_run`.
    pub baseten_key: Option<String>,
    /// Baseten model id.
    pub baseten_model: String,
    /// Baseten-compatible base URL.
    pub baseten_base_url: String,
    /// Max output tokens per agent call (Baseten Muse Spark Open allows 1,048,576).
    pub max_tokens: u32,
}

impl ServeConfig {
    /// A config over `data`, serving `addr`, from the environment.
    pub fn from_env(data: PathBuf, addr: String, dry_run: bool) -> Self {
        Self {
            data,
            app: "demo".into(),
            site: "demo".into(),
            template: std::env::var("TEMPLATE").unwrap_or_else(|_| "business".into()),
            addr,
            dry_run,
            baseten_key: std::env::var("BASETEN_API_KEY").ok(),
            baseten_model: std::env::var("BASETEN_MODEL")
                .unwrap_or_else(|_| "meta-models/muse-spark-open-1026".into()),
            baseten_base_url: std::env::var("BASETEN_BASE_URL")
                .unwrap_or_else(|_| "https://inference.baseten.co/v1".into()),
            max_tokens: std::env::var("BASETEN_MAX_TOKENS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(16_384),
        }
    }
}

/// The product stores, one database each.
pub struct ProductStores {
    /// Decision cards.
    pub cards: Arc<CardStore>,
    /// Admitted releases and the current pointer.
    pub releases: Arc<ReleaseStore>,
    /// Concierge chunks.
    pub corpus: Arc<Corpus>,
    /// Budgets, KPIs, ladder.
    pub growth: Arc<Growth>,
    /// Freeze switch and defense audit.
    pub freeze: Arc<Freeze>,
    /// Stock and held orders.
    pub commerce: Arc<Commerce>,
}

impl ProductStores {
    /// Opens every product store under `data`.
    pub async fn open(data: &Path) -> anyhow::Result<Self> {
        let url = |name: &str| format!("sqlite://{}?mode=rwc", data.join(name).display());
        Ok(Self {
            cards: Arc::new(
                CardStore::open(&url("cards.db"))
                    .await
                    .map_err(anyhow_err)?,
            ),
            releases: Arc::new(
                ReleaseStore::open(&url("releases.db"))
                    .await
                    .map_err(anyhow_err)?,
            ),
            corpus: Arc::new(Corpus::open(&url("corpus.db")).await.map_err(anyhow_err)?),
            growth: Arc::new(Growth::open(&url("growth.db")).await.map_err(anyhow_err)?),
            freeze: Arc::new(Freeze::open(&url("defense.db")).await.map_err(anyhow_err)?),
            commerce: Arc::new(
                Commerce::open(&url("commerce.db"), Arc::new(LogProvider::default()))
                    .await
                    .map_err(anyhow_err)?,
            ),
        })
    }
}

fn anyhow_err(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!("{error}")
}

struct AutoApprove;

impl Execute for AutoApprove {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

/// Seeds the demo site when nothing is served: the business release through the
/// LaunchGate behind an approved card, its facts as corpus chunks, and a starter
/// budget. Returns the served digest.
pub async fn seed_demo(
    stores: &ProductStores,
    site: &str,
    template: &str,
) -> anyhow::Result<String> {
    if let Some(current) = stores.releases.current(site).await.map_err(anyhow_err)? {
        return Ok(current.digest);
    }
    let (pages, name) = match template {
        "shop" => (shop_site(), "shop"),
        "blog" => (blog_site(), "blog"),
        "news" => (news_site(), "news"),
        _ => (business_site(), "business"),
    };
    let release = build_release(pages, name, vec!["seed".into()]);
    if template == "shop" {
        for (sku, qty, unit_micro) in [
            ("sukuma-bunch", 10, 50_000_000),
            ("arabica-250g", 10, 850_000_000),
            ("mango-crate", 10, 600_000_000),
        ] {
            stores
                .commerce
                .set_stock(site, sku, qty, unit_micro, "KES")
                .await
                .map_err(anyhow_err)?;
        }
    }
    let card = stores
        .cards
        .file(NewCard::new(
            site,
            CardKind::Approval,
            "Chief of Staff",
            "Seed: admit the demo release",
            serde_json::json!({ "release": release.digest }),
        ))
        .await
        .map_err(anyhow_err)?;
    stores
        .cards
        .decide(&card.id, Decision::Approve, &AutoApprove)
        .await
        .map_err(anyhow_err)?;
    let served = LaunchGate::new(site)
        .admit(&stores.releases, &stores.cards, &release, &card.id)
        .await
        .map_err(anyhow_err)?;
    for page in &served.pages {
        for fact in &page.facts {
            stores
                .corpus
                .put(&agentrix_site_concierge::Chunk {
                    site: site.to_string(),
                    doc: page.slug.clone(),
                    chunk_id: fact.id.clone(),
                    source_uri: format!("release:{}#{}", served.digest, page.slug),
                    text: fact.text.clone(),
                })
                .await
                .map_err(anyhow_err)?;
        }
    }
    stores
        .growth
        .set_cap(site, 1_000_000)
        .await
        .map_err(anyhow_err)?;
    Ok(served.digest)
}

/// Watches the kernel bus and files a Security Guard incident card for every
/// run that fails: the supervisor parks and retries, the owner hears about
/// the ones that stay down. Runs until aborted.
pub fn incident_watch(
    mut subscriber: adk_kernel::EventSubscriber,
    cards: Arc<CardStore>,
    site: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(record) = subscriber.recv().await {
            let adk_kernel::KernelEvent::Process {
                pid, to, reason, ..
            } = &record.event
            else {
                continue;
            };
            if !matches!(to, adk_core::RunState::Failed) {
                continue;
            }
            let card = NewCard::new(
                site.as_str(),
                CardKind::Incident,
                "Security Guard",
                format!("Security Guard: run {pid} failed"),
                serde_json::json!({ "pid": pid, "reason": reason }),
            );
            if let Err(error) = cards.file(card).await {
                tracing::warn!(error = %error, "incident card filing failed");
            }
        }
    })
}

/// Serves `config`: stores, seed, agents, started kernel, live edge. Runs until
/// Ctrl-C, then stops the kernel.
pub async fn serve(config: &ServeConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data)?;
    let stores = ProductStores::open(&config.data).await?;
    let digest = seed_demo(&stores, &config.site, &config.template).await?;
    tracing::info!(digest = %digest[..12.min(digest.len())], "demo site seeded");
    let current = Current::new(
        stores
            .releases
            .current(&config.site)
            .await
            .map_err(anyhow_err)?
            .ok_or_else(|| anyhow::anyhow!("no current release after seed"))?,
    );

    let sessions_db = format!(
        "sqlite://{}?mode=rwc",
        config.data.join("sessions.db").display()
    );
    let sessions = Arc::new(
        adk_session::SqliteSessionService::new(&sessions_db)
            .await
            .map_err(anyhow_err)?,
    );
    sessions.migrate().await.map_err(anyhow_err)?;

    let model = agents::build_model(config)?;
    let kernel_stores = workflows::open_kernel_stores(&config.data).await?;
    let runners = agents::build_runners(
        &config.app,
        &config.site,
        &stores,
        sessions,
        model,
        kernel_stores.signals.clone(),
        QuietHours::from_env(),
    )?;
    let workflows = Arc::new(workflows::SiteWorkflows::new(
        config.site.clone(),
        current.clone(),
        stores.releases.clone(),
        stores.cards.clone(),
        stores.growth.clone(),
        stores.freeze.clone(),
    ));

    let kernel = workflows::build_kernel(
        &config.app,
        &config.site,
        &config.data,
        &kernel_stores,
        stores.growth.clone(),
        runners,
        workflows,
    )
    .await?;
    let incidents = incident_watch(
        kernel.bus().subscribe(),
        stores.cards.clone(),
        config.site.clone(),
    );
    let partition: adk_kernel::PartitionId = format!("{}.back-office", config.app).into();
    let mut check = agentrix_site_host::SelfCheck::new();
    check.stores(&workflows::kernel_db_url(&config.data));
    let standing = workflows::standing_triggers(&config.app, workflows::site_id(), &partition);
    check.processes(&standing).map_err(anyhow_err)?;
    check.kernel(standing.len() + 1);
    let sink = adk_kernel::charter::SignalSink::for_kernel(
        &kernel,
        kernel_stores.signals.clone(),
        partition,
    );
    tracker::seed_charter(
        kernel_stores.charters.as_ref(),
        workflows::site_id(),
        tracker::site_kind_for(&config.template),
    )
    .await
    .map_err(anyhow_err)?;
    let probe = Signal {
        id: uuid::Uuid::new_v4(),
        kind: SignalKind::ContentGap {
            topic: Bounded::new("self-check probe", 80).map_err(anyhow_err)?,
            question_count: 1,
            samples: Vec::new(),
        },
        emitted_by: format!("urn:adk:agent:{}/self-check", config.site),
        emitted_at: chrono::Utc::now(),
    };
    let outcome = sink.emit(probe).await.map_err(anyhow_err)?;
    check.signal(&outcome).map_err(anyhow_err)?;
    check
        .durable_db(&config.data.join("kernel.db"))
        .map_err(anyhow_err)?;
    tracing::info!(report = %check, "self-check green");
    kernel.start().await.map_err(anyhow_err)?;
    tracing::info!(app = %config.app, "kernel started");
    let restored = tracker::site_tracker(
        uuid::Uuid::nil(),
        &config.site,
        stores.growth.clone(),
        kernel_stores.charters.clone(),
        kernel_stores.kpis.clone(),
    )
    .restore_pauses()
    .await
    .map_err(anyhow_err)?;
    if !restored.is_empty() {
        tracing::info!(jobs = ?restored, "breach pauses restored");
    }

    let intake = Arc::new(signals::Intake {
        site: config.site.clone(),
        corpus: stores.corpus.clone(),
        sink,
    });

    let app = router_on(&current, RateLimit::default()).merge(signals::intake_routes(intake));
    let listener = tokio::net::TcpListener::bind(&config.addr)
        .await
        .map_err(anyhow_err)?;
    tracing::info!(addr = %listener.local_addr().map_err(anyhow_err)?, "serving the site");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await
    .map_err(anyhow_err)?;
    incidents.abort();
    kernel.stop().await.map_err(anyhow_err)?;
    Ok(())
}
