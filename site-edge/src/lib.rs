//! Agentrix Website Phase 1: the read-only dual site.
//!
//! One [`Release`] bundle — pages, provenance and a content digest — is served three
//! ways from the same bytes: human HTML pages, structured agent twins, and a public
//! JSON CLI. [`digest_match`] proves the twins carry the same facts as the human
//! pages by extracting the facts back out of the rendered HTML and comparing them
//! field by field, so the check reads what visitors read, not what the generator
//! wrote.
//!
//! Phase 3 serves through [`Current`]: handlers load the served release per
//! request, so publish and rollback swap what both audiences see without a
//! restart.

use awp_types::ContentKind;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::{Router, middleware};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::net::SocketAddr;

/// A single checkable fact on a page, the unit the digest-match compares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    /// Stable within the page, such as `hours-mon-fri`.
    pub id: String,
    /// The fact's text, such as `Open Monday to Friday, 9:00 to 17:00.`
    pub text: String,
}

/// A page of the site: its human content and the facts it asserts. The kind
/// says what the page is — a standing page, a product, a post, an article —
/// so shops, blogs and news sites share one release shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// URL slug, such as `services`.
    pub slug: String,
    /// What the page is.
    pub kind: ContentKind,
    /// Human title.
    pub title: String,
    /// Introductory paragraph.
    pub intro: String,
    /// The facts the page asserts.
    pub facts: Vec<Fact>,
}

/// Where a release came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Generator name and version.
    pub generator: String,
    /// Template kind, such as `business`.
    pub template: String,
    /// Evidence sources the pages were built from.
    pub sources: Vec<String>,
    /// When the release was built (excluded from the digest).
    pub created: String,
}

/// An immutable, content-addressed bundle: the pages, their provenance, and the
/// digest over both. The edge serves only what a release contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Hex SHA-256 over the canonical pages, template and sources.
    pub digest: String,
    /// The pages.
    pub pages: Vec<Page>,
    /// The provenance.
    pub provenance: Provenance,
}

/// The machine-readable projection of one page, served to AI agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Twin {
    /// Page slug.
    pub slug: String,
    /// What the page is.
    pub kind: ContentKind,
    /// Page title.
    pub title: String,
    /// The page's facts, verbatim.
    pub facts: Vec<Fact>,
    /// The release digest the twin derives from.
    pub release: String,
}

/// The served release pointer. Handlers load it per request; [`set`](Self::set)
/// swaps what both audiences see. The pointer always names an admitted release:
/// only the LaunchGate path moves it.
#[derive(Debug)]
pub struct Current(Mutex<Arc<Release>>);

impl Current {
    /// A pointer serving `release`.
    pub fn new(release: Release) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Arc::new(release))))
    }

    /// The release now served.
    pub fn get(&self) -> Arc<Release> {
        self.0.lock().expect("current release").clone()
    }

    /// Serves `release` from the next request on.
    pub fn set(&self, release: Release) {
        *self.0.lock().expect("current release") = Arc::new(release);
    }
}

/// The fixture business site Phase 1 serves: home, services and contact.
pub fn business_site() -> Vec<Page> {
    vec![
        Page {
            kind: ContentKind::Page,
            slug: "home".into(),
            title: "Harbor & Plum — Accounting for small firms".into(),
            intro: "A two-partner practice in Mombasa serving retailers and cafés.".into(),
            facts: vec![
                Fact {
                    id: "founded".into(),
                    text: "Founded in 2019.".into(),
                },
                Fact {
                    id: "staff".into(),
                    text: "Two partners and six accountants.".into(),
                },
                Fact {
                    id: "clients".into(),
                    text: "Serves 140 active client firms.".into(),
                },
            ],
        },
        Page {
            kind: ContentKind::Service,
            slug: "services".into(),
            title: "Services and fees".into(),
            intro: "Monthly bookkeeping, payroll and annual filings on fixed fees.".into(),
            facts: vec![
                Fact {
                    id: "bookkeeping".into(),
                    text: "Monthly bookkeeping from KES 15,000.".into(),
                },
                Fact {
                    id: "payroll".into(),
                    text: "Payroll for up to 20 staff from KES 8,000.".into(),
                },
                Fact {
                    id: "filings".into(),
                    text: "Annual filings from KES 25,000.".into(),
                },
            ],
        },
        Page {
            kind: ContentKind::Page,
            slug: "contact".into(),
            title: "Contact and hours".into(),
            intro: "Visit the office or write any time; replies within one business day.".into(),
            facts: vec![
                Fact {
                    id: "address".into(),
                    text: "Moi Avenue 402, Mombasa.".into(),
                },
                Fact {
                    id: "hours".into(),
                    text: "Open Monday to Friday, 9:00 to 17:00 EAT.".into(),
                },
                Fact {
                    id: "email".into(),
                    text: "Reach the practice at hello@example.com.".into(),
                },
            ],
        },
    ]
}

