//! The hosted agent teams: Muse through Baseten behind LlmAgents, FunctionTools
//! into the product stores, and a Sequential outreach team.
//!
//! Three runners cover the four agent targets: the Growth runner closes content
//! gaps (the reviews trigger shares it, reading its job from the run input),
//! the Revenue runner is the qualifier-to-drafter outreach team, and the
//! Steward runner answers escalations with corpus retrieval. The signal-woken
//! runners pull their pending work from the kernel signal store (`next_*`
//! tools) and acknowledge each record by filing its held card — nothing
//! publishes unwatched. Tools carry JSON schemas so the model argues
//! correctly, and every runner carries a tool-call budget.

use std::sync::Arc;

use adk_agent::{LlmAgentBuilder, SequentialAgent};
use adk_core::{Agent, Llm, RunBudget};
use adk_kernel::PartitionId;
use adk_kernel::charter::{SignalFilter, SignalStore};
use adk_model::muse::{MuseClient, MuseConfig};
use adk_runner::Runner;
use adk_tool::FunctionTool;
use agentrix_site_concierge::Corpus;
use agentrix_site_defense::{Freeze, Guard, GuardDecision, QuietHours};
use agentrix_site_growth::{Growth, JobLevel};
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

/// No arguments: the pending-work tools take none.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NoArgs {}

/// Everything a tool gate checks: the ladder, the freeze switch, quiet hours.
#[derive(Clone)]
pub struct Gate {
    /// The ladder gate.
    pub growth: Arc<Growth>,
    /// The freeze switch.
    pub freeze: Arc<Freeze>,
    /// Quiet hours, when set.
    pub quiet: Option<QuietHours>,
    /// The site served.
    pub site: String,
    /// The job acting, gated when paused.
    pub job: &'static str,
}

/// The tools' gate: freeze and quiet hours first (audited skips), then the
/// ladder's eager demotion and floor check. Readers sit at Observe, draft
/// filers at Propose — filing the held card is the floor outcome, so it never
/// refuses on level.
async fn check_gate(gate: &Gate, tool: &str, floor: JobLevel) -> Result<(), adk_core::AdkError> {
    let guard = match gate.quiet {
        Some(quiet) => Guard::new(&gate.freeze).with_quiet(quiet),
        None => Guard::new(&gate.freeze),
    };
    match guard
        .check(&gate.site, tool, chrono::Utc::now())
        .await
        .map_err(|e| adk_core::AdkError::config(e.to_string()))?
    {
        GuardDecision::Run => {}
        GuardDecision::Frozen(reason) => {
            return Err(adk_core::AdkError::config(format!("site frozen: {reason}")));
        }
        GuardDecision::Deferred(wake) => {
            return Err(adk_core::AdkError::config(format!(
                "quiet hours: next wake {}",
                wake.to_rfc3339()
            )));
        }
    }
    gate.growth
        .enforce(&gate.site, gate.job, floor, tool)
        .await
        .map(|_| ())
        .map_err(|e| adk_core::AdkError::config(e.to_string()))
}

/// A tool reading the oldest open signal record of one kind: the job's pending
/// work. Returns the record's payload, or `{"empty": true}` when nothing waits.
/// Reading never acknowledges; the filing tool closes the record it handled.
/// What a pending-work tool reads: the store, the partition, the gate.
pub struct NextWork {
    /// The kernel signal store.
    pub signals: Arc<dyn SignalStore>,
    /// The back-office partition.
    pub partition: PartitionId,
    /// The freeze, quiet-hours and ladder gate.
    pub gate: Gate,
    /// The signal kind read.
    pub kind: &'static str,
}

pub fn next_tool(work: NextWork, name: &'static str, description: &'static str) -> FunctionTool {
    FunctionTool::new(
        name,
        description,
        move |_ctx: Arc<dyn adk_core::ToolContext>, _args: serde_json::Value| {
            let signals = work.signals.clone();
            let partition = work.partition.clone();
            let gate = work.gate.clone();
            let kind = work.kind;
            let tool = name;
            async move {
                check_gate(&gate, tool, JobLevel::Observe).await?;
                let filter = SignalFilter::default()
                    .partition(partition)
                    .kind(kind)
                    .open(true)
                    .limit(1);
                let records = signals
                    .list(&filter)
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                let Some(record) = records.into_iter().next() else {
                    return Ok(serde_json::json!({ "empty": true }));
                };
                payload(kind, &record.id, &record.signal.kind).ok_or_else(|| {
                    adk_core::AdkError::config(format!(
                        "signal record {} is not a {kind}",
                        record.id
                    ))
                })
            }
        },
    )
    .with_parameters_schema::<NoArgs>()
    .with_read_only(true)
}

