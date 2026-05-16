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

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::cli_jobs::runner::CliJobRunner;
use crate::cli_jobs::store::CliJobStore;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason, ToolDefinition,
};
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::agents as api_agents;
use xvision_engine::api::eval::{self as api_eval, EvalRunRequest};
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::scenario::{CreateScenarioRequest, ListScenariosFilter};
use xvision_engine::api::strategy as api_strategy;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::authoring;
use xvision_engine::chat_session::{action_confirmation_card, ChatSessionStore, ContextScope, InlineAction};
use xvision_engine::eval::run::RunMode;
use xvision_engine::eval::scenario::Scenario;

const WIZARD_SYSTEM_PROMPT_BASE: &str = include_str!("../prompts/wizard.md");

/// Cap on tool-use → tool-result iterations per `next_event` call. Prevents
/// a misbehaving model from looping forever; v1 wizards never need more
/// than 3-4 round trips per user turn.
const MAX_TOOL_LOOP_ITERATIONS: usize = 12;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfile {
    Workspace,
    StrategySetup,
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self::Workspace
    }
}

impl AgentProfile {
    pub fn prompt_section(self) -> &'static str {
        match self {
            AgentProfile::Workspace => {
                "## Agent profile: workspace\n\
                 You are the xvision workspace assistant. Inspect existing strategies, scenarios, \
                 eval runs, and cached market data before recommending work. Prefer typed tools \
                 and queued xvn CLI jobs over asking the user to run commands."
            }
            AgentProfile::StrategySetup => {
                "## Agent profile: strategy setup\n\
                 You are focused on strategy setup: creating, editing, validating, and evaluating \
                 strategies. Use existing strategies and existing scenarios before falling back to \
                 templates or asking broad questions. When you create a strategy, ensure it has a \
                 trader agent with an explicit provider/model before claiming it is eval-ready. \
                 Do not say a tool change succeeded until the tool_result says it succeeded. For \
                 strategy tools, pass `id` or `strategy_id` as a top-level field, never nested under \
                 the tool name. Ask one targeted clarification only when the available strategies/scenarios \
                 are genuinely ambiguous."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WizardEvent {
    /// A chunk of assistant text. The SSE route streams these as they
    /// arrive; clients append to the visible bubble.
    Token { text: String },
    /// The agent is about to call an authoring verb. Front-end uses this
    /// to render a "running tool" indicator.
    ToolCall { tool: String, args: serde_json::Value },
    /// Result of the most-recent tool call. Front-end uses this to update
    /// the displayed draft state.
    ToolResult { tool: String, result: serde_json::Value },
    /// A typed rich display block to append to the active assistant bubble.
    ContentBlock { block: serde_json::Value },
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
    agent_provider: Option<String>,
    agent_model: Option<String>,
    pool: SqlitePool,
    session_id: String,
    scope: ContextScope,
    profile: AgentProfile,
    cli_runner: Option<Arc<CliJobRunner>>,
    /// Tracked across iterations: the most recent strategy id mentioned in
    /// a tool-call/-result. Used to populate `Done.draft_id`.
    last_draft_id: Option<String>,
    /// Pending events queued during the current `next_event` invocation.
    pending: Vec<WizardEvent>,
    is_done: bool,
}

#[derive(Debug, Deserialize)]
struct CreateStrategyAgentReq {
    #[serde(default)]
    strategy_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AttachAgentReq {
    #[serde(default)]
    strategy_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    agent_id: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Clone)]
enum StrategyResolution {
    Resolved(api_strategy::StrategySummary),
    NeedsClarification(serde_json::Value),
}

#[derive(Debug, Clone)]
enum ScenarioResolution {
    Resolved(Scenario),
    NeedsClarification(serde_json::Value),
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
        Self::new_with_profile(
            xvn_home,
            dispatch,
            model,
            None,
            None,
            pool,
            session_id,
            scope,
            AgentProfile::Workspace,
            None,
            new_message,
        )
        .await
    }

    pub async fn new_with_profile(
        xvn_home: PathBuf,
        dispatch: Arc<dyn LlmDispatch>,
        model: String,
        agent_provider: Option<String>,
        agent_model: Option<String>,
        pool: SqlitePool,
        session_id: String,
        scope: ContextScope,
        profile: AgentProfile,
        cli_runner: Option<Arc<CliJobRunner>>,
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
            agent_provider,
            agent_model,
            pool,
            session_id,
            scope,
            profile,
            cli_runner,
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
            "{base}\n\n## Current context\n{header}\n\n{runtime}\n",
            base = WIZARD_SYSTEM_PROMPT_BASE,
            header = format!(
                "{}\n\n{}",
                self.scope.header_label(),
                self.profile.prompt_section()
            ),
            runtime = self.agent_runtime_prompt_section(),
        )
    }

    fn agent_runtime_prompt_section(&self) -> String {
        match (&self.agent_provider, &self.agent_model) {
            (Some(provider), Some(model)) => format!(
                "## Selected strategy-agent runtime\n\
                 New strategy agents may use provider `{provider}` and model `{model}` unless the user asks for a different configured pair."
            ),
            _ => "## Selected strategy-agent runtime\nNo provider/model pair is selected for new strategy agents; ask for one before creating an eval-ready agent.".into(),
        }
    }

    async fn run_one_turn(&mut self) -> anyhow::Result<()> {
        for _ in 0..MAX_TOOL_LOOP_ITERATIONS {
            let messages = self.load_messages_from_store().await?;
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: self.system_prompt(),
                messages,
                max_tokens: 1500,
                tools: agent_tool_defs(self.profile),
                temperature: None,
                response_schema: None,
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
            ChatSessionStore::append(&self.pool, &self.session_id, "assistant", &assistant_blocks).await?;

            // Emit Token events for any text blocks the model produced.
            for block in &resp.content {
                if let ContentBlock::Text { text } = block {
                    if !text.is_empty() {
                        self.pending.push(WizardEvent::Token { text: text.clone() });
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
            let mut tool_result_blocks: Vec<serde_json::Value> = Vec::with_capacity(tool_uses.len());
            let mut rich_blocks: Vec<serde_json::Value> = Vec::new();
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
                if let Some(block) = rich_block_for_tool_result(&name, &result_value) {
                    rich_blocks.push(block);
                }
                tool_result_blocks.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": result_value.to_string(),
                }));
            }
            ChatSessionStore::append(&self.pool, &self.session_id, "user", &tool_result_blocks).await?;
            if !rich_blocks.is_empty() {
                ChatSessionStore::append(&self.pool, &self.session_id, "assistant", &rich_blocks).await?;
                for block in rich_blocks {
                    self.pending.push(WizardEvent::ContentBlock { block });
                }
            }

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
        let history = ChatSessionStore::load_history(&self.pool, &self.session_id).await?;
        let mut out = Vec::with_capacity(history.len());
        for cm in history {
            let mut blocks = Vec::with_capacity(cm.content_blocks.len());
            for v in cm.content_blocks {
                if let Ok(block) = serde_json::from_value(v) {
                    blocks.push(block);
                }
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
        if let Some(id) = result.get("strategy_id").and_then(|v| v.as_str()) {
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

    async fn run_tool(&self, name: &str, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        if !agent_tool_defs(self.profile).iter().any(|d| d.name == name) {
            anyhow::bail!("tool '{name}' is not available in {:?} profile", self.profile);
        }
        let input = normalize_tool_input(name, input);
        match name {
            "list_templates" => {
                let out = authoring::list_templates();
                Ok(serde_json::to_value(out)?)
            }
            "create_strategy" => {
                let req: authoring::CreateStrategyReq = serde_json::from_value(input)?;
                let out = api_strategy::create_strategy(&self.api_context, req).await?;
                let mut value = serde_json::to_value(&out)?;
                if let Some(agent) = self.create_default_strategy_agent(&out.id).await? {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("agent".into(), agent);
                    }
                }
                Ok(value)
            }
            "get_strategy" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_strategy: missing `id`"))?;
                let out = api_strategy::get(&self.api_context, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "list_strategies" => {
                let out = api_strategy::list(&self.api_context).await?;
                Ok(serde_json::to_value(out)?)
            }
            "list_scenarios" => {
                let filter = ListScenariosFilter {
                    include_archived: input
                        .get("include_archived")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    ..Default::default()
                };
                let out = api_scenario::list(&self.api_context, filter).await?;
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
                let out = api_strategy::update_slot(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "update_manifest" => {
                let req: authoring::UpdateManifestReq = serde_json::from_value(input)?;
                let out = api_strategy::update_manifest(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "set_mechanical_param" => {
                let req: authoring::SetMechanicalParamReq = serde_json::from_value(input)?;
                let out = api_strategy::set_mechanical_param(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "set_risk_config" => {
                let req: authoring::SetRiskConfigReq = serde_json::from_value(input)?;
                let out = api_strategy::set_risk_config(&self.api_context, req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "create_strategy_agent" => {
                let req: CreateStrategyAgentReq = serde_json::from_value(input)?;
                let out = self.create_and_attach_strategy_agent(req).await?;
                Ok(out)
            }
            "attach_agent" => {
                let req: AttachAgentReq = serde_json::from_value(input)?;
                let out = self.attach_existing_agent(req).await?;
                Ok(serde_json::to_value(out)?)
            }
            "resolve_strategy" => {
                let query = input
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("resolve_strategy: missing `query`"))?;
                match self.resolve_strategy_query(query).await? {
                    StrategyResolution::Resolved(strategy) => Ok(strategy_resolution_json(&strategy)),
                    StrategyResolution::NeedsClarification(payload) => Ok(payload),
                }
            }
            "validate_draft" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("validate_draft: missing `id`"))?;
                let out = api_strategy::validate_draft(&self.api_context, id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "run_eval" => {
                let agent_query = input
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("run_eval: missing `agent_id`"))?;
                let scenario_query = input
                    .get("scenario_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("run_eval: missing `scenario_id`"))?;
                let strategy = match self.resolve_strategy_query(agent_query).await? {
                    StrategyResolution::Resolved(strategy) => strategy,
                    StrategyResolution::NeedsClarification(payload) => return Ok(payload),
                };
                let scenario = match self.resolve_scenario_query(scenario_query).await? {
                    ScenarioResolution::Resolved(scenario) => scenario,
                    ScenarioResolution::NeedsClarification(payload) => return Ok(payload),
                };
                let mode = input.get("mode").and_then(|v| v.as_str()).unwrap_or("backtest");
                let mode = match mode {
                    "paper" => RunMode::Paper,
                    _ => RunMode::Backtest,
                };
                if mode == RunMode::Backtest && scenario_needs_bars(&scenario) {
                    return Ok(serde_json::json!({
                        "agent_id": strategy.agent_id.clone(),
                        "scenario_id": scenario.id.clone(),
                        "scenario": scenario.clone(),
                        "ui_action": fetch_bars_ui_action(&scenario)
                    }));
                }
                let req = EvalRunRequest {
                    agent_id: strategy.agent_id,
                    scenario_id: scenario.id,
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
            "fetch_bars" => {
                let asset = input
                    .get("asset")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("fetch_bars: missing `asset`"))?;
                let from = input
                    .get("from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("fetch_bars: missing `from`"))?;
                let to = input
                    .get("to")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("fetch_bars: missing `to`"))?;
                let granularity = input.get("granularity").and_then(|v| v.as_str()).unwrap_or("1h");
                if !matches!(granularity, "1h" | "1d") {
                    anyhow::bail!("fetch_bars: granularity must be `1h` or `1d`");
                }
                let argv = vec![
                    "bars".to_string(),
                    "fetch".to_string(),
                    "--asset".to_string(),
                    asset.to_string(),
                    "--from".to_string(),
                    from.to_string(),
                    "--to".to_string(),
                    to.to_string(),
                    "--granularity".to_string(),
                    granularity.to_string(),
                ];
                let store = CliJobStore::new(self.pool.clone());
                let job = store.create_queued(argv, 300).await?;
                if let Some(runner) = &self.cli_runner {
                    runner.start(job.clone());
                }
                Ok(serde_json::json!({
                    "job_id": job.job_id,
                    "status": job.status.as_str(),
                    "argv": job.argv
                }))
            }
            "get_cli_job" => {
                let job_id = input
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_cli_job: missing `job_id`"))?;
                let store = CliJobStore::new(self.pool.clone());
                let job = store
                    .get(job_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?;
                Ok(serde_json::json!({
                    "job_id": job.job_id,
                    "argv": job.argv,
                    "status": job.status.as_str(),
                    "created_at": job.created_at,
                    "started_at": job.started_at,
                    "finished_at": job.finished_at,
                    "exit_code": job.exit_code,
                    "timed_out": job.timed_out,
                    "cancel_requested": job.cancel_requested,
                    "stdout_bytes": job.stdout_bytes,
                    "stderr_bytes": job.stderr_bytes,
                    "stdout_truncated": job.stdout_truncated,
                    "stderr_truncated": job.stderr_truncated,
                    "error_message": job.error_message
                }))
            }
            "get_cli_job_output" => {
                let job_id = input
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_cli_job_output: missing `job_id`"))?;
                let store = CliJobStore::new(self.pool.clone());
                let output = store
                    .output(job_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?;
                Ok(serde_json::json!({
                    "job_id": output.job_id,
                    "status": output.status.as_str(),
                    "exit_code": output.exit_code,
                    "stdout": output.stdout,
                    "stderr": output.stderr,
                    "stdout_bytes": output.stdout_bytes,
                    "stderr_bytes": output.stderr_bytes,
                    "stdout_truncated": output.stdout_truncated,
                    "stderr_truncated": output.stderr_truncated
                }))
            }
            other => anyhow::bail!("unknown authoring verb: {other}"),
        }
    }

    async fn resolve_strategy_query(&self, query: &str) -> anyhow::Result<StrategyResolution> {
        let query = query.trim();
        let strategies = api_strategy::list(&self.api_context).await?;
        if strategies.is_empty() {
            anyhow::bail!("no strategies exist yet; create a strategy before running eval");
        }

        if is_generic_strategy_query(query) {
            return if strategies.len() == 1 {
                Ok(StrategyResolution::Resolved(strategies[0].clone()))
            } else {
                Ok(StrategyResolution::NeedsClarification(
                    strategy_clarification_payload(&strategies),
                ))
            };
        }

        let matches: Vec<_> = strategies
            .iter()
            .filter(|strategy| strategy_matches_query(strategy, query))
            .cloned()
            .collect();
        match matches.len() {
            1 => Ok(StrategyResolution::Resolved(matches[0].clone())),
            n if n > 1 => Ok(StrategyResolution::NeedsClarification(
                strategy_clarification_payload(&matches),
            )),
            _ if strategies.len() == 1 => Ok(StrategyResolution::Resolved(strategies[0].clone())),
            _ => anyhow::bail!("strategy '{query}' was not found"),
        }
    }

    async fn resolve_scenario_query(&self, query: &str) -> anyhow::Result<ScenarioResolution> {
        let query = query.trim();
        let scenarios = self.known_scenarios().await?;
        if scenarios.is_empty() {
            anyhow::bail!("no scenarios exist yet; create or seed a scenario before running eval");
        }

        let matches: Vec<_> = scenarios
            .iter()
            .filter(|scenario| scenario_matches_query(scenario, query))
            .cloned()
            .collect();
        match matches.len() {
            1 => Ok(ScenarioResolution::Resolved(matches[0].clone())),
            n if n > 1 => Ok(ScenarioResolution::NeedsClarification(
                scenario_clarification_payload(&matches),
            )),
            _ => anyhow::bail!("scenario '{query}' was not found"),
        }
    }

    async fn known_scenarios(&self) -> anyhow::Result<Vec<Scenario>> {
        let mut scenarios = api_scenario::list(
            &self.api_context,
            ListScenariosFilter {
                source: None,
                tags: vec![],
                include_archived: false,
                parent_scenario_id: None,
            },
        )
        .await?;
        for seeded in xvision_engine::eval::scenario_seed::canonical_seed_rows() {
            if !scenarios.iter().any(|scenario| scenario.id == seeded.id) {
                scenarios.push(seeded);
            }
        }
        Ok(scenarios)
    }

    async fn create_default_strategy_agent(
        &self,
        strategy_id: &str,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        if self.agent_provider.is_none() || self.agent_model.is_none() {
            return Ok(None);
        }
        let out = self
            .create_and_attach_strategy_agent(CreateStrategyAgentReq {
                strategy_id: Some(strategy_id.to_string()),
                id: None,
                role: Some("trader".into()),
                name: None,
                provider: None,
                model: None,
                system_prompt: None,
            })
            .await?;
        Ok(Some(out))
    }

    async fn create_and_attach_strategy_agent(
        &self,
        req: CreateStrategyAgentReq,
    ) -> anyhow::Result<serde_json::Value> {
        let strategy_id = req
            .strategy_id
            .or(req.id)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("create_strategy_agent: missing `strategy_id`"))?;
        let role = req.role.unwrap_or_else(|| "trader".into()).trim().to_string();
        if role.is_empty() {
            anyhow::bail!("create_strategy_agent: role is required");
        }
        let strategy = api_strategy::get(&self.api_context, &strategy_id).await?;
        if let Some(existing_ref) = strategy.agents.iter().find(|agent| agent.role == role) {
            let existing_agent_id = existing_ref.agent_id.clone();
            let agents = strategy.agents.clone();
            let pipeline = strategy.pipeline.clone();
            let agent = api_agents::get(&self.api_context, &existing_agent_id).await?;
            let (provider, model) = agent
                .slots
                .first()
                .map(|slot| (slot.provider.clone(), slot.model.clone()))
                .unwrap_or_else(|| ("".into(), "".into()));
            return Ok(serde_json::json!({
                "strategy_id": strategy_id,
                "agent_id": existing_agent_id,
                "role": role,
                "provider": provider,
                "model": model,
                "agents": agents,
                "pipeline": pipeline,
                "already_attached": true
            }));
        }
        let (provider, model) = self.resolve_agent_runtime(req.provider, req.model)?;
        let name = req
            .name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let suffix = strategy_id
                    .chars()
                    .rev()
                    .take(6)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                format!("{} {role} agent {suffix}", strategy.manifest.display_name)
            });
        let system_prompt = req
            .system_prompt
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| prompt_for_strategy_role(&strategy, &role))
            .unwrap_or_default();
        let agent = api_agents::create(
            &self.api_context,
            api_agents::CreateAgentRequest {
                name,
                description: format!(
                    "Auto-created for strategy {} as role `{role}`.",
                    strategy.manifest.id
                ),
                tags: vec!["strategy-agent".into(), role.clone()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: provider.clone(),
                    model: model.clone(),
                    system_prompt,
                    skill_ids: vec![],
                    // Default to "auto from model"; the dispatcher
                    // resolves this from the model's metadata at
                    // request time (q15 §1).
                    max_tokens: None,
                }],
            },
        )
        .await?;
        let attached = api_strategy::add_agent(
            &self.api_context,
            api_strategy::AddAgentReq {
                strategy_id: strategy_id.clone(),
                agent_id: agent.agent_id.clone(),
                role: role.clone(),
            },
        )
        .await?;
        Ok(serde_json::json!({
            "strategy_id": strategy_id,
            "agent_id": agent.agent_id,
            "role": role,
            "provider": provider,
            "model": model,
            "agents": attached.agents,
            "pipeline": attached.pipeline
        }))
    }

    async fn attach_existing_agent(
        &self,
        req: AttachAgentReq,
    ) -> anyhow::Result<xvision_engine::api::strategy::StrategyAgentsOut> {
        let strategy_id = req
            .strategy_id
            .or(req.id)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("attach_agent: missing `strategy_id`"))?;
        let role = req.role.unwrap_or_else(|| "trader".into()).trim().to_string();
        if role.is_empty() {
            anyhow::bail!("attach_agent: role is required");
        }
        let out = api_strategy::add_agent(
            &self.api_context,
            api_strategy::AddAgentReq {
                strategy_id,
                agent_id: req.agent_id,
                role,
            },
        )
        .await?;
        Ok(out)
    }

    fn resolve_agent_runtime(
        &self,
        provider: Option<String>,
        model: Option<String>,
    ) -> anyhow::Result<(String, String)> {
        let provider = provider
            .or_else(|| self.agent_provider.clone())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("create_strategy_agent: missing provider; pick a provider/model in the chat model picker or pass provider explicitly"))?;
        let model = model
            .or_else(|| self.agent_model.clone())
            .or_else(|| Some(self.model.clone()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("create_strategy_agent: missing model; pick a provider/model in the chat model picker or pass model explicitly"))?;
        Ok((provider, model))
    }
}

fn prompt_for_strategy_role(strategy: &xvision_engine::strategies::Strategy, role: &str) -> Option<String> {
    let role = role.trim().to_ascii_lowercase();
    if role == "trader" {
        return strategy.trader_slot.as_ref().map(|slot| slot.prompt.clone());
    }
    if role == "intern" {
        return strategy.intern_slot.as_ref().map(|slot| slot.prompt.clone());
    }
    if role == "regime" {
        return strategy.regime_slot.as_ref().map(|slot| slot.prompt.clone());
    }
    None
}

fn strategy_resolution_json(strategy: &api_strategy::StrategySummary) -> serde_json::Value {
    serde_json::json!({
        "agent_id": &strategy.agent_id,
        "display_name": &strategy.display_name,
        "template": &strategy.template,
        "providers": &strategy.providers,
        "models": &strategy.models,
        "provider_models": &strategy.provider_models
    })
}

fn strategy_clarification_payload(strategies: &[api_strategy::StrategySummary]) -> serde_json::Value {
    let options: Vec<_> = strategies
        .iter()
        .map(|strategy| {
            serde_json::json!({
                "agent_id": &strategy.agent_id,
                "display_name": &strategy.display_name,
                "template": &strategy.template
            })
        })
        .collect();
    serde_json::json!({
        "needs_clarification": {
            "question": "Which strategy do you want to use?",
            "options": options
        }
    })
}

fn scenario_clarification_payload(scenarios: &[Scenario]) -> serde_json::Value {
    let options: Vec<_> = scenarios
        .iter()
        .map(|scenario| {
            serde_json::json!({
                "scenario_id": &scenario.id,
                "display_name": &scenario.display_name
            })
        })
        .collect();
    serde_json::json!({
        "needs_clarification": {
            "question": "Which scenario do you want to use?",
            "options": options
        }
    })
}

fn is_generic_strategy_query(query: &str) -> bool {
    matches!(
        search_key(query).as_str(),
        "" | "strategy" | "the strategy" | "the strategy we have" | "strategy we have"
    )
}

fn strategy_matches_query(strategy: &api_strategy::StrategySummary, query: &str) -> bool {
    if strategy.agent_id == query {
        return true;
    }
    let haystack = search_key(&format!(
        "{} {} {} {}",
        strategy.agent_id,
        strategy.display_name,
        strategy.template,
        strategy.tags.join(" ")
    ));
    query_tokens_match(query, &haystack)
}

fn scenario_matches_query(scenario: &Scenario, query: &str) -> bool {
    if scenario.id == query {
        return true;
    }
    let haystack = search_key(&format!(
        "{} {} {}",
        scenario.id,
        scenario.display_name,
        scenario.tags.join(" ")
    ));
    query_tokens_match(query, &haystack)
}

fn query_tokens_match(query: &str, haystack: &str) -> bool {
    let query_key = search_key(query);
    !query_key.is_empty() && query_key.split_whitespace().all(|token| haystack.contains(token))
}

fn search_key(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn scenario_needs_bars(scenario: &Scenario) -> bool {
    scenario.bar_cache_policy.data_fetched_at.is_none() && !legacy_fixture_exists(scenario)
}

fn legacy_fixture_exists(scenario: &Scenario) -> bool {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("probes")
        .join(format!("{}.parquet", scenario.bar_cache_policy.cache_key))
        .exists()
}

fn fetch_bars_ui_action(scenario: &Scenario) -> serde_json::Value {
    let asset = scenario
        .asset
        .first()
        .map(|asset| asset.symbol.clone())
        .unwrap_or_else(|| "BTC".into());
    serde_json::json!({
        "type": "fetch_bars",
        "label": "Fetch bars",
        "scenario_id": &scenario.id,
        "cache_key": &scenario.bar_cache_policy.cache_key,
        "argv": [
            "bars",
            "fetch",
            "--asset",
            asset,
            "--from",
            scenario.time_window.start.date_naive().to_string(),
            "--to",
            scenario.time_window.end.date_naive().to_string(),
            "--granularity",
            scenario.granularity.to_string()
        ]
    })
}

fn normalize_tool_input(tool: &str, input: serde_json::Value) -> serde_json::Value {
    let mut input = match input {
        serde_json::Value::Object(mut obj) => {
            if let Some(nested) = obj.remove(tool).filter(|v| v.is_object()) {
                nested
            } else if let Some(nested) = obj.remove("input").filter(|v| v.is_object()) {
                nested
            } else {
                serde_json::Value::Object(obj)
            }
        }
        other => other,
    };

    if let serde_json::Value::Object(obj) = &mut input {
        if expects_id(tool) && !obj.contains_key("id") {
            if let Some(strategy_id) = obj.get("strategy_id").cloned() {
                obj.insert("id".into(), strategy_id);
            }
        }
        if matches!(tool, "create_strategy_agent" | "attach_agent") && !obj.contains_key("strategy_id") {
            if let Some(id) = obj.get("id").cloned() {
                obj.insert("strategy_id".into(), id);
            }
        }
        if tool == "create_scenario" {
            normalize_create_scenario_input(obj);
        }
    }
    input
}

fn normalize_create_scenario_input(obj: &mut serde_json::Map<String, serde_json::Value>) {
    if let Some(name) = string_field(obj, "name").filter(|_| !obj.contains_key("display_name")) {
        obj.insert("display_name".into(), serde_json::Value::String(name));
    }
    let display_name = string_field(obj, "display_name").unwrap_or_else(|| "Chat scenario".into());

    if missing_string(obj, "description") {
        obj.insert(
            "description".into(),
            serde_json::Value::String(format!("{display_name} scenario generated from chat.")),
        );
    }
    ensure_string(obj, "asset_class", "Crypto");
    normalize_enum_string(obj, "asset_class", &[("crypto", "Crypto")]);
    ensure_string(obj, "quote_currency", "Usd");
    normalize_enum_string(
        obj,
        "quote_currency",
        &[("usd", "Usd"), ("usdt", "Usdt"), ("usdc", "Usdc")],
    );
    ensure_string(obj, "granularity", "4h");
    ensure_string(obj, "timezone", "UTC");
    ensure_array(obj, "tags");
    ensure_null(obj, "notes");
    ensure_null(obj, "parent_scenario_id");
    ensure_string(obj, "source", "Generated");
    normalize_enum_string(
        obj,
        "source",
        &[
            ("generated", "Generated"),
            ("user", "User"),
            ("canonical", "Canonical"),
            ("clone", "Clone"),
        ],
    );

    match obj.get_mut("asset") {
        Some(serde_json::Value::Array(assets)) if !assets.is_empty() => {
            for asset in assets {
                normalize_asset_ref(asset);
            }
        }
        Some(asset @ serde_json::Value::Object(_)) => {
            normalize_asset_ref(asset);
            let normalized = asset.clone();
            *asset = serde_json::Value::Array(vec![normalized]);
        }
        _ => {
            let symbol = string_field(obj, "asset")
                .or_else(|| string_field(obj, "symbol"))
                .or_else(|| infer_asset_symbol(&display_name))
                .unwrap_or_else(|| "BTC".into());
            obj.insert("asset".into(), serde_json::json!([asset_ref_json(&symbol)]));
        }
    }

    if !obj.get("time_window").is_some_and(|v| v.is_object()) {
        if let Some(window) = infer_time_window(&display_name) {
            obj.insert("time_window".into(), window);
        }
    }
    obj.entry("capital")
        .or_insert_with(|| serde_json::json!({"initial": 100000.0, "currency": "USD"}));
    obj.entry("calendar")
        .or_insert_with(|| serde_json::json!("Continuous24x7"));
    obj.entry("venue").or_insert_with(default_venue_json);
    obj.entry("data_source").or_insert_with(
        || serde_json::json!({"type": "AlpacaHistorical", "feed": null, "adjustment": "Raw"}),
    );
    obj.entry("replay_mode")
        .or_insert_with(|| serde_json::json!({"mode": "Continuous"}));

    if matches!(obj.get("replay_mode"), Some(serde_json::Value::String(_))) {
        obj.insert("replay_mode".into(), serde_json::json!({"mode": "Continuous"}));
    }
}

fn normalize_asset_ref(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(symbol) => {
            *value = asset_ref_json(symbol);
        }
        serde_json::Value::Object(obj) => {
            if missing_string(obj, "symbol") {
                obj.insert("symbol".into(), serde_json::Value::String("BTC".into()));
            }
            if missing_string(obj, "venue_symbol") {
                let symbol = string_field(obj, "symbol").unwrap_or_else(|| "BTC".into());
                obj.insert(
                    "venue_symbol".into(),
                    serde_json::Value::String(venue_symbol(&symbol)),
                );
            }
            if missing_string(obj, "class") {
                obj.insert("class".into(), serde_json::Value::String("Crypto".into()));
            }
            normalize_enum_string(obj, "class", &[("crypto", "Crypto")]);
        }
        _ => {
            *value = asset_ref_json("BTC");
        }
    }
}

fn asset_ref_json(symbol: &str) -> serde_json::Value {
    let base = base_symbol(symbol);
    serde_json::json!({
        "class": "Crypto",
        "symbol": base,
        "venue_symbol": venue_symbol(&base)
    })
}

fn venue_symbol(symbol: &str) -> String {
    let base = base_symbol(symbol);
    if symbol.contains('/') {
        symbol.to_ascii_uppercase()
    } else {
        format!("{base}/USD")
    }
}

fn base_symbol(symbol: &str) -> String {
    symbol
        .trim()
        .split('/')
        .next()
        .unwrap_or("BTC")
        .trim()
        .to_ascii_uppercase()
}

fn infer_asset_symbol(display_name: &str) -> Option<String> {
    let lower = display_name.to_ascii_lowercase();
    if lower.contains("solana") || lower.split_whitespace().any(|part| part == "sol") {
        return Some("SOL".into());
    }
    if lower.contains("bitcoin") || lower.split_whitespace().any(|part| part == "btc") {
        return Some("BTC".into());
    }
    None
}

fn infer_time_window(display_name: &str) -> Option<serde_json::Value> {
    let lower = display_name.to_ascii_lowercase();
    let quarter = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find_map(|part| part.strip_prefix('q')?.parse::<u32>().ok())
        .filter(|q| (1..=4).contains(q))?;
    let year = lower
        .split(|c: char| !c.is_ascii_digit())
        .find_map(|part| (part.len() == 4).then(|| part.parse::<i32>().ok()).flatten())?;
    let month = (quarter - 1) * 3 + 1;
    let start = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single()?;
    let end_month = if quarter == 4 { 1 } else { month + 3 };
    let end_year = if quarter == 4 { year + 1 } else { year };
    let end = Utc.with_ymd_and_hms(end_year, end_month, 1, 0, 0, 0).single()?;
    Some(serde_json::json!({
        "start": start.to_rfc3339(),
        "end": end.to_rfc3339()
    }))
}

fn default_venue_json() -> serde_json::Value {
    serde_json::json!({
        "venue": "Alpaca",
        "fees": {"maker_bps": 0, "taker_bps": 10},
        "slippage": {"model": "linear", "bps": 2},
        "latency": {"decision_to_fill_ms": 500},
        "fill_model": {
            "market_order_fill": "NextBarOpen",
            "limit_order_fill": "NeverFills",
            "partial_fills": false,
            "volume_constraints": null
        }
    })
}

fn ensure_string(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str, default: &str) {
    if missing_string(obj, key) {
        obj.insert(key.into(), serde_json::Value::String(default.into()));
    }
}

fn ensure_array(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    if !obj.get(key).is_some_and(|v| v.is_array()) {
        obj.insert(key.into(), serde_json::Value::Array(vec![]));
    }
}

fn ensure_null(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    obj.entry(key).or_insert(serde_json::Value::Null);
}

fn missing_string(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .is_none()
}

fn string_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn normalize_enum_string(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    aliases: &[(&str, &str)],
) {
    let Some(value) = string_field(obj, key) else {
        return;
    };
    let lower = value.to_ascii_lowercase();
    if let Some((_, canonical)) = aliases.iter().find(|(alias, _)| *alias == lower) {
        obj.insert(key.into(), serde_json::Value::String((*canonical).into()));
    }
}

fn expects_id(tool: &str) -> bool {
    matches!(
        tool,
        "get_strategy"
            | "update_slot"
            | "update_manifest"
            | "set_mechanical_param"
            | "set_risk_config"
            | "validate_draft"
    )
}

fn rich_block_for_tool_result(tool: &str, result: &serde_json::Value) -> Option<serde_json::Value> {
    if result.get("error").is_some() {
        return None;
    }
    let card = match tool {
        "create_strategy" => {
            let id = result.get("id")?.as_str()?;
            action_confirmation_card(
                format!("strategy-created:{id}"),
                "Strategy draft created",
                format!("Draft {id} is ready for inspection."),
                InlineAction {
                    label: "Open draft".into(),
                    href: Some(format!("/authoring/{id}")),
                    command: None,
                },
            )
            .ok()?
        }
        "run_eval" => {
            let id = result.get("run_id")?.as_str()?;
            action_confirmation_card(
                format!("eval-started:{id}"),
                "Eval run started",
                format!("Run {id} has been queued."),
                InlineAction {
                    label: "Open run".into(),
                    href: Some(format!("/eval-runs/{id}")),
                    command: None,
                },
            )
            .ok()?
        }
        "fetch_bars" => {
            let id = result.get("job_id")?.as_str()?;
            action_confirmation_card(
                format!("bars-fetch:{id}"),
                "Bar fetch queued",
                format!("CLI job {id} is warming the local bar cache."),
                InlineAction {
                    label: "Open eval runs".into(),
                    href: Some("/eval-runs".into()),
                    command: None,
                },
            )
            .ok()?
        }
        _ => return None,
    };
    serde_json::to_value(card).ok()
}

pub type AgentChatLoop = WizardLoop;

/// Authoring/eval verbs as `ToolDefinition`s. The schemas mirror the
/// engine's request structs but only declare the fields a model needs;
/// optional fields are omitted from `required`.
fn wizard_tool_defs() -> Vec<ToolDefinition> {
    agent_tool_defs(AgentProfile::StrategySetup)
}

pub(crate) fn agent_tool_defs(profile: AgentProfile) -> Vec<ToolDefinition> {
    let mut defs = strategy_tool_defs();
    match profile {
        AgentProfile::StrategySetup => {}
        AgentProfile::Workspace => {
            defs.extend(workspace_tool_defs());
        }
    }
    defs
}

fn strategy_tool_defs() -> Vec<ToolDefinition> {
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
            name: "create_scenario".into(),
            description: "Create a scenario. Use list_scenarios first when the user asks for an existing scenario; only create when they request a new one and provide the required fields.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "display_name": {"type": "string", "minLength": 1},
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
            name: "update_manifest".into(),
            description: "Persist manifest fields shown in the Strategy Inspector, including asset universe and decision cadence."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "asset_universe": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1
                    },
                    "decision_cadence_minutes": {"type": "integer", "minimum": 1}
                },
                "required": ["id"]
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
            name: "create_strategy_agent".into(),
            description: "Create a reusable Agent with an explicit provider/model and attach it to a strategy. Use role `trader` for eval-ready single-agent strategies. If provider/model are omitted, the currently selected chat provider/model is used.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy_id": {"type": "string"},
                    "id": {"type": "string", "description": "Alias for strategy_id"},
                    "role": {"type": "string", "default": "trader"},
                    "name": {"type": "string"},
                    "provider": {"type": "string"},
                    "model": {"type": "string"},
                    "system_prompt": {"type": "string"}
                },
                "required": ["strategy_id"]
            }),
        },
        ToolDefinition {
            name: "attach_agent".into(),
            description: "Attach an existing Agent to a strategy as a role. Use role `trader` for eval-ready single-agent strategies.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy_id": {"type": "string"},
                    "id": {"type": "string", "description": "Alias for strategy_id"},
                    "agent_id": {"type": "string"},
                    "role": {"type": "string", "default": "trader"}
                },
                "required": ["strategy_id", "agent_id"]
            }),
        },
        ToolDefinition {
            name: "resolve_strategy".into(),
            description: "Resolve a user phrase like `the strategy we have` to one strategy id, or return a single clarification question when ambiguous.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
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
        ToolDefinition {
            name: "list_strategies".into(),
            description: "List persisted strategies before creating a new one.".into(),
            input_schema: serde_json::json!({
                "type": "object", "properties": {}, "required": []
            }),
        },
        ToolDefinition {
            name: "list_scenarios".into(),
            description: "List persisted scenarios. Use this before asking which scenario to eval against.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "include_archived": {"type": "boolean"}
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_scenario".into(),
            description: "Read one persisted scenario by id.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
    ]
}

fn workspace_tool_defs() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "fetch_bars".into(),
            description: "Queue an allowlisted `xvn bars fetch` CLI job to warm the local bar cache.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "asset": {"type": "string", "description": "BTC, ETH, SOL, etc."},
                    "from": {"type": "string", "description": "UTC date YYYY-MM-DD"},
                    "to": {"type": "string", "description": "UTC date YYYY-MM-DD"},
                    "granularity": {"type": "string", "enum": ["1h", "1d"]}
                },
                "required": ["asset", "from", "to"]
            }),
        },
        ToolDefinition {
            name: "get_cli_job".into(),
            description: "Inspect status and metadata for a queued xvn CLI job.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"job_id": {"type": "string"}},
                "required": ["job_id"]
            }),
        },
        ToolDefinition {
            name: "get_cli_job_output".into(),
            description: "Read captured stdout/stderr for a queued xvn CLI job.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"job_id": {"type": "string"}},
                "required": ["job_id"]
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
            loop_with_session(mock, "tell me what strategies I have", ContextScope::Workspace).await;
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
            rows.iter()
                .any(|row| { row.get("display_name").and_then(|v| v.as_str()) == Some("Wizard Inventory") }),
            "expected created strategy in {out}"
        );
    }

    #[tokio::test]
    async fn wizard_update_manifest_tool_persists_inspector_manifest_fields() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "make this BTC 6h", ContextScope::Workspace).await;
        let created = wl
            .run_tool(
                "create_strategy",
                serde_json::json!({"template": "mean_reversion", "name": "Bollinger Bands 6H"}),
            )
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        wl.run_tool(
            "update_manifest",
            serde_json::json!({
                "id": id,
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 360
            }),
        )
        .await
        .expect("update manifest");

        let strategy = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert_eq!(strategy["manifest"]["asset_universe"][0], "BTC/USD");
        assert_eq!(strategy["manifest"]["decision_cadence_minutes"], 360);
    }

    #[tokio::test]
    async fn wizard_update_manifest_accepts_nested_tool_input_and_strategy_id_alias() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "make this BTC 4h", ContextScope::Workspace).await;
        let created = wl
            .run_tool(
                "create_strategy",
                serde_json::json!({"template": "mean_reversion", "name": "Nested Input"}),
            )
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        wl.run_tool(
            "update_manifest",
            serde_json::json!({
                "update_manifest": {
                    "strategy_id": id,
                    "asset_universe": ["BTC/USD"],
                    "decision_cadence_minutes": 240
                }
            }),
        )
        .await
        .expect("update manifest");

        let strategy = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert_eq!(strategy["manifest"]["asset_universe"][0], "BTC/USD");
        assert_eq!(strategy["manifest"]["decision_cadence_minutes"], 240);
    }

    #[tokio::test]
    async fn create_scenario_recovers_missing_description_and_sol_q1_shape() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(
            mock,
            "try a paper eval on solana in q1 2026",
            ContextScope::Workspace,
        )
        .await;

        let out = wl
            .run_tool(
                "create_scenario",
                serde_json::json!({
                    "create_scenario": {
                        "display_name": "SOL Q1 2026",
                        "asset": "SOL",
                        "granularity": "4h"
                    }
                }),
            )
            .await
            .expect("create scenario should repair missing description and defaults");

        assert_eq!(out["display_name"], "SOL Q1 2026");
        assert_eq!(out["description"], "SOL Q1 2026 scenario generated from chat.");
        assert_eq!(out["asset"][0]["symbol"], "SOL");
        assert_eq!(out["asset"][0]["venue_symbol"], "SOL/USD");
        assert_eq!(out["time_window"]["start"], "2026-01-01T00:00:00Z");
        assert_eq!(out["time_window"]["end"], "2026-04-01T00:00:00Z");
    }

    #[tokio::test]
    async fn create_strategy_agent_tool_creates_and_attaches_trader_agent() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "attach agent", ContextScope::Workspace).await;
        let created = wl
            .run_tool(
                "create_strategy",
                serde_json::json!({"template": "ma_crossover_baseline", "name": "MA Agent"}),
            )
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        let out = wl
            .run_tool(
                "create_strategy_agent",
                serde_json::json!({
                    "strategy_id": id,
                    "role": "trader",
                    "provider": "openai",
                    "model": "gpt-4.1-mini"
                }),
            )
            .await
            .expect("create strategy agent");

        assert_eq!(out["strategy_id"], id);
        assert_eq!(out["role"], "trader");
        assert_eq!(out["provider"], "openai");
        assert_eq!(out["model"], "gpt-4.1-mini");
        assert_eq!(out["agents"][0]["role"], "trader");
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
        let (wl, pool, td, _sid) = loop_with_session(mock, "run eval", ContextScope::Workspace).await;
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
                WizardEvent::Done { draft_id: Some(id) } => Some(id.clone()),
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
        let (mut wl, _pool, _td, _sid) = loop_with_session(mock, "go", ContextScope::Workspace).await;
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

    #[tokio::test]
    async fn workspace_fetch_bars_tool_queues_cli_job() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "fetch_bars",
                serde_json::json!({
                    "asset": "BTC",
                    "from": "2025-04-01",
                    "to": "2025-06-30",
                    "granularity": "1h"
                }),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Queued the bars fetch.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, pool, _td, _sid) = loop_with_session(mock, "fetch bars", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        let job_id = events
            .iter()
            .find_map(|e| match e {
                WizardEvent::ToolResult { tool, result } if tool == "fetch_bars" => {
                    result.get("job_id").and_then(|v| v.as_str()).map(str::to_string)
                }
                _ => None,
            })
            .expect("fetch_bars should return a job id");
        let store = CliJobStore::new(pool);
        let job = store.get(&job_id).await.unwrap().expect("queued job");
        assert_eq!(
            job.argv,
            vec![
                "bars",
                "fetch",
                "--asset",
                "BTC",
                "--from",
                "2025-04-01",
                "--to",
                "2025-06-30",
                "--granularity",
                "1h"
            ]
        );
    }

    #[test]
    fn wizard_tool_defs_advertises_core_verbs() {
        let defs = wizard_tool_defs();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        for v in [
            "list_templates",
            "create_strategy",
            "get_strategy",
            "list_strategies",
            "list_scenarios",
            "get_scenario",
            "create_scenario",
            "update_slot",
            "update_manifest",
            "set_mechanical_param",
            "set_risk_config",
            "create_strategy_agent",
            "attach_agent",
            "validate_draft",
            "run_eval",
        ] {
            assert!(names.contains(&v), "missing verb {v} in {names:?}");
        }
    }

    #[test]
    fn strategy_setup_profile_focuses_tools_on_strategy_work() {
        let defs = agent_tool_defs(AgentProfile::StrategySetup);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"list_strategies"));
        assert!(names.contains(&"list_scenarios"));
        assert!(names.contains(&"get_scenario"));
        assert!(names.contains(&"run_eval"));
        assert!(
            !names.contains(&"fetch_bars"),
            "setup profile should not expose broad workspace fetch tools: {names:?}"
        );
    }

    #[test]
    fn workspace_profile_gets_broader_cli_job_tools() {
        let defs = agent_tool_defs(AgentProfile::Workspace);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"list_strategies"));
        assert!(names.contains(&"list_scenarios"));
        assert!(names.contains(&"fetch_bars"));
        assert!(names.contains(&"get_cli_job"));
        assert!(names.contains(&"get_cli_job_output"));
    }

    #[test]
    fn strategy_setup_prompt_biases_existing_strategy_and_scenario_reuse() {
        let prompt = AgentProfile::StrategySetup.prompt_section();
        assert!(prompt.contains("strategy setup"));
        assert!(prompt.contains("existing strategies"));
        assert!(prompt.contains("existing scenarios"));
    }

    // -- Plan #11 Phase B persistence assertions ----------------------------

    #[tokio::test]
    async fn user_message_persists_immediately_at_construction() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (mut _wl, pool, _td, sid) = loop_with_session(mock, "hi there", ContextScope::Workspace).await;
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
        let (mut wl, pool, _td, sid) = loop_with_session(mock, "hi", ContextScope::Workspace).await;
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
        let (mut wl, pool, _td, sid) = loop_with_session(mock, "list", ContextScope::Workspace).await;
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
        let (mut wl1, pool, td, sid) = loop_with_session(mock1, "first", ContextScope::Workspace).await;
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