/// The fixture shop Phase 1 serves: a store home plus three products. Prices
/// ride as facts; stock is volatile and lives in the commerce store, never in
/// the release.
pub fn shop_site() -> Vec<Page> {
    vec![
        Page {
            kind: ContentKind::Page,
            slug: "home".into(),
            title: "Kikapu — fresh groceries in Nairobi".into(),
            intro: "Order by noon for same-day delivery across Nairobi.".into(),
            facts: vec![
                Fact {
                    id: "delivery".into(),
                    text: "Same-day delivery for orders before noon.".into(),
                },
                Fact {
                    id: "coverage".into(),
                    text: "Delivers across Nairobi county.".into(),
                },
            ],
        },
        Page {
            kind: ContentKind::Product,
            slug: "sukuma-bunch".into(),
            title: "Sukuma wiki bunch".into(),
            intro: "Cut this morning on a Kiambu farm.".into(),
            facts: vec![
                Fact {
                    id: "price".into(),
                    text: "Price: KES 50.".into(),
                },
                Fact {
                    id: "unit".into(),
                    text: "Sold per 500g bunch.".into(),
                },
            ],
        },
        Page {
            kind: ContentKind::Product,
            slug: "arabica-250g".into(),
            title: "AA arabica 250g".into(),
            intro: "Single-origin Nyeri, roasted weekly.".into(),
            facts: vec![
                Fact {
                    id: "price".into(),
                    text: "Price: KES 850.".into(),
                },
                Fact {
                    id: "roast".into(),
                    text: "Medium roast, whole bean.".into(),
                },
            ],
        },
        Page {
            kind: ContentKind::Product,
            slug: "mango-crate".into(),
            title: "Mango crate 5kg".into(),
            intro: "Makueni Kent mangoes in season.".into(),
            facts: vec![
                Fact {
                    id: "price".into(),
                    text: "Price: KES 600.".into(),
                },
                Fact {
                    id: "origin".into(),
                    text: "Grown in Makueni county.".into(),
                },
            ],
        },
    ]
}

/// The fixture blog: a home plus three posts.
pub fn blog_site() -> Vec<Page> {
    vec![
        Page {
            kind: ContentKind::Page,
            slug: "home".into(),
            title: "The Long Rains — notes on building in Nairobi".into(),
            intro: "Essays on startups, craft and the city.".into(),
            facts: vec![Fact {
                id: "cadence".into(),
                text: "New essays every Sunday.".into(),
            }],
        },
        Page {
            kind: ContentKind::Post,
            slug: "ship-small".into(),
            title: "Ship small, ship weekly".into(),
            intro: "Why the smallest releasable thing wins.".into(),
            facts: vec![Fact {
                id: "thesis".into(),
                text: "Weekly releases beat quarterly launches.".into(),
            }],
        },
        Page {
            kind: ContentKind::Post,
            slug: "talk-to-dukas".into(),
            title: "Talk to dukas, not dashboards".into(),
            intro: "What three kiosk owners taught us about pricing.".into(),
            facts: vec![Fact {
                id: "thesis".into(),
                text: "Counter conversations set prices better than analytics.".into(),
            }],
        },
        Page {
            kind: ContentKind::Post,
            slug: "repair-culture".into(),
            title: "The repair culture dividend".into(),
            intro: "Fixing beats replacing, in code and in kilns.".into(),
            facts: vec![Fact {
                id: "thesis".into(),
                text: "Repair budgets return twice their cost.".into(),
            }],
        },
    ]
}