/// The `next_*` payload for one open record: its id plus the kind's fields.
fn payload(kind: &str, record: &str, signal: &awp_types::SignalKind) -> Option<serde_json::Value> {
    use awp_types::SignalKind as Kind;
    match (kind, signal) {
        (
            "content_gap",
            Kind::ContentGap {
                topic,
                question_count,
                samples,
            },
        ) => Some(serde_json::json!({
            "record": record,
            "topic": topic.as_str(),
            "count": question_count,
            "samples": samples.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        })),
        (
            "review_seen",
            Kind::ReviewSeen {
                platform,
                rating,
                excerpt,
            },
        ) => Some(serde_json::json!({
            "record": record,
            "platform": platform,
            "rating": rating,
            "excerpt": excerpt.as_str(),
        })),
        (
            "lead_captured",
            Kind::LeadCaptured {
                lead_ref, source, ..
            },
        ) => Some(serde_json::json!({
            "record": record,
            "lead_ref": lead_ref.0,
            "channel": channel_name(source),
        })),
        (
            "support_escalation",
            Kind::SupportEscalation {
                reason,
                contact_ref,
            },
        ) => Some(serde_json::json!({
            "record": record,
            "reason": reason.0,
            "contact": contact_ref.0,
        })),
        _ => None,
    }
}

/// The wire name of a channel: its serialized form, or `other`.
fn channel_name(channel: &awp_types::Channel) -> String {
    serde_json::to_value(channel)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "other".to_string())
}

/// Closes the handled signal record, if any: filing the card acknowledges the
/// work, so the next wake reads what is still open.
async fn acknowledge(
    signals: &Arc<dyn SignalStore>,
    record: Option<&str>,
) -> Result<(), adk_core::AdkError> {
    if let Some(record) = record {
        signals
            .close(record)
            .await
            .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
    }
    Ok(())
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
    /// The gap record handled, closed on filing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_record: Option<String>,
}

/// A tool filing content-draft cards: the Content Editor's hands.
pub fn file_draft_tool(
    cards: Arc<CardStore>,
    signals: Arc<dyn SignalStore>,
    gate: Gate,
) -> FunctionTool {
    FunctionTool::new(
        "file_content_draft",
        "Files a held content draft for a page. Always held for approval; never publishes.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let cards = cards.clone();
            let signals = signals.clone();
            let gate = gate.clone();
            async move {
                check_gate(&gate, "file_content_draft", JobLevel::Propose).await?;
                let parsed: FileDraftArgs = serde_json::from_value(args.clone())
                    .map_err(|e| adk_core::AdkError::config(format!("bad draft args: {e}")))?;
                let title = format!("Content Editor: agent draft for `{}`", parsed.page_slug);
                let card = cards
                    .file(NewCard::new(
                        gate.site.as_str(),
                        CardKind::Draft,
                        "Content Editor",
                        title,
                        args,
                    ))
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                acknowledge(&signals, parsed.gap_record.as_deref()).await?;
                Ok(serde_json::json!({ "card": card.id }))
            }
        },
    )
    .with_parameters_schema::<FileDraftArgs>()
}

/// Arguments for filing a reply draft.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileReplyArgs {
    /// The platform the review appeared on.
    pub platform: String,
    /// The drafted reply, in the owner's voice.
    pub reply: String,
    /// The review record handled, closed on filing.
    pub review_record: String,
}

/// A tool filing reply drafts: the Reputation Manager's hands. Held, like every
/// draft; posting is a separate approved act.
pub fn file_reply_draft_tool(
    cards: Arc<CardStore>,
    signals: Arc<dyn SignalStore>,
    gate: Gate,
) -> FunctionTool {
    FunctionTool::new(
        "file_reply_draft",
        "Files a held reply draft for a seen review. Never posts; the owner approves first.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let cards = cards.clone();
            let signals = signals.clone();
            let gate = gate.clone();
            async move {
                check_gate(&gate, "file_reply_draft", JobLevel::Propose).await?;
                let parsed: FileReplyArgs = serde_json::from_value(args.clone())
                    .map_err(|e| adk_core::AdkError::config(format!("bad reply args: {e}")))?;
                let title = format!("Reputation Manager: reply draft for {}", parsed.platform);
                let card = cards
                    .file(NewCard::new(
                        gate.site.as_str(),
                        CardKind::Draft,
                        "Reputation Manager",
                        title,
                        args,
                    ))
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                acknowledge(&signals, Some(parsed.review_record.as_str())).await?;
                Ok(serde_json::json!({ "card": card.id }))
            }
        },
    )
    .with_parameters_schema::<FileReplyArgs>()
}

