//! Server-side LLM agent that drives the strategy authoring loop. The user
//! sends one chat message; this struct repeatedly calls the LLM with the
//! strategy/eval verbs as `ToolDefinition`s, routes any `ToolUse` blocks
//! the model emits to `xvision_engine::authoring`, persists every turn
//! (user / assistant / tool_result) to the `chat_sessions` store, and
//! re-calls until the model responds with a text-only `EndTurn`.
//!
//! Plan 2d Phase 2D.B Task 6 + Plan #11 Phase B Task 3 (chat-rail
//! persistence). Stacks on Plan 2a Phase 2A.B (PR #31; the seven
//! authoring verbs in the engine module), Phase 2A.C T10 (PR #33; the
//! multi-turn `Message`/`ContentBlock`/`ToolDefinition` shape), and
//! Plan #11 Phase A (PR #44; `ChatSessionStore` + `ContextScope`).
//!
//! Deliberately surface-agnostic at this layer — the SSE routes in
//! `routes::wizard` and `routes::chat_rail` wrap `WizardEvent`s into an
//! `event-stream` body. Tests drive the loop directly with a
//! `MockDispatch::sequence(...)` against a tempdir-backed sqlite.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason, ToolDefinition,
};
use xvision_engine::api::eval::{self as api_eval, EvalRunRequest};
use xvision_engine::api::scenario::{self as api_scenario, CreateScenarioRequest, ListScenariosFilter};
use xvision_engine::authoring;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};
use xvision_engine::eval::run::RunMode;

const WIZARD_SYSTEM_PROMPT_BASE: &str = include_str!("../prompts/wizard.md");

/// Cap on tool-use → tool-result iterations per `next_event` call. Prevents
/// a misbehaving model from looping forever; v1 wizards never need more
/// than 3-4 round trips per user turn.
const MAX_TOOL_LOOP_ITERATIONS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WizardEvent {
    /// A chunk of assistant text. The SSE route streams these as they
    /// arrive; clients append to the visible bubble.
    Token { text: String },
    /// The agent is about to call an authoring verb. Front-end uses this
    /// to render a "running tool" indicator.
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
    /// Result of the most-recent tool call. Front-end uses this to update
    /// the displayed draft state.
    ToolResult {
        tool: String,
        result: serde_json::Value,
    },
    /// Conversation complete. `draft_id` carries the most recently created
    /// or referenced strategy id (if any), so the front-end can transition
    /// to the inspector view.
    Done { draft_id: Option<String> },
    /// The dispatch errored or the loop hit a hard cap.
    Error { message: String },
}

pub struct WizardLoop {
    api_context: ApiContext,
    dispatch: Arc<dyn LlmDispatch>,
    model: String,
    pool: SqlitePool,
    session_id: String,
    scope: ContextScope,
    /// Tracked across iterations: the most recent strategy id mentioned in
    /// a tool-call/-result. Used to populate `Done.draft_id`.
    last_draft_id: Option<String>,
    /// Pending events queued during the current `next_event` invocation.
    pending: Vec<WizardEvent>,
    is_done: bool,
}

impl WizardLoop {
    /// Construct a session-aware wizard loop. The user's `new_message` is
    /// persisted as a `user` text block in the chat session store BEFORE
    /// any LLM call so even a dispatch failure leaves the message in
    /// history. Subsequent LLM turns are reconstructed from the store, not
    /// in-memory state — that's the load-bearing change for the rail
    /// (Plan #11): a session can pause and resume across HTTP requests.
    pub async fn new(
        xvn_home: PathBuf,
        dispatch: Arc<dyn LlmDispatch>,
        model: String,
        pool: SqlitePool,
        session_id: String,
        scope: ContextScope,
        new_message: String,
    ) -> anyhow::Result<Self> {
        let user_block = serde_json::json!({"type": "text", "text": new_message});
        ChatSessionStore::append(&pool, &session_id, "user", &[user_block]).await?;
        let api_context = ApiContext::new(
            pool.clone(),
            Actor::Cli {
                user: "wizard".to_string(),
            },
            xvn_home,
        );
        Ok(Self {
            api_context,
            dispatch,
            model,
            pool,
            session_id,
            scope,
            last_draft_id: None,
            pending: vec![],
            is_done: false,
        })
    }