/// The fixture news site: a home plus three articles.
pub fn news_site() -> Vec<Page> {
    vec![
        Page {
            kind: ContentKind::Page,
            slug: "home".into(),
            title: "Pwani Wire — Mombasa daily".into(),
            intro: "The coast's morning brief.".into(),
            facts: vec![Fact {
                id: "cadence".into(),
                text: "Published every morning at 6 EAT.".into(),
            }],
        },
        Page {
            kind: ContentKind::Article,
            slug: "ferry-timetable".into(),
            title: "Likoni ferry adds dawn crossings".into(),
            intro: "Two extra sailings from Monday.".into(),
            facts: vec![Fact {
                id: "crossings".into(),
                text: "Dawn crossings at 4:30 and 5:00 from Monday.".into(),
            }],
        },
        Page {
            kind: ContentKind::Article,
            slug: "fish-prices".into(),
            title: "Fish prices ease at Kongowea".into(),
            intro: "Good catches push tilapia down a fifth.".into(),
            facts: vec![Fact {
                id: "prices".into(),
                text: "Tilapia down twenty percent this week.".into(),
            }],
        },
        Page {
            kind: ContentKind::Article,
            slug: "street-lights".into(),
            title: "Old Town street lights restored".into(),
            intro: "The county crew finished Ndia Kuu.".into(),
            facts: vec![Fact {
                id: "lights".into(),
                text: "Ndia Kuu street lights work again.".into(),
            }],
        },
    ]
}

/// Builds the release over `pages`: the digest covers the canonical pages, template
/// and sources, so rebuilding the same inputs yields the same digest.
pub fn build_release(pages: Vec<Page>, template: &str, sources: Vec<String>) -> Release {
    let canonical = serde_json::json!({
        "template": template,
        "sources": sources,
        "pages": pages,
    });
    let digest = hex(Sha256::digest(canonical.to_string().as_bytes()));
    Release {
        digest,
        pages,
        provenance: Provenance {
            generator: "agentrix-site-edge 0.1.0".into(),
            template: template.into(),
            sources,
            created: chrono::Utc::now().to_rfc3339(),
        },
    }
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Renders the human HTML for `page`. Every fact lands in exactly one
/// `<li data-fact-id="...">`, which is the hook [`digest_match`] extracts.
pub fn render_page(page: &Page, digest: &str) -> String {
    let mut facts = String::new();
    for fact in &page.facts {
        facts.push_str(&format!(
            "      <li data-fact-id=\"{}\">{}</li>\n",
            escape(&fact.id),
            escape(&fact.text)
        ));
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\">\n    <title>{}</title>\n  </head>\n  <body>\n    <main>\n      <h1>{}</h1>\n      <p>{}</p>\n      <ul>\n{}      </ul>\n      <footer>Release <code>{}</code></footer>\n    </main>\n  </body>\n</html>\n",
        escape(&page.title),
        escape(&page.title),
        escape(&page.intro),
        facts,
        digest,
    )
}

/// The agent twin of `page`: the same facts, structured, with the release digest.
pub fn twin_of(page: &Page, digest: &str) -> Twin {
    Twin {
        slug: page.slug.clone(),
        kind: page.kind,
        title: page.title.clone(),
        facts: page.facts.clone(),
        release: digest.to_string(),
    }
}

/// Extracts `(id, text)` facts from rendered HTML by scanning the
/// `<li data-fact-id="...">...</li>` hooks.
pub fn extract_facts(html: &str) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<li data-fact-id=\"") {
        rest = &rest[start + "<li data-fact-id=\"".len()..];
        let Some(end) = rest.find("\">") else { break };
        let id = rest[..end].to_string();
        rest = &rest[end + "\">".len()..];
        let Some(close) = rest.find("</li>") else {
            break;
        };
        facts.push((unescape(&id), unescape(rest[..close].trim())));
        rest = &rest[close + "</li>".len()..];
    }
    facts
}

