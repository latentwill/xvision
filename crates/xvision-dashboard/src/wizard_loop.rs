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

use crate::cli_jobs::eval_run_bridge;
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
use xvision_engine::strategies_folder;

const WIZARD_SYSTEM_PROMPT_BASE: &str = include_str!("../prompts/wizard.md");

/// Wizard-side input deserializer for `create_strategy`. The wizard's
/// surface intentionally does not accept a `template` — strategy
/// templates are no longer a library concept in the wizard (see
/// the `templates-elimination` contract, 2026-05-21). New drafts are
/// always blank; `create_strategy_agent` and `update_slot` fill them
/// in. The public API / MCP `CreateStrategyReq` shape is unchanged.
#[derive(Debug, Deserialize)]
struct WizardCreateStrategyInput {
    name: String,
    #[serde(default)]
    creator: Option<String>,
}

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
                 strategies. Use existing strategies and existing scenarios before creating new \
                 work. `create_strategy` always starts a blank draft — fill it in via \
                 `create_strategy_agent` / `update_slot` / `set_mechanical_param`. When you create \
                 a strategy, ensure it has a trader agent with an explicit provider/model before \
                 claiming it is eval-ready. \
                 Do not say a tool change succeeded until the tool_result says it succeeded. For \
                 strategy tools, pass `id` or `strategy_id` as a top-level field, never nested under \
                 the tool name. Ask one targeted clarification only when the available strategies/scenarios \
                 are genuinely ambiguous. \
                 When `validate_draft` returns `ok: false`, quote the `errors[]` text to the user \
                 verbatim before attempting any fix. If a second `validate_draft` call returns the \
                 same error class, stop editing and ask the user how to proceed — do not silently \
                 retry. A repeating validation error often means the validator itself is wrong, \
                 not the prompt."
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
    /// Most recent failing tool call: `(tool_name, error_message)`. Updated
    /// each iteration of the tool-use loop when `run_tool` returns an
    /// error. Quoted in the loop-cap diagnostic so operators land on the
    /// failing schema instead of the generic "stuck calling tools"
    /// message.
    last_tool_error: Option<(String, String)>,
    /// Signature of the most recent failing tool call:
    /// `(tool_name, error_class)`. `error_class` is a stable summary of
    /// the failure shape — sorted+joined `errors[]` for `validate_draft`,
    /// and the `error` string for any tool that surfaced its failure via
    /// the `Ok(json!({"error": ...}))` tool_result path (including the
    /// pre-dispatch `InvalidJobId` shape check on `get_cli_job{,_output}`
    /// added for F-10). Reset on success or when either the tool name or
    /// the error class changes. Paired with `tool_failure_streak` to
    /// break out of "model retries the same tool with the same broken
    /// argument" loops that the operator can never resolve (e.g. a
    /// false-positive validator rule, or a hallucinated `eval_run_*`
    /// job_id that will never resolve).
    last_tool_failure: Option<(String, String)>,
    tool_failure_streak: u32,
    /// Pending events queued during the current `next_event` invocation.
    pending: Vec<WizardEvent>,
    is_done: bool,
}

/// Hard cap on consecutive failures of the **same tool with the same
/// error class** before the wizard loop forcibly ends the turn and
/// surfaces the error to the user. Two means: one initial failure plus
/// one "tried to fix it and got the same error" — beyond that, the
/// model is not making progress and silently retrying just hides the
/// real issue. Originally added in PR #316 for `validate_draft`
/// (qa-round-5 F-3 / chat-rail-validate-retry-budget); generalised in
/// F-10 to cover `get_cli_job` / `get_cli_job_output` shape-check loops
/// without duplicating the data structure.
const MAX_TOOL_FAILURE_STREAK: u32 = 2;

/// Crockford base32 alphabet used by ULID (case-insensitive). Excludes
/// I, L, O, and U to avoid visual ambiguity. ULIDs are exactly 26 chars.
const ULID_LEN: usize = 26;

/// Returns true iff `s` is 26 chars of Crockford base32 (case-
/// insensitive). Excludes I, L, O, U per the ULID spec.
fn is_valid_ulid(s: &str) -> bool {
    if s.len() != ULID_LEN {
        return false;
    }
    s.bytes().all(|b| {
        matches!(
            b,
            b'0'..=b'9'
                | b'A'..=b'H'
                | b'J' | b'K'
                | b'M' | b'N'
                | b'P'..=b'T'
                | b'V'..=b'Z'
                | b'a'..=b'h'
                | b'j' | b'k'
                | b'm' | b'n'
                | b'p'..=b't'
                | b'v'..=b'z'
        )
    })
}

/// Returns true iff `s` is a syntactically valid `job_id` for the
/// `get_cli_job` / `get_cli_job_output` tools. Accepts the shapes the
/// workspace path can legitimately produce:
///
/// - A bare ULID (26 chars), e.g. when the model has been handed one
///   directly. Strictly the F-10 contract.
/// - `job_<ULID>` (30 chars), which is the shape `CliJobStore::create_queued`
///   actually produces today (`format!("job_{}", ulid::Ulid::new())`).
///   Rejecting these would break the entire `fetch_bars → get_cli_job`
///   flow, so we accept this single specific prefix.
/// - `eval_run_<ULID>`, a synthetic read-only bridge over `eval_runs`.
///   The wizard receives this shape from run-eval workflows; accepting it
///   keeps `get_cli_job{,_output}` able to resolve eval-run status without
///   writing duplicate cli_jobs rows.
///
/// Anything else — including the audit-evidence pattern
/// `eval_run_XKI6IWGw5aFZXsqkW3a3` (the suffix isn't even 26 chars) — is
/// rejected before the store is touched.
fn is_valid_cli_job_id(s: &str) -> bool {
    if is_valid_ulid(s) {
        return true;
    }
    if let Some(rest) = s.strip_prefix("job_") {
        return is_valid_ulid(rest);
    }
    if let Some(rest) = s.strip_prefix(eval_run_bridge::EVAL_RUN_PREFIX) {
        return is_valid_ulid(rest);
    }
    false
}

/// Reason a `job_id` failed the pre-dispatch shape check. Returned in
/// the `InvalidJobId` tool_result so the model gets a structured hint
/// (and doesn't just re-emit the same hallucinated id).
fn cli_job_id_rejection_reason(s: &str) -> &'static str {
    // Specifically diagnose the audit anti-pattern: an `eval_run_` /
    // `run_` / arbitrary-prefix id, where the model has stuffed a
    // different artifact's id (or a hallucinated one) into job_id.
    if s.starts_with(eval_run_bridge::EVAL_RUN_PREFIX) {
        return "job_id may use `eval_run_<ULID>` for eval-run bridge lookups, but the suffix must be a valid 26-character Crockford base32 ULID";
    }
    let known_bad_prefixes = ["run_", "agent_", "strategy_", "scenario_", "cycle_", "draft_"];
    for p in known_bad_prefixes {
        if s.starts_with(p) {
            return "job_id must be a CLI job id (bare ULID or `job_<ULID>`); the id you supplied looks like it belongs to a different artifact (eval run, strategy, scenario, cycle, draft) — call list_cli_jobs to get the right job_id";
        }
    }
    if !s.starts_with("job_") {
        "job_id must be a bare 26-character ULID (Crockford base32: 0-9, A-Z minus I/L/O/U; case-insensitive) or the `job_<ULID>` shape returned by fetch_bars"
    } else {
        "job_id has the expected `job_` prefix but the suffix is not a valid 26-character Crockford base32 ULID"
    }
}