/// Arguments for filing a follow-up draft.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileFollowupArgs {
    /// The lead reference from `next_lead`.
    pub lead_ref: String,
    /// The channel the lead came over.
    pub channel: String,
    /// The drafted follow-up message.
    pub message: String,
    /// The lead record handled, closed on filing.
    pub lead_record: String,
}

/// A tool filing follow-up drafts: the Sales Rep's hands. Held; outreach never
/// contacts unwatched.
pub fn file_followup_tool(
    cards: Arc<CardStore>,
    signals: Arc<dyn SignalStore>,
    gate: Gate,
) -> FunctionTool {
    FunctionTool::new(
        "file_followup",
        "Files a held follow-up draft for a qualified lead. Never sends; the owner approves first.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let cards = cards.clone();
            let signals = signals.clone();
            let gate = gate.clone();
            async move {
                check_gate(&gate, "file_followup", JobLevel::Propose).await?;
                let parsed: FileFollowupArgs = serde_json::from_value(args.clone())
                    .map_err(|e| adk_core::AdkError::config(format!("bad followup args: {e}")))?;
                let title = format!("Sales Rep: follow up with lead {}", parsed.lead_ref);
                let card = cards
                    .file(NewCard::new(
                        gate.site.as_str(),
                        CardKind::Draft,
                        "Sales Rep",
                        title,
                        args,
                    ))
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                acknowledge(&signals, Some(parsed.lead_record.as_str())).await?;
                Ok(serde_json::json!({ "card": card.id }))
            }
        },
    )
    .with_parameters_schema::<FileFollowupArgs>()
}

/// Arguments for filing an answer draft.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileAnswerArgs {
    /// The grounded answer.
    pub answer: String,
    /// The escalation record handled, closed on filing.
    pub escalation_record: String,
}

/// A tool filing answer drafts: the Chief of Staff's hands. The answer quotes
/// the corpus; the owner approves before it reaches the visitor.
pub fn file_answer_draft_tool(
    cards: Arc<CardStore>,
    signals: Arc<dyn SignalStore>,
    gate: Gate,
) -> FunctionTool {
    FunctionTool::new(
        "file_answer_draft",
        "Files a held answer draft for an escalation. Never sends; the owner approves first.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let cards = cards.clone();
            let signals = signals.clone();
            let gate = gate.clone();
            async move {
                check_gate(&gate, "file_answer_draft", JobLevel::Propose).await?;
                let parsed: FileAnswerArgs = serde_json::from_value(args.clone())
                    .map_err(|e| adk_core::AdkError::config(format!("bad answer args: {e}")))?;
                let title = "Chief of Staff: answer draft for an escalation".to_string();
                let card = cards
                    .file(NewCard::new(
                        gate.site.as_str(),
                        CardKind::Draft,
                        "Chief of Staff",
                        title,
                        args,
                    ))
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                acknowledge(&signals, Some(parsed.escalation_record.as_str())).await?;
                Ok(serde_json::json!({ "card": card.id }))
            }
        },
    )
    .with_parameters_schema::<FileAnswerArgs>()
}

/// Arguments for quoting the corpus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetrieveArgs {
    /// The question to quote for.
    pub query: String,
}