/// The Phase 1 gate: every twin carries the same facts as its human page. Rebuilds
/// the release digest, renders each page, extracts its facts from the HTML, and
/// compares them field by field with the twin. `Ok` on a full match, or the list
/// of mismatches.
pub fn digest_match(release: &Release) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let rebuilt = build_release(
        release.pages.clone(),
        &release.provenance.template,
        release.provenance.sources.clone(),
    );
    if rebuilt.digest != release.digest {
        problems.push(format!(
            "release digest {} does not match rebuilt {}",
            release.digest, rebuilt.digest
        ));
    }
    for page in &release.pages {
        let html = render_page(page, &release.digest);
        let seen: HashMap<_, _> = extract_facts(&html).into_iter().collect();
        let twin = twin_of(page, &release.digest);
        for fact in &twin.facts {
            match seen.get(&fact.id) {
                Some(text) if *text == fact.text => {}
                Some(text) => problems.push(format!(
                    "page {} fact {} differs: page says {text:?}, twin says {:?}",
                    page.slug, fact.id, fact.text
                )),
                None => problems.push(format!(
                    "page {} fact {} is missing from the human page",
                    page.slug, fact.id
                )),
            }
        }
        for id in seen.keys() {
            if !twin.facts.iter().any(|f| &f.id == id) {
                problems.push(format!(
                    "page {} fact {id} is on the human page but not in the twin",
                    page.slug
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// A public CLI request: one command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliRequest {
    /// The command, such as `get services` or `twin contact`.
    pub command: String,
}

/// A public CLI response: human-readable text plus structured data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliResponse {
    /// Whether the command ran.
    pub ok: bool,
    /// Human-readable output or error.
    pub text: String,
    /// Structured payload, such as a twin.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

/// Runs one CLI command against `release`. The read-only allowlist is the anonymous
/// capability policy: `help`, `list`, `get <slug>` and `twin <slug>` run; anything
/// else — including anything that reads as a write — is refused, never attempted.
pub fn run_command(release: &Release, input: &str) -> CliResponse {
    let words: Vec<&str> = input.split_whitespace().collect();
    let fail = |text: String| CliResponse {
        ok: false,
        text,
        data: serde_json::Value::Null,
    };
    let page = |slug: &str| release.pages.iter().find(|p| p.slug == slug);
    match words.as_slice() {
        [] => fail("Empty command. Try `help`.".into()),
        ["help"] => CliResponse {
            ok: true,
            text: "Commands: help, list, get <slug>, twin <slug>. This CLI is read-only.".into(),
            data: serde_json::json!({ "commands": ["help", "list", "get", "twin"] }),
        },
        ["list"] => {
            let slugs: Vec<&str> = release.pages.iter().map(|p| p.slug.as_str()).collect();
            CliResponse {
                ok: true,
                text: format!("Pages: {}", slugs.join(", ")),
                data: serde_json::json!({ "pages": slugs, "release": release.digest }),
            }
        }
        ["get", slug] => match page(slug) {
            Some(page) => CliResponse {
                ok: true,
                text: format!("{} — {}", page.title, page.intro),
                data: serde_json::json!({
                    "slug": page.slug,
                    "title": page.title,
                    "intro": page.intro,
                    "facts": page.facts.iter().map(|f| &f.text).collect::<Vec<_>>(),
                }),
            },
            None => fail(format!("Unknown page `{slug}`. Try `list`.")),
        },
        ["twin", slug] => match page(slug) {
            Some(page) => CliResponse {
                ok: true,
                text: format!("Twin of `{slug}` at release {}.", short(&release.digest)),
                data: serde_json::to_value(twin_of(page, &release.digest))
                    .unwrap_or(serde_json::Value::Null),
            },
            None => fail(format!("Unknown page `{slug}`. Try `list`.")),
        },
        [verb, ..] => fail(format!(
            "Refused: `{verb}` is not a read command. This CLI is read-only; try `help`."
        )),
    }
}

fn short(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}

/// Per-IP rate limit: at most `max_per_minute` requests in a rolling minute.
#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    /// Requests per rolling minute per IP.
    pub max_per_minute: u32,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            max_per_minute: 120,
        }
    }
}

#[derive(Debug, Clone)]
struct EdgeState {
    current: Arc<Current>,
    limit: RateLimit,
    hits: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
}

async fn rate_limit(
    state: EdgeState,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let now = Instant::now();
    let window = Duration::from_secs(60);
    let admitted = {
        let mut hits = state.hits.lock().expect("hit table");
        let queue = hits.entry(ip).or_default();
        while queue
            .front()
            .is_some_and(|at| now.duration_since(*at) > window)
        {
            queue.pop_front();
        }
        if queue.len() as u32 >= state.limit.max_per_minute {
            false
        } else {
            queue.push_back(now);
            true
        }
    };
    if !admitted {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("60"));
        return (
            StatusCode::TOO_MANY_REQUESTS,
            headers,
            "Rate limit exceeded.",
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&state.current.get().digest) {
        response.headers_mut().insert("x-release-digest", value);
    }
    response
}

async fn index(State(state): State<EdgeState>) -> Html<String> {
    let release = state.current.get();
    let mut items = String::new();
    for page in &release.pages {
        items.push_str(&format!(
            "      <li><a href=\"/p/{}\">{}</a> (<a href=\"/twin/{}\">twin</a>)</li>\n",
            escape(&page.slug),
            escape(&page.title),
            escape(&page.slug),
        ));
    }
    Html(format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head><meta charset=\"utf-8\"><title>{}</title></head>\n  <body><main><h1>{}</h1><ul>\n{}</ul><footer>Release <code>{}</code></footer></main></body>\n</html>\n",
        escape(&release.pages[0].title),
        escape(&release.pages[0].title),
        items,
        release.digest,
    ))
}

