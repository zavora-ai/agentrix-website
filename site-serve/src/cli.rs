//! The agent CLI: verified tiers over the release, stock, orders and signals.
//!
//! Anonymous callers read: `help`, `list`, `get`, `twin`, `ask`, `stock`.
//! Verified callers — bearer tokens the owner issues — also place held
//! orders, open support threads and capture leads with the consent receipts
//! they assert. Holds always card for approval; nothing a caller does
//! completes unwatched.

use std::sync::Arc;

use agentrix_site_commerce::{Commerce, NewOrder};
use agentrix_site_concierge::{Corpus, retrieve_doc};
use agentrix_site_edge::{CliRequest, CliResponse, Current, run_command};
use agentrix_site_inbox::CardStore;
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use sha2::{Digest, Sha256};

/// Owner-issued bearer tokens: hashed at rest, revocable, one tier.
pub struct TokenStore {
    pool: sqlx::sqlite::SqlitePool,
}

impl TokenStore {
    /// Opens (creating and migrating) the store at `url`.
    pub async fn open(url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tokens (
                site TEXT NOT NULL,
                hash TEXT NOT NULL,
                label TEXT NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (site, hash)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Issues a verified token for `label`; the plaintext shows once.
    pub async fn issue(&self, site: &str, label: &str) -> Result<String, sqlx::Error> {
        let token = format!("agx_{}", uuid::Uuid::new_v4().as_simple());
        sqlx::query("INSERT INTO tokens (site, hash, label, revoked) VALUES (?, ?, ?, 0)")
            .bind(site)
            .bind(token_hash(&token))
            .bind(label)
            .execute(&self.pool)
            .await?;
        Ok(token)
    }

    /// The verified label for `bearer`, or `None` for anonymous, revoked or
    /// unknown callers.
    pub async fn verified_label(&self, site: &str, bearer: Option<&str>) -> Option<String> {
        let bearer = bearer?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT label FROM tokens WHERE site = ? AND hash = ? AND revoked = 0")
                .bind(site)
                .bind(token_hash(bearer))
                .fetch_optional(&self.pool)
                .await
                .ok()?;
        row.map(|(label,)| label)
    }

    /// Revokes `bearer`; `false` when it was unknown.
    pub async fn revoke(&self, site: &str, bearer: &str) -> Result<bool, sqlx::Error> {
        let done = sqlx::query("UPDATE tokens SET revoked = 1 WHERE site = ? AND hash = ?")
            .bind(site)
            .bind(token_hash(bearer))
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

fn token_hash(bearer: &str) -> String {
    format!("{:x}", Sha256::digest(bearer.as_bytes()))
}

/// Everything the CLI commands touch.
#[derive(Clone)]
pub struct CliState {
    /// The site served.
    pub site: String,
    /// The served release for reads.
    pub current: Arc<Current>,
    /// Bearer tokens.
    pub tokens: Arc<TokenStore>,
    /// Stock and held orders.
    pub commerce: Arc<Commerce>,
    /// Approval cards for holds.
    pub cards: Arc<CardStore>,
    /// Chunks for product answers.
    pub corpus: Arc<Corpus>,
    /// The kernel-attached sink for support and lead signals.
    pub sink: adk_kernel::charter::SignalSink,
}

fn bearer_of(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn refused(command: &str) -> CliResponse {
    CliResponse {
        ok: false,
        text: format!("{command} needs a verified token"),
        data: serde_json::Value::Null,
    }
}

/// Runs one command under the caller's tier.
async fn cli(
    State(state): State<Arc<CliState>>,
    headers: HeaderMap,
    Json(body): Json<CliRequest>,
) -> (axum::http::StatusCode, Json<CliResponse>) {
    let release = state.current.get();
    let words: Vec<&str> = body.command.split_whitespace().collect();
    let answer = dispatch(&state, &release, bearer_of(&headers), &words).await;
    let status = if answer.ok {
        axum::http::StatusCode::OK
    } else if answer.text.contains("needs a verified token") {
        axum::http::StatusCode::FORBIDDEN
    } else {
        axum::http::StatusCode::BAD_REQUEST
    };
    (status, Json(answer))
}

async fn dispatch(
    state: &CliState,
    release: &agentrix_site_edge::Release,
    bearer: Option<&str>,
    words: &[&str],
) -> CliResponse {
    let text = words.join(" ");
    let Some((command, args)) = words.split_first() else {
        return run_command(release, "");
    };
    match *command {
        "help" => CliResponse {
            ok: true,
            text: "help | list | get <slug> | twin <slug> | ask <slug> <question> | stock <sku> || verified: order <sku> <qty> | support <message> | lead <channel> <contact> consent <uuid,...> | token status".into(),
        data: serde_json::Value::Null,
        },
        "list" | "get" | "twin" => run_command(release, &text),
        "ask" => ask_product(state, args).await,
        "stock" => stock_of(state, args).await,
        "token" => {
            let label = state.tokens.verified_label(&state.site, bearer).await;
            CliResponse {
                ok: true,
                text: match label {
                    Some(label) => format!("verified as {label}"),
                    None => "anonymous".into(),
                },
            data: serde_json::Value::Null,
            }
        }
        "order" | "support" | "lead" => {
            let Some(label) = state.tokens.verified_label(&state.site, bearer).await else {
                return refused(command);
            };
            match *command {
                "order" => place_order(state, args).await,
                "support" => open_support(state, &label, args).await,
                _ => capture_lead(state, args).await,
            }
        }
        _ => run_command(release, &text),
    }
}

/// Answers from one page's chunks: each product answers for itself.
async fn ask_product(state: &CliState, args: &[&str]) -> CliResponse {
    let Some((slug, question)) = args.split_first() else {
        return CliResponse {
            ok: false,
            text: "usage: ask <slug> <question>".into(),
            data: serde_json::Value::Null,
        };
    };
    let hits = retrieve_doc(&state.corpus, &state.site, slug, &question.join(" "), 2).await;
    match hits {
        Ok(hits) if !hits.is_empty() => CliResponse {
            ok: true,
            text: hits
                .iter()
                .map(|hit| format!("\"{}\" [{}]", hit.chunk.text, hit.chunk.chunk_id))
                .collect::<Vec<_>>()
                .join("\n"),
            data: serde_json::Value::Null,
        },
        Ok(_) => CliResponse {
            ok: false,
            text: format!("no grounding for that on {slug}"),
            data: serde_json::Value::Null,
        },
        Err(error) => CliResponse {
            ok: false,
            text: error.to_string(),
            data: serde_json::Value::Null,
        },
    }
}

/// Reads units and unit price for a SKU.
async fn stock_of(state: &CliState, args: &[&str]) -> CliResponse {
    let [sku] = args else {
        return CliResponse {
            ok: false,
            text: "usage: stock <sku>".into(),
            data: serde_json::Value::Null,
        };
    };
    match state.commerce.stock_of(&state.site, sku).await {
        Ok(Some(stock)) => CliResponse {
            ok: true,
            text: format!(
                "{}: {} units at {} micro-{} each",
                stock.sku, stock.qty, stock.unit_micro, stock.currency
            ),
            data: serde_json::Value::Null,
        },
        Ok(None) => CliResponse {
            ok: false,
            text: format!("unknown sku {sku}"),
            data: serde_json::Value::Null,
        },
        Err(error) => CliResponse {
            ok: false,
            text: error.to_string(),
            data: serde_json::Value::Null,
        },
    }
}

/// Places a held order: reserves stock and cards the hold for approval.
async fn place_order(state: &CliState, args: &[&str]) -> CliResponse {
    let [sku, qty] = args else {
        return CliResponse {
            ok: false,
            text: "usage: order <sku> <qty>".into(),
            data: serde_json::Value::Null,
        };
    };
    let qty: i64 = match qty.parse() {
        Ok(qty) if qty > 0 => qty,
        _ => {
            return CliResponse {
                ok: false,
                text: "quantity must be a positive integer".into(),
                data: serde_json::Value::Null,
            };
        }
    };
    let order = NewOrder {
        sku: (*sku).to_string(),
        qty,
        buyer: "agent:cli".to_string(),
        idempotency_key: format!("cli-{}", uuid::Uuid::new_v4().as_simple()),
    };
    match state
        .commerce
        .order_hold(&state.site, &state.cards, order)
        .await
    {
        Ok((held, card)) => CliResponse {
            ok: true,
            text: format!(
                "held {} ({} micro-{}); approval card {}",
                held.id, held.total_micro, held.currency, card.id
            ),
            data: serde_json::Value::Null,
        },
        Err(error) => CliResponse {
            ok: false,
            text: error.to_string(),
            data: serde_json::Value::Null,
        },
    }
}

/// Opens a support thread: files the escalation that wakes the Chief of Staff.
async fn open_support(state: &CliState, label: &str, args: &[&str]) -> CliResponse {
    if args.is_empty() {
        return CliResponse {
            ok: false,
            text: "usage: support <message>".into(),
            data: serde_json::Value::Null,
        };
    }
    let signal = agentrix_site_concierge::Signal::SupportEscalation(
        agentrix_site_concierge::EscalationSignal {
            reason: args.join(" "),
        },
    );
    let kind = crate::signals::translate(&signal, label);
    let envelope = awp_types::Signal {
        id: uuid::Uuid::new_v4(),
        kind,
        emitted_by: format!("urn:adk:agent:{}/cli", state.site),
        emitted_at: chrono::Utc::now(),
    };
    match state.sink.emit(envelope).await {
        Ok(outcome) => CliResponse {
            ok: true,
            text: format!("support thread opened: {outcome:?}"),
            data: serde_json::Value::Null,
        },
        Err(error) => CliResponse {
            ok: false,
            text: error.to_string(),
            data: serde_json::Value::Null,
        },
    }
}

/// Captures a lead with the consent receipts the caller asserts.
async fn capture_lead(state: &CliState, args: &[&str]) -> CliResponse {
    let [channel, contact, consent_word, receipts, ..] = args else {
        return CliResponse {
            ok: false,
            text: "usage: lead <channel> <contact> consent <uuid,...>".into(),
            data: serde_json::Value::Null,
        };
    };
    if *consent_word != "consent" {
        return CliResponse {
            ok: false,
            text: "usage: lead <channel> <contact> consent <uuid,...>".into(),
            data: serde_json::Value::Null,
        };
    }
    let mut consent = Vec::new();
    for receipt in receipts.split(',') {
        if uuid::Uuid::parse_str(receipt.trim()).is_err() {
            return CliResponse {
                ok: false,
                text: format!("bad consent receipt {receipt}"),
                data: serde_json::Value::Null,
            };
        }
        consent.push(receipt.trim().to_string());
    }
    if consent.is_empty() {
        return CliResponse {
            ok: false,
            text: "a lead needs at least one consent receipt".into(),
            data: serde_json::Value::Null,
        };
    }
    let signal =
        agentrix_site_concierge::Signal::LeadCaptured(agentrix_site_concierge::LeadSignal {
            channel: (*channel).to_string(),
            contact_ref: (*contact).to_string(),
            consent,
        });
    let kind = crate::signals::translate(&signal, "cli");
    let envelope = awp_types::Signal {
        id: uuid::Uuid::new_v4(),
        kind,
        emitted_by: format!("urn:adk:agent:{}/cli", state.site),
        emitted_at: chrono::Utc::now(),
    };
    match state.sink.emit(envelope).await {
        Ok(outcome) => CliResponse {
            ok: true,
            text: format!("lead captured: {outcome:?}"),
            data: serde_json::Value::Null,
        },
        Err(error) => CliResponse {
            ok: false,
            text: error.to_string(),
            data: serde_json::Value::Null,
        },
    }
}

/// The CLI routes over shared `state`: one terminal, two tiers.
pub fn cli_routes(state: Arc<CliState>) -> Router {
    Router::new().route("/cli", post(cli)).with_state(state)
}