/// A tool quoting the site's corpus: the Steward's memory.
pub fn retrieve_tool(corpus: Arc<Corpus>, gate: Gate) -> FunctionTool {
    FunctionTool::new(
        "retrieve_chunks",
        "Quotes the site's own pages for a question. Returns the top chunks with documents and sources.",
        move |_ctx: Arc<dyn adk_core::ToolContext>, args: serde_json::Value| {
            let corpus = corpus.clone();
            let gate = gate.clone();
            async move {
                check_gate(&gate, "retrieve_chunks", JobLevel::Observe).await?;
                let parsed: RetrieveArgs = serde_json::from_value(args)
                    .map_err(|e| adk_core::AdkError::config(format!("bad retrieve args: {e}")))?;
                let hits = agentrix_site_concierge::retrieve(&corpus, &gate.site, &parsed.query, 3)
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
/// outreach team), Steward (support with retrieval). Every runner pulls pending
/// work from the signal store and is tool-call budgeted so a confused model
/// cannot loop.
pub fn build_runners(
    app: &str,
    site: &str,
    stores: &ProductStores,
    sessions: Arc<dyn adk_session::SessionService>,
    model: Arc<dyn Llm>,
    signals: Arc<dyn SignalStore>,
    quiet: Option<QuietHours>,
) -> anyhow::Result<Vec<(String, Arc<Runner>)>> {
    let partition: PartitionId = format!("{app}.back-office").into();
    let gate = |job: &'static str| Gate {
        growth: stores.growth.clone(),
        freeze: stores.freeze.clone(),
        quiet,
        site: site.to_string(),
        job,
    };
    let growth_agent = LlmAgentBuilder::new("content_editor")
        .description("Closes content gaps with held drafts")
        .instruction(
            "You close content gaps and draft review replies. Read the run input's `job`. \
             For `freshness`: call next_gap. On `{\"empty\": true}` reply NO_GAPS and stop. \
             Else call file_content_draft with page_slug, facts (id + text pairs answering \
             the gap), rationale, and the gap_record. For `social`: call next_review. On \
             `{\"empty\": true}` reply NO_REVIEWS and stop. Else call file_reply_draft with \
             platform, reply, and the review_record. Draft only; approval is someone else's job.",
        )
        .model(model.clone())
        .tool(
            Arc::new(next_tool(
                NextWork {
                    signals: signals.clone(),
                    partition: partition.clone(),
                    gate: gate("freshness"),
                    kind: "content_gap",
                },
                "next_gap",
                "Reads the oldest open content gap: record id, topic, count, samples. `{\"empty\": true}` when none waits.",
            )) as Arc<dyn adk_core::Tool>,
        )
        .tool(
            Arc::new(file_draft_tool(
                stores.cards.clone(),
                signals.clone(),
                gate("freshness"),
            )) as Arc<dyn adk_core::Tool>,
        )
        .tool(
            Arc::new(next_tool(
                NextWork {
                    signals: signals.clone(),
                    partition: partition.clone(),
                    gate: gate("social"),
                    kind: "review_seen",
                },
                "next_review",
                "Reads the oldest open review sighting: record id, platform, rating, excerpt. `{\"empty\": true}` when none waits.",
            )) as Arc<dyn adk_core::Tool>,
        )
        .tool(
            Arc::new(file_reply_draft_tool(
                stores.cards.clone(),
                signals.clone(),
                gate("social"),
            )) as Arc<dyn adk_core::Tool>,
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
            "Call next_lead. On `{\"empty\": true}` reply NO_LEADS and stop. Else reply with \
             exactly one line: QUALIFIED yes/no, the lead_ref, the channel, the lead_record, \
             Example: QUALIFIED yes lead-9 email rec-4 — asked for pricing.",
        )
        .model(model.clone())
        .tool(
            Arc::new(next_tool(
                NextWork {
                    signals: signals.clone(),
                    partition: partition.clone(),
                    gate: gate("nurture"),
                    kind: "lead_captured",
                },
                "next_lead",
                "Reads the oldest open captured lead: record id, lead_ref, channel. `{\"empty\": true}` when none waits.",
            )) as Arc<dyn adk_core::Tool>,
        )
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let drafter = LlmAgentBuilder::new("followup_drafter")
        .description("Drafts the follow-up for a qualified lead")
        .instruction(
            "Given the qualification line, call file_followup with its lead_ref, channel, the \
             lead_record it carries, and your short personal message. On NO_LEADS reply \
             NOTHING_TO_DO and stop. Draft only; outreach never contacts unwatched.",
        )
        .model(model.clone())
        .tool(Arc::new(file_followup_tool(
            stores.cards.clone(),
            signals.clone(),
            gate("nurture"),
        )) as Arc<dyn adk_core::Tool>)
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
            "You are the Chief of Staff's support desk. Call next_escalation. On \
             `{\"empty\": true}` reply NO_ESCALATIONS and stop. Else call retrieve_chunks for \
             the reason, then file_answer_draft with the grounded answer quoting the chunks \
             and the escalation_record. If nothing grounds the answer, say what is missing \
             instead of guessing.",
        )
        .model(model)
        .tool(
            Arc::new(next_tool(
                NextWork {
                    signals: signals.clone(),
                    partition: partition.clone(),
                    gate: gate("owner-questions"),
                    kind: "support_escalation",
                },
                "next_escalation",
                "Reads the oldest open escalation: record id, reason, contact. `{\"empty\": true}` when none waits.",
            )) as Arc<dyn adk_core::Tool>,
        )
        .tool(
            Arc::new(retrieve_tool(stores.corpus.clone(), gate("owner-questions")))
                as Arc<dyn adk_core::Tool>,
        )
        .tool(
            Arc::new(file_answer_draft_tool(
                stores.cards.clone(),
                signals,
                gate("owner-questions"),
            )) as Arc<dyn adk_core::Tool>,
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