async fn human_page(State(state): State<EdgeState>, Path(slug): Path<String>) -> impl IntoResponse {
    let release = state.current.get();
    match release.pages.iter().find(|p| p.slug == slug) {
        Some(page) => Html(render_page(page, &release.digest)).into_response(),
        None => (StatusCode::NOT_FOUND, "Unknown page.").into_response(),
    }
}

async fn twin(State(state): State<EdgeState>, Path(slug): Path<String>) -> impl IntoResponse {
    let release = state.current.get();
    match release.pages.iter().find(|p| p.slug == slug) {
        Some(page) => Json(twin_of(page, &release.digest)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "unknown page" })),
        )
            .into_response(),
    }
}

async fn cli(State(state): State<EdgeState>, Json(body): Json<CliRequest>) -> impl IntoResponse {
    let release = state.current.get();
    let answer = run_command(&release, &body.command);
    let status = if answer.ok {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(answer)).into_response()
}

fn routes(current: Arc<Current>, limit: RateLimit) -> Router {
    let state = EdgeState {
        current,
        limit,
        hits: Arc::new(Mutex::new(HashMap::new())),
    };
    let guard = state.clone();
    Router::new()
        .route("/", get(index))
        .route("/p/{slug}", get(human_page))
        .route("/twin/{slug}", get(twin))
        .route("/cli", post(cli))
        .layer(middleware::from_fn(move |request, next| {
            let guard = guard.clone();
            async move { rate_limit(guard, request, next).await }
        }))
        .with_state(state)
}

/// The read-only edge router over `release`: human pages at `/p/:slug`, twins at
/// `/twin/:slug`, the CLI at `POST /cli`, all under the per-IP `limit`.
pub fn router(release: Release, limit: RateLimit) -> Router {
    routes(Current::new(release), limit)
}

/// The edge router over the shared [`Current`] pointer: publish and rollback move
/// the pointer and both audiences follow from the next request on.
pub fn router_on(current: &Arc<Current>, limit: RateLimit) -> Router {
    routes(current.clone(), limit)
}
