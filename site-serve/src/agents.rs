//! The hosted agent teams: Muse through Baseten behind LlmAgents, FunctionTools
//! into the product stores, and a Sequential outreach team.
//!
//! Three runners cover the four agent targets: the Growth runner closes content
//! gaps (the reviews trigger shares it in v1, reading its job from the run
//! input), the Revenue runner is the qualifier-to-drafter outreach team, and the
//! Steward runner answers with corpus retrieval. Every agent call files cards or
//! quotes chunks — nothing publishes unwatched. Tools carry JSON schemas so the
//! model argues correctly, and every runner carries a tool-call budget.

use std::sync::Arc;

use adk_agent::{LlmAgentBuilder, SequentialAgent};
use adk_core::{Agent, Llm, RunBudget};
use adk_model::muse::{MuseClient, MuseConfig};
use adk_runner::Runner;
use adk_tool::FunctionTool;
use agentrix_site_concierge::Corpus;
use agentrix_site_inbox::{CardKind, CardStore, NewCard};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ProductStores, ServeConfig};

/// Builds the model: Muse through Baseten, or a scripted stand-in for `--dry-run`.
pub fn build_model(config: &ServeConfig) -> anyhow::Result<Arc<dyn Llm>> {
    if config.dry_run {
        let replies = (0..64)
            .map(|_| adk_model::ScriptedResponse::text("dry-run reply"))
            .collect::<Vec<_>>();
        return Ok(Arc::new(adk_model::ScriptedLlm::new(
            "muse-scripted",
            replies,
        )));
    }
    let key = config
        .baseten_key
        .clone()
        .ok_or_else(|| anyhow::anyhow!("BASETEN_API_KEY is not set (or pass --dry-run)"))?;
    let client = MuseClient::new(
        MuseConfig::new(key, &config.baseten_model)
            .with_base_url(&config.baseten_base_url)
            .with_max_tokens(config.max_tokens),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Arc::new(client))
}

/// One proposed fact.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FactArg {
    /// Stable fact id within the page.
    pub id: String,
    /// The fact's text.
    pub text: String,
}

/// Arguments for filing a content draft.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileDraftArgs {
    /// The page slug the draft improves.
    pub page_slug: String,
    /// Proposed facts.
    pub facts: Vec<FactArg>,
    /// Why the draft exists.
    pub rationale: String,
}

/// A tool filing content-draft cards: the Content Editor's hands.
pub fn file_draft_tool(cards: Arc<CardStore>, site: String) -> FunctionTool {
    FunctionTool::new(
        "file_content_draft",
        "Files a held content draft for a page. Always held for approval; never publishes.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let cards = cards.clone();
            let site = site.clone();
            async move {
                let parsed: FileDraftArgs = serde_json::from_value(args.clone())
                    .map_err(|e| adk_core::AdkError::config(format!("bad draft args: {e}")))?;
                let title = format!("Content Editor: agent draft for `{}`", parsed.page_slug);
                let card = cards
                    .file(NewCard::new(
                        site.as_str(),
                        CardKind::Draft,
                        "Content Editor",
                        title,
                        args,
                    ))
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                Ok(serde_json::json!({ "card": card.id }))
            }
        },
    )
    .with_parameters_schema::<FileDraftArgs>()
}

/// Arguments for quoting the corpus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetrieveArgs {
    /// The question to quote for.
    pub query: String,
}

/// A tool quoting the site's corpus: the Steward's memory.
pub fn retrieve_tool(corpus: Arc<Corpus>, site: String) -> FunctionTool {
    FunctionTool::new(
        "retrieve_chunks",
        "Quotes the site's own pages for a question. Returns the top chunks with documents and sources.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let corpus = corpus.clone();
            let site = site.clone();
            async move {
                let parsed: RetrieveArgs = serde_json::from_value(args)
                    .map_err(|e| adk_core::AdkError::config(format!("bad retrieve args: {e}")))?;
                let hits = agentrix_site_concierge::retrieve(&corpus, &site, &parsed.query, 3)
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                Ok(serde_json::json!({
                    "chunks": hits.iter().map(|hit| serde_json::json!({
                        "doc": hit.chunk.doc,
                        "text": hit.chunk.text,
                        "source": hit.chunk.source_uri,
                        "score": hit.score,
                    })).collect::<Vec<_>>(),
                }))
            }
        },
    )
    .with_parameters_schema::<RetrieveArgs>()
    .with_read_only(true)
    .with_concurrency_safe(true)
}

/// Builds the three hosted runners: Growth (content + reviews), Revenue (the
/// outreach team), Steward (support with retrieval). Every runner is tool-call
/// budgeted so a confused model cannot loop.
pub fn build_runners(
    app: &str,
    site: &str,
    stores: &ProductStores,
    sessions: Arc<dyn adk_session::SessionService>,
    model: Arc<dyn Llm>,
) -> anyhow::Result<Vec<(String, Arc<Runner>)>> {
    let growth_agent = LlmAgentBuilder::new("content_editor")
        .description("Closes content gaps with held drafts")
        .instruction(
            "You are the Content Editor. The run input carries content gaps. For each gap, \
             call file_content_draft with page_slug, facts (id + text pairs answering the gap), \
             and rationale. Draft only; approval is someone else's job.",
        )
        .model(model.clone())
        .tool(
            Arc::new(file_draft_tool(stores.cards.clone(), site.to_string()))
                as Arc<dyn adk_core::Tool>,
        )
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let growth = Runner::builder()
        .app_name(app)
        .agent(Arc::new(growth_agent) as Arc<dyn Agent>)
        .session_service(sessions.clone())
        .budget(RunBudget::new().max_tool_calls(8))
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let qualifier = LlmAgentBuilder::new("lead_qualifier")
        .description("Qualifies one lead")
        .instruction(
            "Given the lead JSON in the run input, reply with exactly one line: \
             QUALIFIED yes/no — one reason.",
        )
        .model(model.clone())
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let drafter = LlmAgentBuilder::new("followup_drafter")
        .description("Drafts the follow-up for a qualified lead")
        .instruction(
            "Given the qualification line, draft a short personal follow-up message. \
             Draft only; outreach never contacts unwatched.",
        )
        .model(model.clone())
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let outreach = SequentialAgent::new(
        "outreach_team",
        vec![
            Arc::new(qualifier) as Arc<dyn Agent>,
            Arc::new(drafter) as Arc<dyn Agent>,
        ],
    );
    let revenue = Runner::builder()
        .app_name(app)
        .agent(Arc::new(outreach) as Arc<dyn Agent>)
        .session_service(sessions.clone())
        .budget(RunBudget::new().max_tool_calls(4))
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let steward = LlmAgentBuilder::new("support_steward")
        .description("Answers escalations from the site's own pages")
        .instruction(
            "You are the Chief of Staff's support desk. For the escalated question in the run \
             input, call retrieve_chunks first, then answer quoting the chunks. If nothing grounds \
             the answer, say what is missing instead of guessing.",
        )
        .model(model)
        .tool(
            Arc::new(retrieve_tool(stores.corpus.clone(), site.to_string()))
                as Arc<dyn adk_core::Tool>,
        )
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let support = Runner::builder()
        .app_name(app)
        .agent(Arc::new(steward) as Arc<dyn Agent>)
        .session_service(sessions)
        .budget(RunBudget::new().max_tool_calls(8))
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(vec![
        ("growth".to_string(), Arc::new(growth)),
        ("revenue".to_string(), Arc::new(revenue)),
        ("steward".to_string(), Arc::new(support)),
    ])
}