/// Classify a tool_result value into a stable signature string for the
/// `(tool_name, error_class)` retry-budget guard, or `None` if the
/// result represents progress (no error → reset streak).
///
/// - `validate_draft`: classify by sorted+joined `errors[]` whenever
///   `ok: false`. Matches the original PR #316 semantics.
/// - Any tool whose result has a top-level `"error"` string (the
///   convention used by the per-tool `Ok(json!({"error": ...}))` path
///   AND by the catch-all `Err(e) -> json!({"error": e.to_string()})`
///   wrapper in `run_one_turn`): classify by that string. F-10 extends
///   this to cover `get_cli_job` / `get_cli_job_output` shape-check
///   failures and underlying "cli job '...' not found" lookups.
///
/// Note: `error` may be a JSON string (the existing wrapper) OR a JSON
/// object/code (e.g. `{"code": "InvalidJobId", "provided": ..., "reason": ...}`).
/// We accept either: if it's a string we use it directly; if it's an
/// object we canonicalise via `to_string()` so two equivalent objects
/// produce the same signature.
fn tool_failure_signature(tool: &str, result: &serde_json::Value) -> Option<String> {
    if tool == "validate_draft" {
        let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
        if ok {
            return None;
        }
        let mut errors: Vec<String> = result
            .get("errors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        errors.sort();
        return Some(errors.join("\n"));
    }
    if let Some(err) = result.get("error") {
        return Some(match err.as_str() {
            Some(s) => s.to_string(),
            None => err.to_string(),
        });
    }
    None
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
            last_tool_error: None,
            last_tool_failure: None,
            tool_failure_streak: 0,
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
                max_tokens: Some(1500),
                temperature: None,
                tools: agent_tool_defs(self.profile),
                response_schema: None,
                cache_control: None,
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
                    Err(e) => {
                        let msg = e.to_string();
                        self.last_tool_error = Some((name.clone(), msg.clone()));
                        serde_json::json!({ "error": msg })
                    }
                };
                self.maybe_track_draft_id(&name, &result_value);
                self.update_tool_failure_streak(&name, &result_value);
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

            if self.tool_failure_streak >= MAX_TOOL_FAILURE_STREAK {
                // Convergence guard: the model has hit the same tool +
                // error class twice in a row without making progress.
                // Silently looping hides the error from the operator
                // (see intake 2026-05-19-qa-validate-draft-cadence-
                // false-positive: a parser bug nobody could fix by
                // editing the prompt; and the F-10 audit
                // chat_session 01KRXXHPRBKYKVEM2Q1VBS2YJ4 where the
                // model called get_cli_job_output with a hallucinated
                // `eval_run_*` id repeatedly). Stop the turn and surface
                // the failure as a stuck card.
                self.emit_tool_loop_break().await?;
                self.is_done = true;
                self.pending.push(WizardEvent::Done {
                    draft_id: self.last_draft_id.clone(),
                });
                return Ok(());
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
        let trailer = match &self.last_tool_error {
            Some((tool, msg)) => format!(" — last failure: {tool} → {msg}"),
            None => " — no tool errors recorded; the model kept calling tools \
                     successfully but never returned a final response"
                .to_string(),
        };
        anyhow::bail!(
            "wizard tool-use loop exceeded {MAX_TOOL_LOOP_ITERATIONS} iterations \
             — model is stuck calling tools without responding{trailer}"
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

    /// Track consecutive same-tool same-error failures. Generalised from
    /// the `validate_draft`-only guard in PR #316 so it also covers
    /// shape-check failures on `get_cli_job` / `get_cli_job_output`
    /// (F-10) and any future tool whose error surface follows the
    /// `Ok(json!({"error": ...}))` tool_result convention.
    ///
    /// Resets the streak on success, on a different tool, or on a
    /// different error class (different error class → progress, even if
    /// still failing).
    fn update_tool_failure_streak(&mut self, tool: &str, result: &serde_json::Value) {
        let signature = tool_failure_signature(tool, result);
        match signature {
            None => {
                // Tool succeeded (or didn't surface an error class we
                // recognise) — clear the streak so a later same-shape
                // failure starts a fresh count.
                self.last_tool_failure = None;
                self.tool_failure_streak = 0;
            }
            Some(class) => {
                let key = (tool.to_string(), class);
                if self.last_tool_failure.as_ref() == Some(&key) {
                    self.tool_failure_streak = self.tool_failure_streak.saturating_add(1);
                } else {
                    self.last_tool_failure = Some(key);
                    self.tool_failure_streak = 1;
                }
            }
        }
    }

    /// Surface the stuck-tool state as a user-visible content block when
    /// the convergence guard fires. The block uses the same
    /// action-card primitive as `rich_block_for_tool_result` so the chat
    /// rail renders it inline — no popup, per the frontend rule.
    ///
    /// Persists the card as an assistant message before streaming so a
    /// chat-rail refresh / SSE drop after the guard fires still shows
    /// the explanation in history — matches the rich-block path at the
    /// end of `run_one_turn`.
    ///
    /// Was originally `emit_validate_loop_break` (validate_draft-only,
    /// PR #316). Generalised in F-10 so the same surface covers
    /// `get_cli_job` / `get_cli_job_output` shape-check loops.
    async fn emit_tool_loop_break(&mut self) -> anyhow::Result<()> {
        let id = self.last_draft_id.clone().unwrap_or_else(|| "unknown".into());
        let (failing_tool, signature) = match &self.last_tool_failure {
            Some((tool, sig)) => (tool.clone(), sig.clone()),
            None => ("(unknown)".to_string(), String::new()),
        };
        let errors_body = if signature.is_empty() {
            "(no error text returned)".to_string()
        } else {
            signature
                .lines()
                .map(|line| format!("• {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (title, action) = if failing_tool == "validate_draft" {
            (
                "Validation stuck — operator review needed".to_string(),
                InlineAction {
                    label: "Open draft".into(),
                    href: Some(format!("/authoring/{id}")),
                    command: None,
                },
            )
        } else {
            // For non-validate-draft tools (F-10: get_cli_job{,_output})
            // there isn't a specific draft URL to deep-link to. Send the
            // operator to the workspace home, which still lets them
            // pivot to whatever artifact the stuck tool was looking at.
            (
                format!("{failing_tool} stuck — operator review needed"),
                InlineAction {
                    label: "Open workspace".into(),
                    href: Some("/".to_string()),
                    command: None,
                },
            )
        };
        let body = format!(
            "`{failing_tool}` failed {streak}× in a row with the same error. \
             Stopping so the operator can decide what to do.\n\n{errors_body}",
            streak = self.tool_failure_streak,
        );
        let card = match action_confirmation_card(
            format!("tool-loop-break:{failing_tool}:{id}"),
            title,
            body,
            action,
        ) {
            Ok(card) => card,
            Err(_) => return Ok(()),
        };
        let block = match serde_json::to_value(card) {
            Ok(block) => block,
            Err(_) => return Ok(()),
        };
        ChatSessionStore::append(
            &self.pool,
            &self.session_id,
            "assistant",
            std::slice::from_ref(&block),
        )
        .await?;
        self.pending.push(WizardEvent::ContentBlock { block });
        Ok(())
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
            "create_strategy" => {
                // Wizard always creates a blank draft — no template
                // dispatch, no placeholder trader prompt. The agent
                // fills in real content via subsequent `create_strategy_agent`
                // and `update_slot` calls before save. Engine-side
                // `authoring::create_blank_strategy` constructs the
                // Strategy directly without consulting `template_registry`.
                //
                // Defensive: any failure here propagates verbatim via `?`.
                // We do NOT pre-cache `self.last_draft_id` (the surrounding
                // loop only tracks it from a successful tool_result `id`
                // field), and we do NOT chain `create_strategy_agent`
                // against a phantom id (the chain runs below only after
                // `create_blank_strategy` returns Ok).
                let raw: WizardCreateStrategyInput = serde_json::from_value(input)?;
                let store = xvision_engine::strategies::store::FilesystemStore::new(
                    xvision_engine::strategies::store::strategy_store_dir(&self.api_context.xvn_home),
                );
                let out = authoring::create_blank_strategy(&store, raw.name, raw.creator).await?;
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
                    limits: None,
                    skip_preflight: false,
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
                // F-10 pre-dispatch shape check: reject anything that
                // isn't a recognisable cli-job id before hitting the
                // store. The audit pattern `eval_run_XKI6IWGw5aFZXsqkW3a3`
                // (chat_session 01KRXXHPRBKYKVEM2Q1VBS2YJ4) would
                // otherwise loop forever returning "cli job '<bad-id>'
                // not found". The structured `InvalidJobId` error
                // surfaces via the existing tool_result path (same
                // mechanism as validate_draft errors, PR #316 F-2) and
                // feeds the generalised retry-budget guard so the
                // wizard force-ends after two same-error retries.
                if !is_valid_cli_job_id(job_id) {
                    return Ok(serde_json::json!({
                        "error": {
                            "code": "InvalidJobId",
                            "provided": job_id,
                            "reason": cli_job_id_rejection_reason(job_id),
                        }
                    }));
                }
                let job = if job_id.starts_with(eval_run_bridge::EVAL_RUN_PREFIX) {
                    eval_run_bridge::get_synthetic_job(&self.pool, job_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("eval run '{job_id}' not found"))?
                } else {
                    let store = CliJobStore::new(self.pool.clone());
                    store
                        .get(job_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?
                };
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
                // F-10 pre-dispatch shape check — see `get_cli_job`
                // arm above for the rationale. Same audit anti-pattern,
                // same surfacing mechanism.
                if !is_valid_cli_job_id(job_id) {
                    return Ok(serde_json::json!({
                        "error": {
                            "code": "InvalidJobId",
                            "provided": job_id,
                            "reason": cli_job_id_rejection_reason(job_id),
                        }
                    }));
                }
                let output = if job_id.starts_with(eval_run_bridge::EVAL_RUN_PREFIX) {
                    eval_run_bridge::get_synthetic_output(&self.pool, job_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("eval run '{job_id}' not found"))?
                } else {
                    let store = CliJobStore::new(self.pool.clone());
                    store
                        .output(job_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?
                };
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
            "list_strategies_folder" => {
                // V2F read-only surface: enumerate entries under
                // `$XVN_HOME/strategies/`. Missing folder → empty list.
                let subfolder = input
                    .get("subfolder")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty());
                let entries = strategies_folder::list(&self.api_context, subfolder).await?;
                Ok(serde_json::to_value(entries)?)
            }
            "read_strategies_file" => {
                let rel_path = input
                    .get("rel_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("read_strategies_file: missing `rel_path`"))?;
                let body = strategies_folder::read(&self.api_context, rel_path).await?;
                Ok(serde_json::to_value(body)?)
            }
            "list_strategy_ideas" => {
                // V2F closer: query the prepopulated strategy idea
                // library at `$XVN_HOME/strategies/library/templates/`.
                // Missing library returns an empty array; bad JSON
                // entries log a warning and are skipped.
                let filter = strategies_folder::ideas::IdeaFilter {
                    category: input
                        .get("category")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    indicator: input
                        .get("indicator")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    limit: input
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .map(|n| n.min(u32::MAX as u64) as u32),
                };
                let ideas = strategies_folder::ideas::list_ideas(&self.api_context, filter).await?;
                Ok(serde_json::to_value(ideas)?)
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
                limit: None,
                offset: None,
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
            .unwrap_or_default();
        let skill_ids = tools_for_strategy_role(&strategy, &role);
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
                    skill_ids,
                    // Default to "auto from model"; the dispatcher
                    // resolves this from the model's metadata at
                    // request time (q15 §1).
                    max_tokens: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    delta_briefing: None,
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

fn tools_for_strategy_role(strategy: &xvision_engine::strategies::Strategy, role: &str) -> Vec<String> {
    let role = role.trim().to_ascii_lowercase();
    if role == "trader" {
        return strategy
            .trader_slot
            .as_ref()
            .map(|slot| slot.allowed_tools.clone())
            .unwrap_or_default();
    }
    if role == "intern" {
        return strategy
            .intern_slot
            .as_ref()
            .map(|slot| slot.allowed_tools.clone())
            .unwrap_or_default();
    }
    if role == "regime" {
        return strategy
            .regime_slot
            .as_ref()
            .map(|slot| slot.allowed_tools.clone())
            .unwrap_or_default();
    }
    Vec::new()
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
            // Common LLM mis-names — collapse to the closest valid
            // variant. Round-4 operator finding (2026-05-18): Qwen
            // emitted `"UserGenerated"`, which serde rejected because
            // ScenarioSource has no such variant.
            ("usergenerated", "Generated"),
            ("user_generated", "Generated"),
            ("user-generated", "Generated"),
            ("autogenerated", "Generated"),
            ("auto_generated", "Generated"),
            ("auto-generated", "Generated"),
            ("system", "Generated"),
            ("default", "Canonical"),
            ("seed", "Canonical"),
            ("cloned", "Clone"),
            ("forked", "Clone"),
            ("custom", "User"),
        ],
    );
    // Final guardrail: if the value still isn't one of the four
    // canonical variants after alias mapping, fall back to "Generated"
    // rather than letting serde reject the whole tool call.
    coerce_to_one_of(
        obj,
        "source",
        &["Canonical", "User", "Clone", "Generated"],
        "Generated",
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
        // Always synthesise a window. The previous `if let Some(window) =
        // infer_time_window(...)` was a no-op when the display_name had no
        // Q1/Q2/Q3/Q4-style hint, leaving the payload without `time_window`
        // and serde would reject it (Qwen 2026-05-18 repro).
        let window = infer_time_window(&display_name).unwrap_or_else(default_time_window);
        obj.insert("time_window".into(), window);
    }
    // Repair `capital` actively: a malformed shape from the agent
    // (anything other than `{ initial: number, currency: string }`) is
    // replaced with the default. `entry().or_insert_with` only fills when
    // the key is *missing* — Qwen passed a shape that satisfied the key
    // check but failed serde at `missing field 'initial'`.
    let capital_valid = obj.get("capital").is_some_and(|v| {
        v.get("initial").and_then(|i| i.as_f64()).is_some()
            && v.get("currency").and_then(|c| c.as_str()).is_some()
    });
    if !capital_valid {
        if let Some(bad) = obj.get("capital") {
            tracing::warn!(
                bad = %bad,
                "create_scenario: repairing malformed capital field with default"
            );
        }
        obj.insert(
            "capital".into(),
            serde_json::json!({"initial": 100000.0, "currency": "USD"}),
        );
    }
    // Unwrap a tag-wrapped calendar shape (`{ "type": "Continuous24x7" }`)
    // to the form serde expects. Same Qwen 2026-05-18 repro: the agent
    // wrapped the variant and serde rejected with `unknown variant 'type'`.
    //
    // CalendarRef (xvision-engine::eval::scenario) is `enum { Continuous24x7,
    // UsEquities, Custom(String) }` with default (externally-tagged) serde:
    //   - unit variants serialize as `"Continuous24x7"` / `"UsEquities"`
    //   - Custom serializes as `{"Custom": "<name>"}` — NOT a bare string
    // so the unit and Custom branches need separate handling.
    if let Some(serde_json::Value::Object(cal_obj)) = obj.get("calendar").cloned() {
        if let Some(tag) = cal_obj.get("type").and_then(|v| v.as_str()) {
            let replacement = match tag {
                "Custom" => {
                    // Pull a name from any of the common keys the agent
                    // might use; fall back to the tag itself so we still
                    // produce a valid Custom("Custom") rather than dropping
                    // the variant.
                    let name = cal_obj
                        .get("name")
                        .or_else(|| cal_obj.get("value"))
                        .or_else(|| cal_obj.get("calendar"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| "Custom".into());
                    serde_json::json!({ "Custom": name })
                }
                other => serde_json::Value::String(other.to_string()),
            };
            obj.insert("calendar".into(), replacement);
        }
    }
    obj.entry("calendar")
        .or_insert_with(|| serde_json::json!("Continuous24x7"));
    // Round-4 operator finding (2026-05-18): Qwen emitted
    // `"calendar": "calendar"` (the field name as the value) and
    // serde rejected with `unknown variant 'calendar'`. Validate the
    // string-shape calendar against the known unit variants here; the
    // `{Custom: "..."}` object shape is left alone. Anything else
    // collapses to the safe default.
    let calendar_valid = match obj.get("calendar") {
        Some(serde_json::Value::String(s)) => matches!(s.as_str(), "Continuous24x7" | "UsEquities"),
        Some(serde_json::Value::Object(o)) => o.contains_key("Custom"),
        _ => false,
    };
    if !calendar_valid {
        if let Some(bad) = obj.get("calendar") {
            tracing::warn!(
                bad = %bad,
                "create_scenario: invalid calendar variant — defaulting to Continuous24x7"
            );
        }
        obj.insert(
            "calendar".into(),
            serde_json::Value::String("Continuous24x7".into()),
        );
    }
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

fn default_time_window() -> serde_json::Value {
    let end = Utc::now();
    let start = end - chrono::Duration::days(90);
    serde_json::json!({
        "start": start.to_rfc3339(),
        "end": end.to_rfc3339(),
    })
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

/// Final-pass guardrail: after [`normalize_enum_string`] has had a
/// chance to map aliases, ensure the field's string value is in
/// `valid`. If it isn't (e.g. the agent invented a variant name with
/// no alias entry), replace it with `default`. Logs a warn so the
/// repair is visible without breaking the tool call.
fn coerce_to_one_of(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    valid: &[&str],
    default: &str,
) {
    let Some(value) = string_field(obj, key) else {
        // If the field isn't a string at all, replace it.
        tracing::warn!(
            key,
            default,
            "create_scenario: missing/non-string enum field — defaulting"
        );
        obj.insert(key.into(), serde_json::Value::String(default.into()));
        return;
    };
    if !valid.iter().any(|v| *v == value.as_str()) {
        tracing::warn!(
            key,
            bad = %value,
            default,
            valid = ?valid,
            "create_scenario: invalid enum variant — defaulting"
        );
        obj.insert(key.into(), serde_json::Value::String(default.into()));
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
        "validate_draft" => {
            // Only render a card on failure — successful validation is a
            // no-op the model can speak to directly.
            if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true) {
                return None;
            }
            let id = result.get("id")?.as_str()?;
            let errors: Vec<String> = result
                .get("errors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let body = if errors.is_empty() {
                "Validation failed but the engine returned no error text. Open the draft to inspect."
                    .to_string()
            } else {
                errors
                    .iter()
                    .map(|e| format!("• {e}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            action_confirmation_card(
                format!("validate-failed:{id}"),
                "Validation failed",
                body,
                InlineAction {
                    label: "Open draft".into(),
                    href: Some(format!("/authoring/{id}")),
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
            name: "create_strategy".into(),
            description: "Instantiate a blank strategy draft. Fill it in with \
                          `create_strategy_agent` (attach an agent) and \
                          `update_slot` / `set_mechanical_param` / `update_manifest`. \
                          Returns { id }."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Human-readable name"},
                    "creator": {"type": "string", "description": "Optional @handle"}
                },
                "required": ["name"]
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
                    "attested_with": {"type": "string"},
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
        ToolDefinition {
            name: "list_strategies_folder".into(),
            description: "Reads from the user's strategies folder (read-only); \
                          enumerates entries under `$XVN_HOME/strategies/`. \
                          Use to consult their notes, library, or imported \
                          reference material when authoring strategies. \
                          Pass `subfolder` (one of `notes`, `docs`, \
                          `strategy-files`, `evals`, `library`) to scope the \
                          listing, or omit it for a full scan. Returns an \
                          empty list when the folder has not been \
                          initialised yet (no `xvn strategies init`).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "subfolder": {
                        "type": "string",
                        "enum": ["notes", "docs", "strategy-files", "evals", "library"],
                        "description": "Optional. Restrict the enumeration to one allowlisted subfolder."
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "read_strategies_file".into(),
            description: "Read one file's contents from the user's strategies \
                          folder. `rel_path` is the path relative to \
                          `$XVN_HOME/strategies/` as returned by \
                          `list_strategies_folder`. Bodies are truncated at \
                          256 KB; check `truncated` on the response.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": {
                        "type": "string",
                        "description": "Path relative to the strategies folder, e.g. `notes/idea.md`."
                    }
                },
                "required": ["rel_path"]
            }),
        },
        ToolDefinition {
            name: "list_strategy_ideas".into(),
            description: "queries the user's pre-populated strategy idea \
                          library; use when the user asks for examples or \
                          ideas. Returns idea summaries with category, \
                          indicators, short description, and the \
                          `source_rel_path` of the underlying template \
                          (pass that to `read_strategies_file` to fetch \
                          the full body). Empty array when \
                          `$XVN_HOME/strategies/library/templates/` \
                          hasn't been initialised yet (suggest \
                          `xvn strategies init`).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Optional category filter. Case-insensitive. \
                                        Known values: `ema`, `bollinger`, \
                                        `fibonacci`, `nansen`, `random`, \
                                        `rsi-volume`."
                    },
                    "indicator": {
                        "type": "string",
                        "description": "Optional indicator filter (case-insensitive \
                                        substring match against the derived indicators \
                                        list, e.g. `rsi`, `ema_200`, `funding_rate_8h`)."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100,
                        "description": "Optional cap on results. Default 20, capped at 100."
                    }
                },
                "required": []
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
    use xvision_engine::eval::run::{Run, RunMode};
    use xvision_engine::eval::store::RunStore;

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
        // Drive the loop through any in-profile tool; the assertions
        // are about the event sequence (ToolCall → ToolResult → Token →
        // Done), not the specific verb. `list_strategies` is the lowest-
        // friction verb available in both profiles — no required input.
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "list_strategies", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Here are your existing strategies.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "what can i build", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        assert!(matches!(&events[0], WizardEvent::ToolCall { tool, .. } if tool == "list_strategies"));
        match &events[1] {
            WizardEvent::ToolResult { tool, result } => {
                assert_eq!(tool, "list_strategies");
                assert!(result.is_array(), "expected an array, got {result}");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        assert!(matches!(&events[2], WizardEvent::Token { text } if text.contains("strategies")));
        assert!(matches!(&events[3], WizardEvent::Done { .. }));
    }

    /// V2F foundation: wizard exposes `list_strategies_folder` and
    /// `read_strategies_file` against `$XVN_HOME/strategies/`. This test
    /// drops a markdown note under `notes/`, drives the wizard through a
    /// `tool_use` for `list_strategies_folder`, and asserts the wizard
    /// dispatched the tool and the result lists the dropped note.
    #[tokio::test]
    async fn wizard_lists_strategies_folder_notes() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_lsf",
                "list_strategies_folder",
                serde_json::json!({"subfolder": "notes"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Found your notes.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, td, _sid) =
            loop_with_session(mock, "what notes do I have", ContextScope::Workspace).await;

        // Drop a markdown note into the per-user strategies folder
        // BEFORE draining the wizard loop so the dispatcher sees it on
        // its `list_strategies_folder` call.
        let notes_dir = td.path().join("strategies").join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        std::fs::write(notes_dir.join("ideas.md"), b"# my ideas\n\n- mean reversion\n").unwrap();

        let events = drain(&mut wl).await;
        assert!(
            matches!(&events[0], WizardEvent::ToolCall { tool, .. } if tool == "list_strategies_folder"),
            "first event: {:?}",
            events.first()
        );
        match &events[1] {
            WizardEvent::ToolResult { tool, result } => {
                assert_eq!(tool, "list_strategies_folder");
                let arr = result.as_array().expect("entries[]");
                assert!(
                    arr.iter()
                        .any(|e| e.get("rel_path").and_then(|v| v.as_str()) == Some("notes/ideas.md")),
                    "expected ideas.md in {result}"
                );
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    /// V2F closer: the wizard's `list_strategy_ideas` tool queries the
    /// curated library populated by `xvn strategies init` and surfaces
    /// EMA template summaries. This is the end-to-end V2F integration
    /// — `prepop::init` writes `library/templates/EMA/*.json`, and the
    /// wizard tool reads back from the same path.
    #[tokio::test]
    async fn wizard_lists_strategy_ideas_for_ema_category() {
        use xvision_engine::strategies_folder::prepop::{self, InitOptions};

        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_ideas",
                "list_strategy_ideas",
                serde_json::json!({"category": "ema", "limit": 3}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Here are three EMA ideas.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, td, _sid) =
            loop_with_session(mock, "give me three EMA strategy ideas", ContextScope::Workspace).await;

        // Populate the library the same way `xvn strategies init` does
        // in production. This proves the V2F closer integrates with the
        // V2F foundation (#414, #419) — same files, same path.
        prepop::init(td.path(), InitOptions::default())
            .await
            .expect("prepop init");

        let events = drain(&mut wl).await;
        assert!(
            matches!(&events[0], WizardEvent::ToolCall { tool, args } if tool == "list_strategy_ideas"
                && args.get("category").and_then(|v| v.as_str()) == Some("ema")),
            "expected list_strategy_ideas with category=ema, got {:?}",
            events.first()
        );
        match &events[1] {
            WizardEvent::ToolResult { tool, result } => {
                assert_eq!(tool, "list_strategy_ideas");
                let arr = result.as_array().expect("ideas[]");
                assert!(
                    arr.len() >= 3,
                    "expected at least three EMA ideas, got {} in {result}",
                    arr.len()
                );
                // Every returned row must be in the EMA category and
                // carry a non-empty name + source_rel_path that points
                // back at the library template tree.
                for row in arr {
                    assert_eq!(
                        row.get("category").and_then(|v| v.as_str()),
                        Some("ema"),
                        "non-ema row leaked into category=ema filter: {row}"
                    );
                    let rel = row
                        .get("source_rel_path")
                        .and_then(|v| v.as_str())
                        .expect("source_rel_path");
                    assert!(
                        rel.starts_with("library/templates/"),
                        "source_rel_path must be under library/templates/: {rel}"
                    );
                    assert!(
                        row.get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| !s.is_empty())
                            .unwrap_or(false),
                        "name must be non-empty: {row}"
                    );
                }
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    /// V2F foundation: `read_strategies_file` returns the file body
    /// through the wizard dispatcher.
    #[tokio::test]
    async fn wizard_reads_strategies_file() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, td, _sid) = loop_with_session(mock, "open my note", ContextScope::Workspace).await;

        let notes_dir = td.path().join("strategies").join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        std::fs::write(notes_dir.join("idea.md"), b"# idea\n\nlong body\n").unwrap();

        let out = wl
            .run_tool(
                "read_strategies_file",
                serde_json::json!({"rel_path": "notes/idea.md"}),
            )
            .await
            .expect("read_strategies_file");
        assert_eq!(out["rel_path"].as_str(), Some("notes/idea.md"));
        assert_eq!(out["kind"].as_str(), Some("markdown"));
        assert_eq!(out["truncated"].as_bool(), Some(false));
        assert!(out["content"].as_str().unwrap().contains("# idea"));
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
    async fn create_scenario_repairs_missing_time_window() {
        // Qwen 2026-05-18 repro: agent omitted `time_window` and the
        // display_name had no Q1/Q2/Q3/Q4 hint, so infer_time_window
        // returned None and the payload reached serde without the
        // field — failing with `missing field 'time_window'`.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "make a btc range scenario", ContextScope::Workspace).await;

        let out = wl
            .run_tool(
                "create_scenario",
                serde_json::json!({
                    "create_scenario": {
                        "display_name": "BTC Range",
                        "asset": "BTC",
                        "granularity": "4h"
                    }
                }),
            )
            .await
            .expect("create scenario should synthesise time_window when undecidable");

        let tw = &out["time_window"];
        assert!(
            tw.is_object(),
            "time_window must be a populated object, got: {tw:?}"
        );
        assert!(
            tw["start"].as_str().is_some(),
            "time_window.start must be a string"
        );
        assert!(tw["end"].as_str().is_some(), "time_window.end must be a string");
    }

    #[tokio::test]
    async fn create_scenario_repairs_malformed_capital() {
        // Qwen 2026-05-18 repro #2: agent passed a `capital` object
        // missing `initial`, and `entry().or_insert_with` skipped the
        // default because the key was present. Serde then failed with
        // `missing field 'initial'`. Repair replaces the bad shape.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "btc 90d scenario", ContextScope::Workspace).await;

        let out = wl
            .run_tool(
                "create_scenario",
                serde_json::json!({
                    "create_scenario": {
                        "display_name": "BTC Repair",
                        "asset": "BTC",
                        "granularity": "4h",
                        "capital": {"amount": 50000}
                    }
                }),
            )
            .await
            .expect("create scenario should repair malformed capital");

        assert_eq!(out["capital"]["initial"], 100000.0);
        assert_eq!(out["capital"]["currency"], "USD");
    }

    #[tokio::test]
    async fn create_scenario_unwraps_tagged_calendar_variant() {
        // Qwen 2026-05-18 repro #3: agent passed `calendar: { "type":
        // "Continuous24x7" }`. Serde rejected with `unknown variant
        // 'type'`. Unwrap to the bare string variant before serde sees
        // it.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "btc scenario", ContextScope::Workspace).await;

        let out = wl
            .run_tool(
                "create_scenario",
                serde_json::json!({
                    "create_scenario": {
                        "display_name": "BTC Calendar",
                        "asset": "BTC",
                        "granularity": "4h",
                        "calendar": {"type": "Continuous24x7"}
                    }
                }),
            )
            .await
            .expect("create scenario should unwrap tag-wrapped calendar variant");

        assert_eq!(out["calendar"], "Continuous24x7");
    }

    #[test]
    fn normalize_create_scenario_unwraps_us_equities_calendar() {
        // Same unwrap rule for the `UsEquities` variant — covered as
        // a unit test on the normalizer so we don't need to spin up
        // a wizard session.
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert("calendar".into(), serde_json::json!({"type": "UsEquities"}));
        normalize_create_scenario_input(&mut obj);
        assert_eq!(obj.get("calendar"), Some(&serde_json::json!("UsEquities")));
    }

    #[test]
    fn normalize_create_scenario_rewrites_tagged_custom_calendar_to_externally_tagged_form() {
        // `CalendarRef::Custom(String)` serializes as `{"Custom": "<name>"}`,
        // NOT a bare string. If the agent passes `{"type": "Custom",
        // "name": "tokyo_hours"}` the unwrap must produce
        // `{"Custom": "tokyo_hours"}` — collapsing to bare `"Custom"`
        // would drop the payload and serde would still fail.
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert(
            "calendar".into(),
            serde_json::json!({"type": "Custom", "name": "tokyo_hours"}),
        );
        normalize_create_scenario_input(&mut obj);
        assert_eq!(
            obj.get("calendar"),
            Some(&serde_json::json!({"Custom": "tokyo_hours"})),
        );
    }

    #[test]
    fn normalize_create_scenario_custom_calendar_falls_back_to_self_named_string() {
        // If the agent passes `{"type": "Custom"}` with no payload,
        // produce `{"Custom": "Custom"}` rather than dropping the
        // variant or collapsing to a bare string.
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert("calendar".into(), serde_json::json!({"type": "Custom"}));
        normalize_create_scenario_input(&mut obj);
        assert_eq!(
            obj.get("calendar"),
            Some(&serde_json::json!({"Custom": "Custom"})),
        );
    }

    #[test]
    fn normalize_create_scenario_falls_back_when_calendar_is_field_name_garbage() {
        // Round-4 operator finding (2026-05-18): Qwen emitted
        // `"calendar": "calendar"` (the field name as the value) and
        // serde rejected with `unknown variant 'calendar'`. The repair
        // must rewrite this to the safe default rather than passing
        // the garbage value through.
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert("calendar".into(), serde_json::Value::String("calendar".into()));
        normalize_create_scenario_input(&mut obj);
        assert_eq!(obj.get("calendar"), Some(&serde_json::json!("Continuous24x7")));
    }

    #[test]
    fn normalize_create_scenario_preserves_valid_calendar_strings() {
        // Sanity: a valid string variant must pass through untouched.
        for variant in ["Continuous24x7", "UsEquities"] {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "display_name".into(),
                serde_json::Value::String("BTC Q1 2026".into()),
            );
            obj.insert("calendar".into(), serde_json::Value::String(variant.into()));
            normalize_create_scenario_input(&mut obj);
            assert_eq!(
                obj.get("calendar"),
                Some(&serde_json::Value::String(variant.into())),
                "valid variant {variant} must not be rewritten",
            );
        }
    }

    #[test]
    fn normalize_create_scenario_aliases_user_generated_source() {
        // Round-4 operator finding (2026-05-18): Qwen emitted
        // `"source": "UserGenerated"`. ScenarioSource has only
        // Canonical | User | Clone | Generated. Map the LLM-invented
        // name to the closest valid variant (Generated) so the tool
        // call succeeds.
        for input in [
            "UserGenerated",
            "user_generated",
            "user-generated",
            "AutoGenerated",
        ] {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "display_name".into(),
                serde_json::Value::String("BTC Q1 2026".into()),
            );
            obj.insert("source".into(), serde_json::Value::String(input.into()));
            normalize_create_scenario_input(&mut obj);
            assert_eq!(
                obj.get("source"),
                Some(&serde_json::Value::String("Generated".into())),
                "input {input} must alias to Generated",
            );
        }
    }

    #[test]
    fn normalize_create_scenario_falls_back_when_source_is_unknown() {
        // Anything the alias table doesn't recognize falls back to
        // "Generated" (the safest default — least surprising for the
        // user, doesn't pretend to be canonical).
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert("source".into(), serde_json::Value::String("totally_bogus".into()));
        normalize_create_scenario_input(&mut obj);
        assert_eq!(obj.get("source"), Some(&serde_json::json!("Generated")));
    }

    #[test]
    fn normalize_create_scenario_custom_calendar_reads_value_key_too() {
        // Some agents may put the payload under `value` instead of
        // `name`. The unwrap accepts either.
        let mut obj = serde_json::Map::new();
        obj.insert(
            "display_name".into(),
            serde_json::Value::String("BTC Q1 2026".into()),
        );
        obj.insert(
            "calendar".into(),
            serde_json::json!({"type": "Custom", "value": "asia_session"}),
        );
        normalize_create_scenario_input(&mut obj);
        assert_eq!(
            obj.get("calendar"),
            Some(&serde_json::json!({"Custom": "asia_session"})),
        );
    }

    #[test]
    fn tool_loop_cap_message_includes_last_tool_error() {
        // Pure assertion on the trailer-building branch the loop-cap
        // bail uses. The full loop-cap reproduction would require a
        // mock dispatch that returns 12+ malformed tool_use blocks;
        // covered here at the message level instead.
        let last_tool_error: Option<(String, String)> =
            Some(("create_scenario".into(), "missing field `time_window`".into()));
        let trailer = match &last_tool_error {
            Some((tool, msg)) => format!(" — last failure: {tool} → {msg}"),
            None => " — no tool errors recorded".to_string(),
        };
        let message = format!(
            "wizard tool-use loop exceeded {MAX_TOOL_LOOP_ITERATIONS} iterations \
             — model is stuck calling tools without responding{trailer}"
        );
        assert!(message.contains("create_scenario"));
        assert!(message.contains("missing field `time_window`"));
        assert!(message.contains("last failure"));
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
    async fn create_strategy_produces_blank_draft_with_no_template() {
        // Wizard `create_strategy` always produces a blank draft —
        // no template dispatch, no placeholder trader prompt. The
        // downstream `create_strategy_agent` / `update_slot` flow
        // attaches a real agent.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, td, _sid) =
            loop_with_session(mock, "make me a strategy", ContextScope::Workspace).await;

        let out = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "Blank Run" }))
            .await
            .expect("create_strategy without template must succeed");

        let id = out["id"].as_str().expect("returned id must be a string");
        assert!(!id.is_empty(), "draft id must be non-empty");

        let store = xvision_engine::strategies::store::FilesystemStore::new(
            xvision_engine::strategies::store::strategy_store_dir(td.path()),
        );
        let draft = xvision_engine::authoring::get_strategy(&store, id)
            .await
            .expect("draft must load");
        assert!(draft.agents.is_empty(), "blank draft must have no AgentRefs");
        assert!(
            draft.trader_slot.is_none(),
            "blank draft must not carry a placeholder trader slot"
        );
        assert_eq!(draft.manifest.template, "custom");
        assert_eq!(draft.manifest.display_name, "Blank Run");
    }

    #[tokio::test]
    async fn create_strategy_then_attach_agent_matches_operator_transcript_shape() {
        // 2026-05-21 operator transcript: ask for a fibonacci+RSI strategy
        // using Gemini Flash Lite 3.1 as the agent's model. Before
        // templates-elimination, the wizard scaffolded a placeholder
        // trader_slot that tripped the agents save-gate. After:
        // `create_strategy` returns `{ id }` for a blank draft, then
        // `create_strategy_agent` attaches a real Gemini agent in one
        // step.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, td, _sid) = loop_with_session(
            mock,
            "build me a fibonacci+RSI strategy using Gemini Flash Lite 3.1",
            ContextScope::Workspace,
        )
        .await;

        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "Fib+RSI" }))
            .await
            .expect("create_strategy must succeed with no template");
        let id = created["id"].as_str().expect("created.id").to_string();

        // The wizard's chained create_default_strategy_agent runs only
        // when agent_provider+model are set in the loop; `loop_with_session`
        // builds the loop with neither set, so no auto-attach happens.
        // The model would normally follow up with an explicit
        // create_strategy_agent call — simulate that.
        assert!(
            created.get("agent").is_none(),
            "no auto-attached agent when wizard agent_provider/model are unset"
        );

        let attached = wl
            .run_tool(
                "create_strategy_agent",
                serde_json::json!({
                    "strategy_id": id,
                    "role": "trader",
                    "provider": "google",
                    "model": "gemini-2.5-flash-lite",
                    "system_prompt":
                        "You are a Fibonacci-retracement + RSI trader. Use 0.382/0.5/0.618 \
                         retracement levels as support/resistance and only enter long when \
                         RSI(14) crosses up through 30 from below. Exit long when RSI(14) \
                         crosses down through 70 or price closes below the latest swing low. \
                         Return JSON: { action, conviction (0-1), justification }."
                }),
            )
            .await
            .expect("create_strategy_agent must succeed against the blank draft");

        assert_eq!(attached["strategy_id"], id);
        assert_eq!(attached["provider"], "google");
        assert_eq!(attached["model"], "gemini-2.5-flash-lite");
        assert_eq!(attached["agents"][0]["role"], "trader");

        let store = xvision_engine::strategies::store::FilesystemStore::new(
            xvision_engine::strategies::store::strategy_store_dir(td.path()),
        );
        let draft = xvision_engine::authoring::get_strategy(&store, &id)
            .await
            .unwrap();
        assert_eq!(draft.agents.len(), 1);
        assert_eq!(draft.agents[0].role, "trader");
        // Save-gate check: the trader agent's slot prompt is real (not
        // the canonical placeholder), so the agents save-gate would
        // accept it. We don't run the save-gate here — that's covered
        // by the engine-level `agent_save_validate` tests — but we
        // confirm the wizard wrote a non-trivial prompt.
        assert!(
            draft.trader_slot.is_none(),
            "blank draft never grew a trader_slot"
        );
    }

    #[tokio::test]
    async fn create_strategy_failure_does_not_cache_last_draft_id_or_chain() {
        // Defensive (contract finding #3): if `create_strategy` fails,
        // the wizard must not cache `last_draft_id` and must not chain
        // `create_strategy_agent` against a phantom id. We trip the
        // deserializer with an empty input (missing required `name`),
        // assert that the failure surfaces as a `{"error": ...}`
        // tool_result, that no follow-on `create_strategy_agent` event
        // is observed, and that `Done.draft_id` is None.
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_bad",
                "create_strategy",
                // Missing required `name` — WizardCreateStrategyInput
                // deserialize fails before we ever touch the store.
                serde_json::json!({}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "I need a name.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "make a strategy", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;

        // ToolResult for the failed create_strategy must carry an error.
        let tr = events
            .iter()
            .find(|e| matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "create_strategy"))
            .expect("expected a create_strategy ToolResult event");
        match tr {
            WizardEvent::ToolResult { result, .. } => {
                assert!(
                    result.get("error").is_some(),
                    "create_strategy failure must surface as {{ error: ... }}: {result}"
                );
            }
            _ => unreachable!(),
        }

        // No follow-on create_strategy_agent invocation must appear.
        let any_chained_attach = events.iter().any(|e| {
            matches!(e, WizardEvent::ToolCall { tool, .. } if tool == "create_strategy_agent")
                || matches!(e, WizardEvent::ToolResult { tool, .. } if tool == "create_strategy_agent")
        });
        assert!(
            !any_chained_attach,
            "no create_strategy_agent event must follow a failed create_strategy: {events:?}"
        );

        // Done.draft_id must be None — last_draft_id was never set.
        let done = events
            .iter()
            .rev()
            .find(|e| matches!(e, WizardEvent::Done { .. }))
            .expect("expected a Done event");
        match done {
            WizardEvent::Done { draft_id } => {
                assert!(
                    draft_id.is_none(),
                    "Done.draft_id must be None after a failed create_strategy: got {draft_id:?}"
                );
            }
            _ => unreachable!(),
        }
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
        // Wizard always produces a blank `custom` draft now — the
        // `template` field in the tool input is ignored. Template-driven
        // dispatch remains available via the public API / MCP / CLI.
        assert_eq!(created.template, "custom");
    }

    #[tokio::test]
    async fn create_strategy_missing_name_surfaces_as_tool_result_error() {
        // After templates-elimination the wizard no longer dispatches on
        // `template`, so the only required field is `name`. Drop it and
        // the deserializer must surface the failure as a tool_result
        // error (not panic, not silently succeed).
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "create_strategy", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Need a name.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 5,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "make me a strategy", ContextScope::Workspace).await;
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
        // After templates-elimination: `list_templates` is no longer
        // advertised by the wizard. Strategy templates remain available
        // via the public API / MCP / CLI for other consumers.
        assert!(
            !names.contains(&"list_templates"),
            "wizard must not advertise list_templates after templates-elimination: {names:?}"
        );
    }

    #[test]
    fn create_strategy_tool_schema_drops_template_field() {
        // After templates-elimination: the wizard's `create_strategy`
        // tool no longer declares a `template` field; the schema's
        // `properties` is limited to `name` (required) and `creator`.
        let defs = wizard_tool_defs();
        let create = defs
            .iter()
            .find(|d| d.name == "create_strategy")
            .expect("create_strategy tool def must exist");
        assert!(
            create.input_schema.pointer("/properties/template").is_none(),
            "template property must not be in the schema: {}",
            create.input_schema
        );
        let required = create
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(
            !required.contains(&"template"),
            "template must not be in required: got {required:?}"
        );
        assert!(
            required.contains(&"name"),
            "name must still be required: got {required:?}"
        );
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
            MockDispatch::tool_use("tu_1", "list_strategies", serde_json::json!({})),
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

    // ---- F-10: chat-rail-tool-id-validation -------------------------------
    //
    // Audit (chat_session 01KRXXHPRBKYKVEM2Q1VBS2YJ4): the model called
    // get_cli_job / get_cli_job_output with `eval_run_XKI6IWGw5aFZXsqkW3a3`,
    // got "cli job '<bad-id>' not found" back, and retried forever. The
    // F-10 fix is a pre-dispatch shape check that surfaces a typed
    // InvalidJobId via the existing tool_result error path, plus
    // extending the PR #316 (qa-round-5 F-3) retry-budget guard to
    // cover these two new tools without duplicating the data
    // structure.

    #[test]
    fn is_valid_ulid_accepts_canonical_26char_crockford() {
        // ULID spec: 26 chars of Crockford base32 (0-9, A-Z minus I/L/O/U),
        // case-insensitive. `ulid::Ulid::new().to_string()` is a known-
        // good fixture; round-trip a freshly generated id.
        let fresh = ulid::Ulid::new().to_string();
        assert_eq!(fresh.len(), 26);
        assert!(is_valid_ulid(&fresh), "fresh ulid rejected: {fresh}");
        // Lowercase is also accepted (case-insensitive per the spec).
        assert!(is_valid_ulid(&fresh.to_lowercase()));
    }

    #[test]
    fn is_valid_ulid_rejects_audit_anti_pattern() {
        // Exact audit fixture: prefix + 20-char suffix. Two reasons to
        // reject: wrong length AND contains `_` and lowercase
        // i/l-adjacent chars outside Crockford.
        assert!(!is_valid_ulid("eval_run_XKI6IWGw5aFZXsqkW3a3"));
        // Bare suffix is also not 26 chars.
        assert!(!is_valid_ulid("XKI6IWGw5aFZXsqkW3a3"));
        // Empty.
        assert!(!is_valid_ulid(""));
        // 26 chars but contains a disallowed letter (I).
        assert!(!is_valid_ulid("IIIIIIIIIIIIIIIIIIIIIIIIII"));
        // 26 chars but contains `_`.
        assert!(!is_valid_ulid("0123456789ABCDEFGHJKMNPQR_"));
    }

    #[test]
    fn is_valid_cli_job_id_accepts_bare_and_job_prefix() {
        let fresh = ulid::Ulid::new().to_string();
        assert!(is_valid_cli_job_id(&fresh), "bare ulid rejected: {fresh}");
        // The actual shape `CliJobStore::create_queued` mints — must be
        // accepted so the existing fetch_bars → get_cli_job flow works.
        let prefixed = format!("job_{}", fresh);
        assert!(is_valid_cli_job_id(&prefixed), "job_<ulid> rejected: {prefixed}");
        // Eval runs are bridged into the cli-job surface using this
        // synthetic prefix. The suffix still has to be a real ULID.
        let eval_prefixed = format!("{}{}", eval_run_bridge::EVAL_RUN_PREFIX, fresh);
        assert!(
            is_valid_cli_job_id(&eval_prefixed),
            "eval_run_<ulid> rejected: {eval_prefixed}"
        );
    }

    #[test]
    fn is_valid_cli_job_id_rejects_audit_anti_pattern() {
        assert!(!is_valid_cli_job_id("eval_run_XKI6IWGw5aFZXsqkW3a3"));
        // Other artifact-id shapes the model might confuse for a job
        // id — all rejected by the same check.
        assert!(!is_valid_cli_job_id("run_01HABCDEFGHJKMNPQRSTVWXYZ"));
        assert!(!is_valid_cli_job_id("strategy_01HABCDEFGHJKMNPQRSTVWXYZ"));
        // A `job_` prefix with a bad suffix is still rejected.
        assert!(!is_valid_cli_job_id("job_eval_run_XKI6IWGw5aFZXsqkW3a3"));
        assert!(!is_valid_cli_job_id("job_"));
    }

    #[test]
    fn cli_job_id_rejection_reason_calls_out_wrong_artifact() {
        let reason = cli_job_id_rejection_reason("eval_run_XKI6IWGw5aFZXsqkW3a3");
        assert!(
            reason.contains("eval_run_<ULID>") && reason.contains("suffix"),
            "reason should mention the invalid eval-run suffix, got: {reason}"
        );
    }

    #[test]
    fn tool_failure_signature_classifies_invalid_job_id_object() {
        // The shape `get_cli_job` returns when it rejects a bad id —
        // `error` is an object, not a string. The signature function
        // must canonicalise it so two equal bad-id retries produce the
        // same signature and the streak guard triggers.
        let result = serde_json::json!({
            "error": {
                "code": "InvalidJobId",
                "provided": "eval_run_XKI6IWGw5aFZXsqkW3a3",
                "reason": "any reason",
            }
        });
        let sig1 = tool_failure_signature("get_cli_job", &result).expect("classified");
        let sig2 = tool_failure_signature("get_cli_job", &result).expect("classified");
        assert_eq!(sig1, sig2, "same input must produce same signature");
        assert!(sig1.contains("InvalidJobId"), "sig: {sig1}");
    }

    #[test]
    fn tool_failure_signature_classifies_validate_draft_errors() {
        // Preserve PR #316 semantics: validate_draft `ok: false` with a
        // sorted-joined errors[] list. This guards against a regression
        // where the rename to the generalised guard accidentally drops
        // the validate_draft path.
        let result = serde_json::json!({
            "ok": false,
            "errors": ["b error", "a error"],
        });
        let sig = tool_failure_signature("validate_draft", &result).expect("classified");
        assert_eq!(sig, "a error\nb error");
        // ok: true returns None (resets the streak).
        let ok_result = serde_json::json!({ "ok": true });
        assert!(tool_failure_signature("validate_draft", &ok_result).is_none());
    }

    #[tokio::test]
    async fn get_cli_job_with_audit_anti_pattern_returns_invalid_job_id_without_hitting_store() {
        // Acceptance #1: the exact audit fixture must short-circuit
        // before the store is touched.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "look up that job", ContextScope::Workspace).await;
        let result = wl
            .run_tool(
                "get_cli_job",
                serde_json::json!({"job_id": "eval_run_XKI6IWGw5aFZXsqkW3a3"}),
            )
            .await
            .expect("shape check must surface via Ok(tool_result), not anyhow::Err");
        let err = result.get("error").expect("error key present");
        assert_eq!(err["code"], "InvalidJobId");
        assert_eq!(err["provided"], "eval_run_XKI6IWGw5aFZXsqkW3a3");
        assert!(err["reason"].as_str().is_some());
        // Sanity: no cli_jobs row was inserted (we never created one and
        // the bad-id path never went near the store).
        let store = CliJobStore::new(wl.pool.clone());
        assert!(store
            .get("eval_run_XKI6IWGw5aFZXsqkW3a3")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_cli_job_output_with_audit_anti_pattern_returns_invalid_job_id() {
        // Same as above but for the output verb — the audit shows the
        // model alternated between get_cli_job and get_cli_job_output.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "dump that job", ContextScope::Workspace).await;
        let result = wl
            .run_tool(
                "get_cli_job_output",
                serde_json::json!({"job_id": "eval_run_XKI6IWGw5aFZXsqkW3a3"}),
            )
            .await
            .expect("shape check must surface via Ok(tool_result)");
        let err = result.get("error").expect("error key present");
        assert_eq!(err["code"], "InvalidJobId");
        assert_eq!(err["provided"], "eval_run_XKI6IWGw5aFZXsqkW3a3");
    }

    #[tokio::test]
    async fn get_cli_job_with_valid_id_reaches_store() {
        // Acceptance #2: a valid id (the `job_<ULID>` shape the store
        // actually mints) is NOT short-circuited — it passes the shape
        // check and dispatches to the store, where the existing happy-
        // path serialisation applies.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, pool, _td, _sid) = loop_with_session(mock, "queue a fetch", ContextScope::Workspace).await;
        let store = CliJobStore::new(pool);
        let job = store
            .create_queued(vec!["bars".into(), "fetch".into()], 60)
            .await
            .expect("create queued job for shape-check happy path");
        let result = wl
            .run_tool("get_cli_job", serde_json::json!({"job_id": job.job_id}))
            .await
            .expect("valid id should reach the store");
        // Store-backed response includes the same job_id and a status
        // field — confirms we went through the real store path, not
        // the InvalidJobId short-circuit.
        assert_eq!(result["job_id"], serde_json::Value::String(job.job_id.clone()));
        assert_eq!(result["status"], "queued");
        assert!(result.get("error").is_none(), "unexpected error: {result}");
    }

    #[tokio::test]
    async fn get_cli_job_with_valid_eval_run_id_reaches_bridge() {
        // PR #348's bridge intentionally accepts eval_run_<ULID> ids. PR #349's
        // shape check must not reject that valid synthetic job id before the
        // bridge can translate it.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, pool, td, _sid) = loop_with_session(mock, "watch eval run", ContextScope::Workspace).await;
        seed_defaults(&pool, &td).await;
        let store = RunStore::new(pool);
        let run = Run::new_queued(
            "agent-test".into(),
            "crypto-rangebound-q2-2025".into(),
            RunMode::Backtest,
        );
        store.create(&run).await.expect("seed eval run");
        let job_id = format!("{}{}", eval_run_bridge::EVAL_RUN_PREFIX, run.id);

        let result = wl
            .run_tool("get_cli_job", serde_json::json!({"job_id": job_id}))
            .await
            .expect("valid eval_run id should reach the bridge");

        assert_eq!(result["job_id"], serde_json::Value::String(job_id));
        assert_eq!(result["status"], "queued");
        assert!(result.get("error").is_none(), "unexpected error: {result}");
    }

    #[tokio::test]
    async fn get_cli_job_with_bare_ulid_passes_shape_check() {
        // Bare 26-char ULID (no `job_` prefix) — passes shape check,
        // then hits the store and returns the underlying "not found"
        // error via the anyhow::bail! → Err path (which the wrapper
        // converts to `{"error": "..."}` in the tool_use loop). At the
        // `run_tool` level this surfaces as Err — the important thing
        // is that we did NOT short-circuit with InvalidJobId.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "look up that job", ContextScope::Workspace).await;
        let bare = ulid::Ulid::new().to_string();
        let result = wl
            .run_tool("get_cli_job", serde_json::json!({"job_id": bare.clone()}))
            .await;
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("not found"),
                    "expected store-level 'not found', got: {msg}"
                );
                assert!(
                    !msg.contains("InvalidJobId"),
                    "bare ulid must not be rejected by shape check: {msg}"
                );
            }
            Ok(v) => {
                // If a future change makes the store return Ok with an
                // error key, that still counts as reaching the store —
                // just confirm we didn't short-circuit with InvalidJobId.
                if let Some(err) = v.get("error") {
                    assert_ne!(err.get("code").and_then(|c| c.as_str()), Some("InvalidJobId"));
                }
            }
        }
    }

    #[tokio::test]
    async fn two_same_error_get_cli_job_output_retries_force_end_with_stuck_card() {
        // Acceptance #3: two consecutive get_cli_job_output calls with
        // the same bad id trip the generalised retry-budget guard
        // (PR #316 extended to cover get_cli_job{,_output} in F-10).
        // The loop force-ends and a stuck card is emitted via the same
        // action_confirmation_card primitive validate_draft uses.
        let bad = serde_json::json!({"job_id": "eval_run_XKI6IWGw5aFZXsqkW3a3"});
        let mock = Arc::new(MockDispatch::sequence(vec![
            // Turn 1: model calls get_cli_job_output with the bad id.
            MockDispatch::tool_use("tu_1", "get_cli_job_output", bad.clone()),
            // Turn 2: model retries with the same bad id (same error
            // class → streak increments to 2 → MAX_TOOL_FAILURE_STREAK).
            MockDispatch::tool_use("tu_2", "get_cli_job_output", bad.clone()),
            // Defensive trailing response in case the loop isn't
            // force-ended (test will fail loudly on the assertions below).
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "should not be reached".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "dump that job", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;

        // Count how many times we dispatched the bad tool.
        let bad_calls = events
            .iter()
            .filter(|e| matches!(e, WizardEvent::ToolCall { tool, .. } if tool == "get_cli_job_output"))
            .count();
        assert_eq!(
            bad_calls, 2,
            "loop must force-end after the 2nd same-error retry, not keep dispatching: {events:#?}"
        );

        // Stuck card surfaced — same primitive validate_draft uses.
        // The RichContentBlock serialises as `{"type": "action_card",
        // "action_id": "tool-loop-break:<tool>:<id>", ...}`.
        let card = events.iter().find_map(|e| match e {
            WizardEvent::ContentBlock { block } => {
                let action_id = block.get("action_id").and_then(|v| v.as_str()).unwrap_or("");
                if action_id.starts_with("tool-loop-break:get_cli_job_output") {
                    Some(block)
                } else {
                    None
                }
            }
            _ => None,
        });
        assert!(
            card.is_some(),
            "expected a tool-loop-break ContentBlock for get_cli_job_output, got: {events:#?}"
        );

        // Turn ends with Done — the wizard reports completion so the
        // chat-rail SSE stream closes cleanly.
        assert!(
            matches!(events.last(), Some(WizardEvent::Done { .. })),
            "expected Done last, got: {:?}",
            events.last()
        );

        // Trailing "should not be reached" assistant text never made it
        // to the rail (the loop ended before the third LLM call).
        let saw_unreachable = events
            .iter()
            .any(|e| matches!(e, WizardEvent::Token { text } if text.contains("should not be reached")));
        assert!(
            !saw_unreachable,
            "loop continued past the streak cap: {events:#?}"
        );
    }

    #[tokio::test]
    async fn different_error_classes_do_not_trip_the_guard() {
        // Regression guard: the retry-budget only fires when the SAME
        // tool returns the SAME error class twice in a row. Two
        // different bad ids (different InvalidJobId.provided ⇒ different
        // signature) must NOT force-end the turn — that would be
        // over-eager and break legitimate exploration where the model
        // tries a few different ids in a row.
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "get_cli_job",
                serde_json::json!({"job_id": "eval_run_AAAAAAAAAAAAAAAAAAAA"}),
            ),
            MockDispatch::tool_use(
                "tu_2",
                "get_cli_job",
                serde_json::json!({"job_id": "strategy_BBBBBBBBBBBBBBBBBBBB"}),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "giving up".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, _td, _sid) = loop_with_session(mock, "find it", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;

        // No stuck card — different error classes shouldn't trip it.
        let stuck = events.iter().any(|e| matches!(
            e,
            WizardEvent::ContentBlock { block }
                if block.get("action_id").and_then(|v| v.as_str()).map(|s| s.starts_with("tool-loop-break:")).unwrap_or(false)
        ));
        assert!(!stuck, "guard fired on two DIFFERENT error classes: {events:#?}");

        // Final assistant text reached the rail — loop continued.
        let saw_giving_up = events
            .iter()
            .any(|e| matches!(e, WizardEvent::Token { text } if text.contains("giving up")));
        assert!(
            saw_giving_up,
            "expected loop to continue to final turn: {events:#?}"
        );
    }
}