    /// Pop one event. The caller streams these to the client one-by-one
    /// (e.g. via SSE); when this returns `None` the loop is finished.
    pub async fn next_event(&mut self) -> Option<WizardEvent> {
        if let Some(ev) = self.pending.pop() {
            return Some(ev);
        }
        if self.is_done {
            return None;
        }
        if let Err(e) = self.run_one_turn().await {
            self.is_done = true;
            return Some(WizardEvent::Error {
                message: e.to_string(),
            });
        }
        // pending is filled in chronological order, but pop() takes from the
        // back — reverse so the caller streams in the right sequence.
        self.pending.reverse();
        self.pending.pop()
    }

    fn system_prompt(&self) -> String {
        // Plan #11 Phase B Task 3 Step 2: inject scope header so the model
        // knows what the user is asking about (workspace, a specific run,
        // a draft, etc.). Tool calls remain available for deeper info.
        format!(
            "{base}\n\n## Current context\n{header}\n",
            base = WIZARD_SYSTEM_PROMPT_BASE,
            header = self.scope.header_label()
        )
    }

    async fn run_one_turn(&mut self) -> anyhow::Result<()> {
        for _ in 0..MAX_TOOL_LOOP_ITERATIONS {
            let messages = self.load_messages_from_store().await?;
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: self.system_prompt(),
                messages,
                max_tokens: 1500,
                tools: wizard_tool_defs(),
            };
            let resp: LlmResponse = self.dispatch.complete(req).await?;

            // Persist the assistant turn — text + any tool_use blocks all
            // go to the same row. The store keeps the same JSON shape
            // ContentBlock derives, so reads round-trip back to Message.
            let assistant_blocks: Vec<serde_json::Value> = resp
                .content
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()?;
            ChatSessionStore::append(
                &self.pool,
                &self.session_id,
                "assistant",
                &assistant_blocks,
            )
            .await?;

            // Emit Token events for any text blocks the model produced.
            for block in &resp.content {
                if let ContentBlock::Text { text } = block {
                    if !text.is_empty() {
                        self.pending
                            .push(WizardEvent::Token { text: text.clone() });
                    }
                }
            }

            let tool_uses: Vec<(String, String, serde_json::Value)> = resp
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            if tool_uses.is_empty() {
                self.is_done = true;
                self.pending.push(WizardEvent::Done {
                    draft_id: self.last_draft_id.clone(),
                });
                return Ok(());
            }

            // Run each tool, build a tool_result block per call, emit
            // ToolCall + ToolResult WizardEvents, persist all results as
            // one user turn.
            let mut tool_result_blocks: Vec<serde_json::Value> =
                Vec::with_capacity(tool_uses.len());
            for (id, name, input) in tool_uses {
                self.pending.push(WizardEvent::ToolCall {
                    tool: name.clone(),
                    args: input.clone(),
                });
                let result = self.run_tool(&name, input).await;
                let result_value = match result {
                    Ok(v) => v,
                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                };
                self.maybe_track_draft_id(&name, &result_value);
                self.pending.push(WizardEvent::ToolResult {
                    tool: name.clone(),
                    result: result_value.clone(),
                });
                tool_result_blocks.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": result_value.to_string(),
                }));
            }
            ChatSessionStore::append(
                &self.pool,
                &self.session_id,
                "user",
                &tool_result_blocks,
            )
            .await?;

            if !matches!(resp.stop_reason, StopReason::ToolUse) {
                // Defensive: the model said EndTurn/MaxTokens but emitted
                // tool_uses. Anthropic shouldn't do this, but if it does
                // we've already persisted the tool_results; finish the turn.
                self.is_done = true;
                self.pending.push(WizardEvent::Done {
                    draft_id: self.last_draft_id.clone(),
                });
                return Ok(());
            }
        }
        anyhow::bail!(
            "wizard tool-use loop exceeded {MAX_TOOL_LOOP_ITERATIONS} iterations \
             — model is stuck calling tools without responding"
        );
    }

    /// Reconstruct the message log from the persisted store. Each
    /// `ChatMessage.content_blocks` is a `Vec<serde_json::Value>` whose
    /// shape matches `ContentBlock`'s tagged-union derive — round-trip
    /// via `from_value`.
    async fn load_messages_from_store(&self) -> anyhow::Result<Vec<Message>> {
        let history =
            ChatSessionStore::load_history(&self.pool, &self.session_id).await?;
        let mut out = Vec::with_capacity(history.len());
        for cm in history {
            let mut blocks = Vec::with_capacity(cm.content_blocks.len());
            for v in cm.content_blocks {
                let block: ContentBlock = serde_json::from_value(v)?;
                blocks.push(block);
            }
            out.push(Message {
                role: cm.role,
                content: blocks,
            });
        }
        Ok(out)
    }

    fn maybe_track_draft_id(&mut self, tool: &str, result: &serde_json::Value) {
        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            self.last_draft_id = Some(id.to_string());
            return;
        }
        // For get_strategy the strategy's manifest.id is what we want.
        if tool == "get_strategy" {
            if let Some(id) = result
                .get("manifest")
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
            {
                self.last_draft_id = Some(id.to_string());
            }
        }
    }

    async fn run_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match name {
            "list_templates" => {
                let out = authoring::list_templates();
                Ok(serde_json::to_value(out)?)
            }
            "create_strategy" => {
                let req: authoring::CreateStrategyReq = serde_json::from_value(input)?;
                let out = xvision_engine::api::strategy::create_strategy(&self.api_context, req)
                    .await?;
                Ok(serde_json::to_value(out)?)
            }
            "get_strategy" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_strategy: missing `id`"))?;
                let out = xvision_engine::api::strategy::get(&self.api_context, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "list_strategies" => {
                let out = xvision_engine::api::strategy::list(&self.api_context).await?;
                Ok(serde_json::to_value(out)?)
            }
            "list_scenarios" => {
                let out = api_scenario::list(&self.api_context, ListScenariosFilter::default()).await?;
                Ok(serde_json::to_value(out)?)
            }
            "get_scenario" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_scenario: missing `id`"))?;
                let out = api_scenario::get(&self.api_context, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "create_scenario" => {
                let req: CreateScenarioRequest = serde_json::from_value(input)?;
                let out = api_scenario::create(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "update_slot" => {
                let req: authoring::UpdateSlotReq = serde_json::from_value(input)?;
                let out = xvision_engine::api::strategy::update_slot(&self.api_context, req)
                    .await?;
                Ok(serde_json::to_value(out)?)
            }
            "set_mechanical_param" => {
                let req: authoring::SetMechanicalParamReq = serde_json::from_value(input)?;
                let out =
                    xvision_engine::api::strategy::set_mechanical_param(&self.api_context, req)
                        .await?;
                Ok(serde_json::to_value(out)?)
            }
            "set_risk_config" => {
                let req: authoring::SetRiskConfigReq = serde_json::from_value(input)?;
                let out =
                    xvision_engine::api::strategy::set_risk_config(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "validate_draft" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("validate_draft: missing `id`"))?;
                let out = xvision_engine::api::strategy::validate_draft(&self.api_context, id)
                    .await?;
                Ok(serde_json::to_value(out)?)
            }
            "run_eval" => {
                let agent_id = input
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("run_eval: missing `agent_id`"))?;
                let scenario_id = input
                    .get("scenario_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("run_eval: missing `scenario_id`"))?;
                let mode = input
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("backtest");
                let mode = match mode {
                    "paper" => RunMode::Paper,
                    _ => RunMode::Backtest,
                };
                let req = EvalRunRequest {
                    agent_id: agent_id.to_string(),
                    scenario_id: scenario_id.to_string(),
                    mode,
                    params_override: None,
                };
                let out = api_eval::start_run(
                    &xvision_engine::api::ApiContext::new(
                        self.pool.clone(),
                        xvision_engine::api::Actor::Cli {
                            user: "dashboard".to_string(),
                        },
                        self.api_context.xvn_home.clone(),
                    ),
                    req,
                )
                .await?;
                Ok(serde_json::json!({
                    "run_id": out.summary.id,
                    "status": out.summary.status,
                    "scenario_id": out.summary.scenario_id
                }))
            }
            other => anyhow::bail!("unknown authoring verb: {other}"),
        }
    }
}

/// Authoring/eval verbs as `ToolDefinition`s. The schemas mirror the
/// engine's request structs but only declare the fields a model needs;
/// optional fields are omitted from `required`.
fn wizard_tool_defs() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_templates".into(),
            description: "List the strategy templates with display name + plain summary.".into(),
            input_schema: serde_json::json!({
                "type": "object", "properties": {}, "required": []
            }),
        },
        ToolDefinition {
            name: "create_strategy".into(),
            description: "Instantiate a new draft from a template. Returns { id }.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "template": {"type": "string", "description": "Template name from list_templates"},
                    "name": {"type": "string", "description": "Human-readable name"},
                    "creator": {"type": "string", "description": "Optional @handle"}
                },
                "required": ["template", "name"]
            }),
        },
        ToolDefinition {
            name: "get_strategy".into(),
            description: "Read the current draft state. Returns the Strategy JSON.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "list_strategies".into(),
            description: "List existing strategy drafts with ids, template names, and model labels.".into(),
            input_schema: serde_json::json!({
                "type": "object", "properties": {}, "required": []
            }),
        },
        ToolDefinition {
            name: "list_scenarios".into(),
            description: "List available scenarios, including canonical and user-created scenarios.".into(),
            input_schema: serde_json::json!({
                "type": "object", "properties": {}, "required": []
            }),
        },
        ToolDefinition {
            name: "get_scenario".into(),
            description: "Read a scenario by id.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "create_scenario".into(),
            description: "Create a scenario. Use list_scenarios first when the user asks for an existing scenario; only create when they request a new one and provide the required fields.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "display_name": {"type": "string"},
                    "description": {"type": "string"},
                    "asset_class": {"type": "string"},
                    "asset": {"type": "array", "items": {"type": "object"}},
                    "quote_currency": {"type": "string"},
                    "time_window": {"type": "object"},
                    "capital": {"type": "object"},
                    "granularity": {"type": "string"},
                    "timezone": {"type": "string"},
                    "calendar": {"type": "object"},
                    "venue": {"type": "object"},
                    "data_source": {"type": "object"},
                    "replay_mode": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "notes": {"type": ["string", "null"]},
                    "parent_scenario_id": {"type": ["string", "null"]},
                    "source": {"type": "string"}
                },
                "required": [
                    "display_name", "description", "asset_class", "asset",
                    "quote_currency", "time_window", "capital", "granularity",
                    "timezone", "calendar", "venue", "data_source",
                    "replay_mode", "tags", "source"
                ]
            }),
        },
        ToolDefinition {
            name: "update_slot".into(),
            description: "Update a regime/intern/trader slot. Only fields with non-null values are mutated.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "slot": {"type": "string", "enum": ["regime", "intern", "trader"]},
                    "prompt": {"type": "string"},
                    "model_requirement": {"type": "string"},
                    "provider": {"type": "string"},
                    "model": {"type": "string"},
                    "allowed_tools": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["id", "slot"]
            }),
        },
        ToolDefinition {
            name: "set_mechanical_param".into(),
            description: "Set a key inside Strategy.mechanical_params (template-specific).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "key": {"type": "string"},
                    "value": {}
                },
                "required": ["id", "key", "value"]
            }),
        },
        ToolDefinition {
            name: "set_risk_config".into(),
            description: "Apply a risk preset (conservative/balanced/aggressive) OR an explicit RiskConfig. Mutually exclusive.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "preset": {"type": "string", "enum": ["conservative", "balanced", "aggressive"]},
                    "explicit": {"type": "object"}
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "validate_draft".into(),
            description: "Validate a strategy draft. Returns { id, ok, errors }.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "run_eval".into(),
            description: "Start an eval run for a strategy and scenario.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {"type": "string"},
                    "scenario_id": {"type": "string"},
                    "mode": {"type": "string", "enum": ["backtest", "paper"]}
                },
                "required": ["agent_id", "scenario_id"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use xvision_engine::agent::llm::MockDispatch;

    /// Build a real sqlite-backed pool against a tempdir + run engine
    /// migrations. Each test gets its own DB so concurrent runs don't
    /// step on each other's chat sessions.
    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("xvn.db");
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::migrate!("../xvision-engine/migrations")
            .run(&pool)
            .await
            .unwrap();
        (pool, td)
    }

    async fn loop_with_session(
        dispatch: Arc<dyn LlmDispatch>,
        msg: &str,
        scope: ContextScope,
    ) -> (WizardLoop, SqlitePool, tempfile::TempDir, String) {
        let (pool, td) = fresh_pool().await;
        let session_id = ChatSessionStore::create_session(&pool, &scope).await.unwrap();
        let wl = WizardLoop::new(
            td.path().to_path_buf(),
            dispatch,
            "claude-sonnet-4-6".into(),
            pool.clone(),
            session_id.clone(),
            scope,
            msg.into(),
        )
        .await
        .unwrap();
        (wl, pool, td, session_id)
    }

    async fn drain(wl: &mut WizardLoop) -> Vec<WizardEvent> {
        let mut out = vec![];
        while let Some(ev) = wl.next_event().await {
            out.push(ev);
        }
        out
    }

    async fn seed_defaults(pool: &SqlitePool, td: &tempfile::TempDir) {
        let ctx = ApiContext::new(
            pool.clone(),
            Actor::Cli {
                user: "wizard-test".to_string(),
            },
            td.path().to_path_buf(),
        );
        xvision_engine::eval::scenario_seed::run_seed_if_needed(&ctx)
            .await
            .expect("seed canonical scenarios and default strategy");
    }

    #[tokio::test]
    async fn text_only_response_emits_token_then_done() {
        let mock = Arc::new(MockDispatch::echo("Sure — which template?"));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "help me build a strategy", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        assert_eq!(events.len(), 2, "events: {events:?}");
        assert!(matches!(&events[0], WizardEvent::Token { text } if text.contains("which template")));
        assert!(matches!(&events[1], WizardEvent::Done { draft_id: None }));
    }

    #[tokio::test]
    async fn tool_use_runs_authoring_verb_and_appends_text() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "list_templates", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "I see 9 templates. Want trend_follower?".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "what can i build", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        assert!(matches!(&events[0], WizardEvent::ToolCall { tool, .. } if tool == "list_templates"));
        match &events[1] {
            WizardEvent::ToolResult { tool, result } => {
                assert_eq!(tool, "list_templates");
                assert!(result.as_array().unwrap().len() >= 8);
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        assert!(matches!(&events[2], WizardEvent::Token { text } if text.contains("template")));
        assert!(matches!(&events[3], WizardEvent::Done { .. }));
    }

    #[tokio::test]
    async fn tell_me_what_strategies_i_have_lists_existing_strategy() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "tell me what strategies I have", ContextScope::Workspace)
                .await;
        wl.run_tool(
            "create_strategy",
            serde_json::json!({"template": "mean_reversion", "name": "Wizard Inventory"}),
        )
        .await
        .expect("create strategy");

        let out = wl
            .run_tool("list_strategies", serde_json::json!({}))
            .await
            .expect("list strategies");
        let rows = out.as_array().expect("list_strategies returns an array");
        assert!(
            rows.iter().any(|row| {
                row.get("display_name").and_then(|v| v.as_str()) == Some("Wizard Inventory")
            }),
            "expected created strategy in {out}"
        );
    }

    #[tokio::test]
    async fn run_eval_resolves_the_strategy_we_have_and_crypto_range_bound_to_fetch_bars_action() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, pool, td, _sid) = loop_with_session(
            mock,
            "run an eval on the strategy we have scenario crypto range bound",
            ContextScope::Workspace,
        )
        .await;
        seed_defaults(&pool, &td).await;
        let created = wl
            .run_tool(
                "create_strategy",
                serde_json::json!({"template": "range_trade", "name": "Only Strategy"}),
            )
            .await
            .expect("create strategy");
        let agent_id = created["id"].as_str().expect("created id");

        let out = wl
            .run_tool(
                "run_eval",
                serde_json::json!({
                    "agent_id": "the strategy we have",
                    "scenario_id": "crypto range bound",
                    "mode": "backtest"
                }),
            )
            .await
            .expect("run_eval should return an action instead of erroring");

        assert_eq!(out["agent_id"], agent_id);
        assert_eq!(out["scenario_id"], "crypto-rangebound-q2-2025");
        assert_eq!(out["ui_action"]["type"], "fetch_bars");
        assert_eq!(out["ui_action"]["scenario_id"], "crypto-rangebound-q2-2025");
    }

    #[tokio::test]
    async fn ambiguous_strategy_ask_returns_one_clarifying_question() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "run an eval on the strategy", ContextScope::Workspace).await;
        wl.run_tool(
            "create_strategy",
            serde_json::json!({"template": "mean_reversion", "name": "First Strategy"}),
        )
        .await
        .expect("create first strategy");
        wl.run_tool(
            "create_strategy",
            serde_json::json!({"template": "trend_follower", "name": "Second Strategy"}),
        )
        .await
        .expect("create second strategy");

        let out = wl
            .run_tool(
                "resolve_strategy",
                serde_json::json!({"query": "the strategy we have"}),
            )
            .await
            .expect("resolve_strategy should return clarification payload");
        let clarification = out
            .get("needs_clarification")
            .expect("expected needs_clarification");
        let question = clarification["question"].as_str().expect("question");
        let options = clarification["options"].as_array().expect("options");
        assert_eq!(
            question.matches('?').count(),
            1,
            "should ask exactly one question: {question}"
        );
        assert_eq!(options.len(), 2, "expected both strategies in {out}");
    }

    #[tokio::test]
    async fn missing_bars_returns_fetch_bars_ui_action() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, pool, td, _sid) =
            loop_with_session(mock, "run eval", ContextScope::Workspace).await;
        seed_defaults(&pool, &td).await;
        let created = wl
            .run_tool(
                "create_strategy",
                serde_json::json!({"template": "range_trade", "name": "Needs Bars"}),
            )
            .await
            .expect("create strategy");

        let out = wl
            .run_tool(
                "run_eval",
                serde_json::json!({
                    "agent_id": created["id"],
                    "scenario_id": "crypto-rangebound-q2-2025",
                    "mode": "backtest"
                }),
            )
            .await
            .expect("run_eval should return an action when bars are missing");

        assert_eq!(out["ui_action"]["type"], "fetch_bars");
        assert_eq!(out["ui_action"]["label"], "Fetch bars");
        assert_eq!(out["ui_action"]["argv"][0], "bars");
        assert_eq!(out["ui_action"]["argv"][1], "fetch");
        assert_eq!(
            out["ui_action"]["cache_key"],
            out["scenario"]["bar_cache_policy"]["cache_key"]
        );
    }

    #[tokio::test]
    async fn create_strategy_round_trip_tracks_draft_id() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "create_strategy",
                serde_json::json!({"template": "trend_follower", "name": "btc-tf-1"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Created. Want me to validate?".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "make me a trend follower", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        let done = events
            .iter()
            .rev()
            .find(|e| matches!(e, WizardEvent::Done { .. }));
        match done {
            Some(WizardEvent::Done { draft_id: Some(id) }) => {
                assert!(!id.is_empty());
            }
            other => panic!("expected Done with draft_id, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wizard_create_strategy_is_visible_via_public_strategy_list() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "create_strategy",
                serde_json::json!({"template": "mean_reversion", "name": "Wizard Visible"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Created the draft.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, pool, td, _sid) =
            loop_with_session(mock, "make me a strategy", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;

        let draft_id = events
            .iter()
            .rev()
            .find_map(|e| match e {
                WizardEvent::Done {
                    draft_id: Some(id),
                } => Some(id.clone()),
                _ => None,
            })
            .expect("wizard should emit a draft id");

        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "wizard-test".to_string(),
            },
            td.path().to_path_buf(),
        );
        let summaries = xvision_engine::api::strategy::list(&ctx)
            .await
            .expect("strategy list");
        let created = summaries
            .iter()
            .find(|item| item.agent_id == draft_id)
            .expect("created strategy must be visible via public list");
        assert_eq!(created.display_name, "Wizard Visible");
        assert_eq!(created.template, "mean_reversion");
    }

    #[tokio::test]
    async fn unknown_template_surfaces_as_tool_result_error() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "create_strategy",
                serde_json::json!({"template": "nope", "name": "x"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "That template doesn't exist.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "make me a nope", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        let result = events
            .iter()
            .find(|e| matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "create_strategy"));
        match result {
            Some(WizardEvent::ToolResult { result, .. }) => {
                assert!(result.get("error").is_some(), "expected error key in {result}");
            }
            other => panic!("expected ToolResult with error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_tool_name_surfaces_as_tool_result_error() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "fly_to_moon", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "ok stopping".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "go", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        let result = events
            .iter()
            .find(|e| matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "fly_to_moon"));
        match result {
            Some(WizardEvent::ToolResult { result, .. }) => {
                assert!(result.get("error").is_some());
            }
            other => panic!("expected ToolResult with error, got {other:?}"),
        }
    }

    #[test]
    fn wizard_tool_defs_advertises_core_verbs() {
        let defs = wizard_tool_defs();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names.len(), 12);
        for v in [
            "list_templates",
            "create_strategy",
            "get_strategy",
            "list_strategies",
            "list_scenarios",
            "get_scenario",
            "create_scenario",
            "update_slot",
            "set_mechanical_param",
            "set_risk_config",
            "validate_draft",
            "run_eval",
        ] {
            assert!(names.contains(&v), "missing verb {v} in {names:?}");
        }
    }

    // -- Plan #11 Phase B persistence assertions ----------------------------

    #[tokio::test]
    async fn user_message_persists_immediately_at_construction() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (mut _wl, pool, _td, sid) =
            loop_with_session(mock, "hi there", ContextScope::Workspace).await;
        let history = ChatSessionStore::load_history(&pool, &sid).await.unwrap();
        // After ::new and BEFORE any next_event call, the user message is
        // already in the store.
        assert_eq!(history.len(), 1, "history: {history:?}");
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content_blocks[0]["text"], "hi there");
    }

    #[tokio::test]
    async fn assistant_response_persists_after_drain() {
        let mock = Arc::new(MockDispatch::echo("got it"));
        let (mut wl, pool, _td, sid) =
            loop_with_session(mock, "hi", ContextScope::Workspace).await;
        drain(&mut wl).await;
        let history = ChatSessionStore::load_history(&pool, &sid).await.unwrap();
        // user "hi" + assistant "got it" + (stop_reason was EndTurn so no
        // tool_result turn).
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].role, "assistant");
        let text = history[1].content_blocks[0]["text"].as_str().unwrap();
        assert!(text.contains("got it"));
    }

    #[tokio::test]
    async fn tool_result_user_turn_persists_after_round_trip() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "list_templates", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "thanks".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, pool, _td, sid) =
            loop_with_session(mock, "list", ContextScope::Workspace).await;
        drain(&mut wl).await;
        let history = ChatSessionStore::load_history(&pool, &sid).await.unwrap();
        // Expected: user "list" → assistant tool_use → user tool_result →
        // assistant "thanks". 4 rows.
        assert_eq!(history.len(), 4, "history: {history:#?}");
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[2].role, "user");
        // The third row's content is a tool_result block.
        assert_eq!(
            history[2].content_blocks[0]["type"].as_str().unwrap(),
            "tool_result"
        );
        assert_eq!(history[3].role, "assistant");
    }

    #[tokio::test]
    async fn second_message_resumes_session_history() {
        // First turn establishes history.
        let mock1 = Arc::new(MockDispatch::echo("first reply"));
        let (mut wl1, pool, td, sid) =
            loop_with_session(mock1, "first", ContextScope::Workspace).await;
        drain(&mut wl1).await;

        // Second turn against the SAME session — the loop should see the
        // first turn in its message log via load_history.
        let mock2 = Arc::new(MockDispatch::echo("second reply"));
        let mut wl2 = WizardLoop::new(
            td.path().to_path_buf(),
            mock2,
            "claude-sonnet-4-6".into(),
            pool.clone(),
            sid.clone(),
            ContextScope::Workspace,
            "second".into(),
        )
        .await
        .unwrap();
        drain(&mut wl2).await;

        let history = ChatSessionStore::load_history(&pool, &sid).await.unwrap();
        // user1 + assistant1 + user2 + assistant2 = 4 rows.
        assert_eq!(history.len(), 4);
        assert_eq!(history[0].content_blocks[0]["text"], "first");
        assert_eq!(history[2].content_blocks[0]["text"], "second");
    }

    #[tokio::test]
    async fn scope_header_appears_in_system_prompt() {
        // Spy on the system prompt by capturing it via a dispatch that
        // records the request. We use a simple wrapper around MockDispatch.
        use std::sync::Mutex;
        struct Spy {
            inner: MockDispatch,
            seen: Arc<Mutex<Vec<String>>>,
        }
        #[async_trait::async_trait]
        impl LlmDispatch for Spy {
            async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
                self.seen.lock().unwrap().push(req.system_prompt.clone());
                self.inner.complete(req).await
            }
        }
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(Spy {
            inner: MockDispatch::echo("ok"),
            seen: seen.clone(),
        });
        let (mut wl, _pool, _td, _sid) = loop_with_session(
            dispatch,
            "what's this run?",
            ContextScope::Run {
                run_id: "01HABC".into(),
            },
        )
        .await;
        drain(&mut wl).await;
        let prompts = seen.lock().unwrap();
        assert!(!prompts.is_empty());
        assert!(
            prompts[0].contains("Run · 01HABC"),
            "system prompt did not include scope header: {}",
            prompts[0]
        );
    }
}
