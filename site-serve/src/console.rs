//! The owner console: the inbox as a web page. Open cards render with
//! approve/reject buttons; approving runs the held effect (completing orders
//! and payments), rejecting closes without effect. Tokens issue here too.

use std::sync::Arc;

use agentrix_site_commerce::Commerce;
use agentrix_site_growth::Growth;
use agentrix_site_inbox::{Card, CardStore, Decision};
use axum::{
    Form, Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use crate::cli::TokenStore;

/// What the console touches.
#[derive(Clone)]
pub struct ConsoleState {
    /// The site served.
    pub site: String,
    /// Decision cards.
    pub cards: Arc<CardStore>,
    /// Orders and payments for approved effects.
    pub commerce: Arc<Commerce>,
    /// Budgets for payment fees.
    pub growth: Arc<Growth>,
    /// Bearer tokens.
    pub tokens: Arc<TokenStore>,
}

struct Noop;

impl agentrix_site_inbox::Execute for Noop {
    fn execute(&self, _card: &Card) -> Result<(), String> {
        Ok(())
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The inbox: every open card with its decision buttons, plus token issuance.
async fn inbox(State(state): State<Arc<ConsoleState>>) -> impl IntoResponse {
    let open = match state.cards.list_open(&state.site).await {
        Ok(open) => open,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Html(error.to_string())).into_response();
        }
    };
    let mut rows = String::new();
    for card in &open {
        rows.push_str(&format!(
            "      <tr><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td><form method=\"post\" action=\"/console/decide\"><input type=\"hidden\" name=\"card_id\" value=\"{}\"><button name=\"decision\" value=\"approve\">Approve</button> <button name=\"decision\" value=\"reject\">Reject</button></form></td></tr>\n",
            escape(&card.role),
            escape(&card.title),
            card.kind,
            escape(&card.id),
            escape(&card.id),
        ));
    }
    Html(format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head><meta charset=\"utf-8\"><title>Owner console</title></head>\n  <body><main><h1>Owner console</h1><table>\n      <tr><th>Role</th><th>Title</th><th>Kind</th><th>Card</th><th>Decide</th></tr>\n{}</table>\n    <h2>Agent tokens</h2><form method=\"post\" action=\"/console/tokens\"><input name=\"label\" placeholder=\"stall-7\" required> <button>Issue verified token</button></form></main></body>\n</html>\n",
        rows
    ))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct DecideForm {
    card_id: String,
    decision: String,
}

/// Records the decision, then runs the held effect for approvals: orders and
/// payments complete against the deciding card.
async fn decide(
    State(state): State<Arc<ConsoleState>>,
    Form(form): Form<DecideForm>,
) -> impl IntoResponse {
    let decision = match form.decision.as_str() {
        "approve" => Decision::Approve,
        "reject" => Decision::Reject,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "decision must be approve or reject",
            )
                .into_response();
        }
    };
    let card = match state.cards.decide(&form.card_id, decision, &Noop).await {
        Ok(card) => card,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
    };
    if decision == Decision::Approve
        && let Err(error) = run_effects(&state, &card).await
    {
        return (StatusCode::CONFLICT, error).into_response();
    }
    Redirect::to("/console").into_response()
}

/// Runs the held effects an approval unlocks: orders and payments complete
/// against the deciding card.
async fn run_effects(state: &ConsoleState, card: &Card) -> Result<(), String> {
    if let Some(order) = card.payload.get("order").and_then(|v| v.as_str()) {
        state
            .commerce
            .order_complete(&state.cards, order, &card.id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())?;
    }
    if let Some(intent) = card.payload.get("intent").and_then(|v| v.as_str()) {
        state
            .commerce
            .complete(&state.growth, &state.cards, intent, &card.id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct TokenForm {
    label: String,
}

/// Issues a verified token; the plaintext shows once.
async fn issue_token(
    State(state): State<Arc<ConsoleState>>,
    Form(form): Form<TokenForm>,
) -> impl IntoResponse {
    match state.tokens.issue(&state.site, &form.label).await {
        Ok(token) => Html(format!(
            "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>Token issued</title></head><body><main><h1>Token issued</h1><p>Copy it now; it shows once.</p><code>{}</code><p><a href=\"/console\">Back to the console</a></p></main></body>\n</html>\n",
            token
        ))
        .into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

/// The console routes over shared `state`.
pub fn console_routes(state: Arc<ConsoleState>) -> Router {
    Router::new()
        .route("/console", get(inbox))
        .route("/console/decide", post(decide))
        .route("/console/tokens", post(issue_token))
        .with_state(state)
}
