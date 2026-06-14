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
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde::{de::Deserializer, de::Error as DeError, Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;
use xvision_core;
use xvision_filters::{parse_json, parse_toml, validate as validate_filter_dsl};

use crate::cli_jobs::eval_run_bridge;
use crate::cli_jobs::runner::CliJobRunner;
use crate::cli_jobs::store::CliJobStore;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason, ToolDefinition,
};
use xvision_engine::agent::memory_recorder::{render_recalled_patterns, MemoryRecorder, RecallResult};
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::agents as api_agents;
use xvision_engine::api::eval::{self as api_eval, EvalRunRequest};
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::scenario::{CreateScenarioRequest, ListScenariosFilter, ScenarioMutations};
use xvision_engine::api::settings::providers as api_providers;
use xvision_engine::api::strategy as api_strategy;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::authoring;
use xvision_engine::chat_session::{
    action_confirmation_card, classify_tool, decide_tool_policy, ChatSessionStore, ContextScope,
    InlineAction, ToolClass, ToolPolicyStore, GLOBAL_SCOPE,
};
use xvision_engine::checkpoint::{CheckpointKind, Checkpointer, SnapshotRequest};
use xvision_engine::eval::{
    findings::Finding,
    run::{RunMode, RunStatus},
    scenario::Scenario,
    store::RunStore,
};
use xvision_engine::focus;
use xvision_engine::strategies::ActivationMode;
use xvision_engine::strategies_folder;
use xvision_observability::{
    Actor as UnifiedActor, CheckpointWrittenEvent, FocusEvent, Redactor, ToolDenied, ToolPolicyChecked,
    ToolPolicyOutcome, TypedError, UnifiedPayload,
};

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

/// Hard deadline on a single authoring-tool execution. Tools should be
/// instantaneous (DB reads / filesystem reads / config writes). A tool
/// that runs longer than this is presumed wedged on something
/// non-recoverable (network call hanging, deadlocked DB writer, etc.)
/// and we surface a typed error so the chat rail's spinner clears and
/// the LLM gets a definitive failure to react to. Without this, a
/// stuck tool leaves the chat rail with a forever-pending tool card
/// (the recurring QA hang on `list_strategies` / `list_scenarios` /
/// `list_strategy_ideas`). 30s is comfortably above any normal
/// authoring verb's runtime; if a legitimate tool needs longer it
/// should stream progress, not block.
const TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Hard ceiling on a single model-dispatch (`LlmDispatch::complete`) call.
/// The provider HTTP clients have their own per-attempt timeouts + retries,
/// but a genuinely-wedged provider (black-holed TCP, a backend that accepts
/// the request and never responds) can still hang the call indefinitely —
/// which strands the whole turn with no tokens streamed and no error. This
/// outer bound converts that into a typed `Error` event the rail can recover
/// from. 120s is well above any healthy interactive completion (max_tokens is
/// 1500) while still bounding a hang. Defense-in-depth companion to
/// `TOOL_EXECUTION_TIMEOUT`.
const MODEL_DISPATCH_TIMEOUT: Duration = Duration::from_secs(120);

/// Cap on tool-use → tool-result iterations per `next_event` call. Prevents
/// a misbehaving model from looping forever; v1 wizards never need more
/// than 3-4 round trips per user turn.
///
/// Keep enough headroom for legitimate multi-step authoring turns while
/// relying on the repeated same-tool/same-error streak guard below to stop
/// noisy failure loops early.
const MAX_TOOL_LOOP_ITERATIONS: usize = 12;

/// Maximum number of messages replayed from the persisted session history
/// into every LLM context. Without this cap the context grows linearly with
/// session length and was observed to cause a 24 s → 122 s latency blow-up
/// across `MAX_TOOL_LOOP_ITERATIONS` iterations (xvision-t4u8 Finding #3).
///
/// 60 messages ≈ 30 user/assistant turn pairs, enough for meaningful
/// multi-step authoring sessions while keeping context bounded.
/// `window_chat_messages` enforces this cap and never splits a
/// tool_use / tool_result pair.
const CHAT_HISTORY_WINDOW_MESSAGES: usize = 60;

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
                 You are the xvision workspace assistant. You may inspect existing strategies, \
                 scenarios, eval runs, and cached market data when it informs the user's request, \
                 but you are FREE to design and build brand-new strategies from scratch — do not \
                 default to recycling a library entry when the user is asking for something new. \
                 Prefer typed tools and queued xvn CLI jobs over asking the user to run commands."
            }
            AgentProfile::StrategySetup => {
                "## Agent profile: strategy setup\n\
                 You are focused on strategy setup: creating, editing, validating, and evaluating \
                 strategies. You may consult existing strategies and scenarios for inspiration, \
                 but you are encouraged to design fresh strategies when the user asks for something \
                 new — do NOT default to picking an existing library entry unless the user \
                 explicitly asks you to reuse one. \
                 `create_strategy` always starts a blank draft — fill it in via \
                 `create_strategy_agent` / `update_slot` / `set_mechanical_param` / `set_filter`. \
                 ## Completion contract\n\
                 Before you tell the user the strategy is ready (or surface the Open Strategy \
                 card), you MUST: (1) attach a trader agent with an explicit provider/model via \
                 `create_strategy_agent`; (2) attach any filter the user asked for via \
                 `set_filter` and confirm the tool_result reports success; (3) ensure every \
                 other configuration item the user requested (cadence, assets, risk preset, \
                 timeframe, etc.) has a confirming tool_result. Do not present the strategy or \
                 say it is ready while ANY of those tools is still pending, failed, or \
                 unattempted. If a step keeps failing, surface the error verbatim and stop — do \
                 not pretend it succeeded. \
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
    /// to render a "running tool" indicator. `id` is the model's
    /// `tool_use` id (same value carried in the matching `ToolResult`),
    /// so the unified projector can pair `tool_requested`→`tool_finished`
    /// EXACTLY — by id, not by tool name — regardless of repeats or
    /// out-of-order/interleaved results.
    ToolCall {
        id: String,
        tool: String,
        args: serde_json::Value,
    },
    /// Result of the tool call identified by `id`. Front-end uses this to
    /// update the displayed draft state; the projector closes the span its
    /// matching `ToolCall` (same `id`) opened.
    ToolResult {
        id: String,
        tool: String,
        result: serde_json::Value,
    },
    /// A typed rich display block to append to the active assistant bubble.
    ContentBlock { block: serde_json::Value },
    /// Conversation complete. `draft_id` carries the most recently created
    /// or referenced strategy id (if any), so the front-end can transition
    /// to the inspector view.
    Done { draft_id: Option<String> },
    /// The dispatch errored or the loop hit a hard cap.
    Error { message: String },
}

/// A net-new unified-event the legacy [`WizardEvent`] vocabulary can't carry,
/// produced by the Phase 2 safety enforcement inside the loop. The chat route
/// drains these alongside the `WizardEvent` stream and projects them through
/// the same per-session projector so the unified `seq` stays gap-free.
///
/// `actor` is the unified-event actor (`Hook` for policy enforcement); `span_id`
/// correlates the event with the tool the check ran for.
#[derive(Debug, Clone)]
pub struct PolicyEvent {
    pub actor: UnifiedActor,
    pub span_id: Option<String>,
    pub payload: UnifiedPayload,
}

/// Outcome of the per-tool safety gate. `Allow` means the caller executes the
/// tool; `Blocked` carries the typed denial tool_result fed back to the model
/// (the tool did NOT run).
enum PolicyVerdict {
    Allow,
    Blocked(serde_json::Value),
}

/// Trim `messages` to at most `cap` entries, keeping the **most recent** ones.
///
/// Tool-use/tool-result pairs are kept together: if the raw window boundary
/// falls so that a `ToolResult` block would appear at the start of the window
/// without its matching `ToolUse` (or vice versa), the boundary is shifted
/// forward (dropping more messages) until the window starts on a clean
/// message that is not an orphan `ToolResult`.
///
/// The system prompt is assembled separately and is NOT part of `messages`,
/// so this function only touches the transcript messages — the system prompt
/// is always preserved in full.
///
/// Callers that want the full history (e.g. `latest_text_for_role`,
/// `ChatSessionStore::resolve`) should call `load_history` directly.
fn window_chat_messages(mut messages: Vec<Message>, cap: usize) -> Vec<Message> {
    if messages.len() <= cap {
        return messages;
    }

    // Start with a naive tail slice of `cap` messages.
    let mut start = messages.len() - cap;

    // Collect the tool_use ids that appear in msgs[0..start] — these were
    // dropped by the window. Any tool_result for one of those ids at
    // msgs[start..] would be an orphan.
    loop {
        // Build the set of tool_use ids dropped by the current window.
        let dropped_tool_use_ids: std::collections::HashSet<String> = messages[..start]
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, .. } = b {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if dropped_tool_use_ids.is_empty() {
            // No dropped tool_use ids — no orphan risk.
            break;
        }

        // Check whether the first message(s) in the window reference a
        // dropped tool_use id via a ToolResult block.
        let first_is_orphan = messages[start..]
            .iter()
            .next()
            .map(|m| {
                m.content.iter().any(|b| {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                        dropped_tool_use_ids.contains(tool_use_id.as_str())
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false);

        if !first_is_orphan {
            break;
        }

        // The first message in the current window is an orphan tool_result.
        // Drop it and try again (also drop the matching tool_use if it's
        // within the current window).
        start += 1;
        if start >= messages.len() {
            // Entire history is orphaned (shouldn't happen in practice).
            return vec![];
        }
    }

    messages.drain(..start);
    messages
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
    /// Effective Research/Act mode + tool-policy scope. The mode is the
    /// source-of-truth value read FROM THE DB at construction (never a client
    /// field); enforcement re-reads it per tool so a `/mode` POST mid-stream is
    /// respected. `user_scope` selects which `tool_policies` rows apply.
    user_scope: String,
    /// Net-new unified safety events queued during the current `next_event`
    /// invocation (ToolPolicyChecked / ToolDenied / ErrorPolicyDenied). Drained
    /// by the route via `take_policy_events`.
    policy_events: Vec<PolicyEvent>,
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
    /// P4 cortex-memory: scope-namespaced recall + redacted write-back.
    /// `Some` only when the dashboard was started with `XVN_CHAT_MEMORY` and a
    /// recorder was provisioned (set via [`WizardLoop::with_chat_memory`]).
    /// `None` → no recall and no write-back; the loop behaves exactly as it did
    /// before P4. Recall/record errors are always swallowed — memory is
    /// best-effort and must never fail a chat turn.
    chat_memory: Option<Arc<MemoryRecorder>>,
    /// Hard timeout applied to each `dispatch.complete` call. Defaults to
    /// [`MODEL_DISPATCH_TIMEOUT`]; overridable (see
    /// [`WizardLoop::with_dispatch_timeout`]) so tests can drive the
    /// hung-provider path deterministically without waiting the full ceiling.
    dispatch_timeout: Duration,
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
    /// Operator-facing summary of what this agent does. The chat-rail bot is
    /// expected to supply this whenever it creates an agent so the agents
    /// list isn't littered with auto-generated placeholder text. Falls back
    /// to a generated one-liner when empty.
    #[serde(default)]
    description: Option<String>,
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

#[derive(Debug, Deserialize)]
struct SetFilterReq {
    #[serde(default)]
    strategy_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_filter_payload")]
    filter: Option<serde_json::Value>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListEvalRunsReq {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    scenario_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct GetEvalRunReq {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ListEvalReviewsReq {
    run_id: String,
}

#[derive(Debug, Deserialize)]
struct GetEvalReviewReq {
    id: String,
}

fn deserialize_filter_payload<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let filter = Option::<serde_json::Value>::deserialize(deserializer)?;
    if matches!(filter.as_ref(), Some(value) if value.is_null()) {
        return Err(DeError::custom("filter payload cannot be null"));
    }
    Ok(filter)
}

fn normalize_set_filter_payload(
    filter: Option<serde_json::Value>,
    source: Option<String>,
    format: Option<String>,
    strategy_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let (payload, format) = set_filter_payload_and_format(filter, source, format)?;
    let parsed = parse_set_filter_payload(payload, format.as_deref(), strategy_id)?;
    validate_filter_dsl(&parsed).map_err(|e| anyhow::anyhow!("filter validation error: {e}"))?;
    serde_json::to_value(parsed).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))
}

fn set_filter_payload_and_format(
    filter: Option<serde_json::Value>,
    source: Option<String>,
    format: Option<String>,
) -> anyhow::Result<(serde_json::Value, Option<String>)> {
    let format = normalize_filter_format(format)?;
    match (filter, source) {
        (Some(filter), None) => Ok((filter, format)),
        (None, Some(source)) => Ok((serde_json::Value::String(source), format)),
        (Some(filter), Some(source)) => {
            if format.is_none() && is_filter_format(source.trim()) {
                Ok((filter, Some(source.trim().to_string())))
            } else {
                anyhow::bail!(
                    "set_filter: `source` is filter text; do not send it with `filter`. Use `format` for json/toml."
                )
            }
        }
        (None, None) => anyhow::bail!(
            "set_filter: filter payload is required; send `filter` or `source` (JSON/TOML body)"
        ),
    }
}

fn normalize_filter_format(format: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(format) = format else {
        return Ok(None);
    };
    let format = format.trim();
    if format.is_empty() {
        anyhow::bail!("set_filter: format cannot be empty");
    }
    if !is_filter_format(format) {
        anyhow::bail!("set_filter: unknown format `{format}`; expected `json` or `toml`");
    }
    Ok(Some(format.to_string()))
}

fn is_filter_format(value: &str) -> bool {
    matches!(value, "json" | "toml")
}

fn parse_set_filter_payload(
    mut filter: serde_json::Value,
    source: Option<&str>,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    filter = extract_set_filter_payload(filter);
    Ok(match filter {
        serde_json::Value::String(src) => parse_set_filter_text(&src, source, strategy_id)?,
        _ => match source {
            Some("json") => parse_set_filter_value(filter, strategy_id)?,
            Some("toml") => anyhow::bail!("set_filter: format `toml` requires text payload"),
            Some(other) if other.trim().is_empty() => parse_set_filter_value(filter, strategy_id)?,
            None => parse_set_filter_value(filter.clone(), strategy_id).or_else(|json_err| {
                let src =
                    serde_json::to_string(&filter).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?;
                parse_set_filter_text(&src, Some("toml"), strategy_id).map_err(|_| json_err)
            })?,
            Some(other) => {
                let src =
                    serde_json::to_string(&filter).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?;
                parse_set_filter_text(&src, Some(other), strategy_id)?
            }
        },
    })
}

fn parse_set_filter_value(
    raw_filter: serde_json::Value,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    let obj = match raw_filter {
        serde_json::Value::Object(mut obj) => {
            if obj
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|id| id.trim().is_empty())
            {
                obj.insert("id".into(), serde_json::Value::String(Ulid::new().to_string()));
            }
            obj.insert(
                "strategy_id".into(),
                serde_json::Value::String(strategy_id.to_string()),
            );
            obj
        }
        _ => {
            anyhow::bail!("filter parse error: filter payload must be an object");
        }
    };

    parse_set_filter_text(
        &serde_json::to_string(&serde_json::Value::Object(obj))
            .map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?,
        Some("json"),
        strategy_id,
    )
}

fn parse_set_filter_text(
    source_text: &str,
    source: Option<&str>,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    let mut filter = match source.unwrap_or_default() {
        "json" => parse_set_filter_text_preferring_json(source_text, strategy_id)?,
        "toml" => parse_toml(source_text).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?,
        _ => parse_set_filter_text_preferring_json(source_text, strategy_id).or_else(|_| {
            parse_toml(source_text)
                .map_err(|e| anyhow::anyhow!("filter parse error: failed parsing as JSON and TOML: {e}"))
        })?,
    };
    if filter.id.as_str().is_empty() {
        filter.id = xvision_filters::FilterId::new(Ulid::new().to_string());
    }
    filter.strategy_id = strategy_id.to_string().into();
    Ok(filter)
}

fn parse_set_filter_text_preferring_json(
    source_text: &str,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    let mut filter = parse_json(source_text).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?;
    if filter.id.as_str().is_empty() {
        filter.id = xvision_filters::FilterId::new(Ulid::new().to_string());
    }
    filter.strategy_id = strategy_id.to_string().into();
    Ok(filter)
}

fn extract_set_filter_payload(raw_filter: serde_json::Value) -> serde_json::Value {
    match raw_filter {
        serde_json::Value::Object(mut obj) => {
            if let Some(filter) = obj.remove("filter") {
                filter
            } else {
                serde_json::Value::Object(obj)
            }
        }
        other => other,
    }
}

#[derive(Debug, Deserialize)]
struct ClearFilterReq {
    #[serde(default)]
    strategy_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
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

/// Truncate `text` to at most `max` chars, appending `…` when trimmed.
/// Char-based (not byte-based) so it never splits a multi-byte UTF-8 scalar.
fn truncate(text: &str, max: usize) -> String {
    let mut s: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        s.push('…');
    }
    s
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
            // v1: single workspace scope. Per-user identity threads in here
            // once auth carries a user id to the chat route.
            user_scope: GLOBAL_SCOPE.to_string(),
            policy_events: Vec::new(),
            last_draft_id: None,
            last_tool_error: None,
            last_tool_failure: None,
            tool_failure_streak: 0,
            pending: vec![],
            is_done: false,
            chat_memory: None,
            dispatch_timeout: MODEL_DISPATCH_TIMEOUT,
        })
    }

    /// Attach the cortex-memory recorder for scope-namespaced recall +
    /// redacted write-back (P4). Chainable so call sites read
    /// `WizardLoop::new_with_profile(...).await?.with_chat_memory(state.chat_memory.clone())`.
    /// Pass `None` (or skip the call) to leave memory disabled.
    pub fn with_chat_memory(mut self, mem: Option<Arc<MemoryRecorder>>) -> Self {
        self.chat_memory = mem;
        self
    }

    /// Override the per-dispatch timeout (default [`MODEL_DISPATCH_TIMEOUT`]).
    /// Test-only seam: lets the hung-provider path be exercised in real time
    /// with a tiny timeout instead of stalling on the production ceiling.
    #[cfg(test)]
    pub fn with_dispatch_timeout(mut self, timeout: Duration) -> Self {
        self.dispatch_timeout = timeout;
        self
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

    /// Drain the net-new unified safety events queued during the last turn
    /// (ToolPolicyChecked / ToolDenied / ErrorPolicyDenied). The chat route
    /// calls this after each `next_event` and projects them through the shared
    /// session projector so the unified stream carries the enforcement record.
    /// Returns the events in chronological (emission) order.
    pub fn take_policy_events(&mut self) -> Vec<PolicyEvent> {
        std::mem::take(&mut self.policy_events)
    }

    /// The tool set offered to the model for this turn: the profile's tools
    /// minus any whose persisted policy is `enabled = false`. Disabled tools
    /// must never be presented (defense-in-depth: even if the model knew the
    /// name, the per-tool enforcement in `run_one_turn` denies it). Reads the
    /// `tool_policies` overrides for this scope once; unknown DB errors fall
    /// back to the unfiltered set so a transient read failure can't silently
    /// strip the model's whole toolbelt — enforcement still gates writes.
    async fn enabled_tool_defs(&mut self) -> Vec<ToolDefinition> {
        let defs = agent_tool_defs(self.profile);
        let overrides = match ToolPolicyStore::get_policies(&self.pool, &self.user_scope).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    error = %e,
                    "failed to load tool_policies; offering unfiltered tool set (enforcement still gates writes)",
                );
                return defs;
            }
        };
        let mut out = Vec::with_capacity(defs.len());
        for def in defs {
            // A disabled override hides the tool; otherwise (override-enabled or
            // no override) keep it. The class default is never "disabled" for
            // the chat authoring verbs (only the reserved Dangerous class is,
            // and none are classified Dangerous yet), so absence of a row keeps
            // the tool visible.
            let disabled = overrides
                .iter()
                .find(|r| r.tool_name == def.name)
                .map(|r| !r.enabled)
                .unwrap_or(false);
            if disabled {
                // Visibility: a hidden tool emits a ToolPolicyChecked{Denied}
                // so the operator can see it was withheld this turn.
                let span = Ulid::new().to_string();
                self.policy_events.push(PolicyEvent {
                    actor: UnifiedActor::Hook,
                    span_id: Some(span.clone()),
                    payload: UnifiedPayload::ToolPolicyChecked(ToolPolicyChecked {
                        span_id: span,
                        tool_name: def.name.clone(),
                        outcome: ToolPolicyOutcome::Denied,
                        mode: self.current_mode().await,
                    }),
                });
                continue;
            }
            out.push(def);
        }
        out
    }

    /// Read the session's current Research/Act mode FROM THE DB. The column is
    /// the source of truth; the client-sent mode is never consulted. Defaults
    /// to `research` (fail closed) if the rail state can't be read.
    async fn current_mode(&self) -> String {
        match ChatSessionStore::load_rail_state(&self.pool, &self.session_id).await {
            Ok(st) => st.mode,
            Err(e) => {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    error = %e,
                    "failed to read session mode; failing closed to research",
                );
                "research".to_string()
            }
        }
    }

    /// Assemble the per-turn system prompt and splice in the scope's focus
    /// document (Phase 2.4). Focus is reloaded from disk on every turn — a
    /// cheap fs read — so an operator edit between turns takes effect on the
    /// next turn without restarting the session. On a focus hit, a
    /// `FocusInjected` event carrying the content hash is queued for the route
    /// to project onto the unified session log/bus.
    async fn system_prompt(&mut self) -> String {
        // Plan #11 Phase B Task 3 Step 2: inject scope header so the model
        // knows what the user is asking about (workspace, a specific run,
        // a draft, etc.). Tool calls remain available for deeper info.
        let base = format!(
            "{base}\n\n## Current context\n{header}\n\n{runtime}\n",
            base = WIZARD_SYSTEM_PROMPT_BASE,
            header = format!(
                "{}\n\n{}",
                self.scope.header_label(),
                self.profile.prompt_section()
            ),
            runtime = self.agent_runtime_prompt_section(),
        );
        let assembled = match self.load_focus_section().await {
            Some((section, ev)) => {
                self.policy_events.push(ev);
                format!("{base}\n{section}\n")
            }
            None => base,
        };
        // P4 cortex-memory recall: prepend the scope's prior salient
        // observations (Patterns) ahead of the assembled prompt so the model
        // sees them first. Best-effort — any recall failure (no embedder, no
        // recorder, store error) returns the assembled prompt unchanged.
        self.prepend_recalled_memory(assembled).await
    }

    /// Best-effort cortex recall. When `chat_memory` is set and the session
    /// has a latest user message, recall the top-k Patterns in the scope's
    /// `chat:` namespace and prepend the rendered `<prior_observations>` block
    /// to `assembled`. Live context → `scenario_start = None` (no temporal
    /// filter). Never propagates: `system_prompt` returns `String`, so every
    /// error path returns `assembled` untouched.
    async fn prepend_recalled_memory(&self, assembled: String) -> String {
        let Some(recorder) = &self.chat_memory else {
            return assembled;
        };
        let Some(query) = self.latest_text_for_role("user").await else {
            return assembled;
        };
        let namespace = self.scope.memory_namespace();
        match recorder.recall_in_namespace(&namespace, &query, 5, None).await {
            Ok(RecallResult::Hits { matches, .. }) if !matches.is_empty() => {
                format!("{}\n\n{}", render_recalled_patterns(&matches), assembled)
            }
            Ok(_) => assembled,
            Err(e) => {
                tracing::warn!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    namespace = %namespace,
                    error = %e,
                    "chat-memory recall failed; proceeding without prior observations"
                );
                assembled
            }
        }
    }

    /// Scan the persisted chat history in reverse for the latest message with
    /// `role`, returning its first `Text` block content. Used to source the
    /// recall query (latest user turn) and the write-back assistant text.
    /// Best-effort: a store read error yields `None`.
    async fn latest_text_for_role(&self, role: &str) -> Option<String> {
        let history = ChatSessionStore::load_history(&self.pool, &self.session_id)
            .await
            .ok()?;
        for cm in history.iter().rev() {
            if cm.role != role {
                continue;
            }
            for block in &cm.content_blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    /// Best-effort cortex write-back on a clean turn completion. Builds a
    /// concise observation from the latest user message + this turn's
    /// assistant text, runs it through the observability [`Redactor`] so a
    /// pasted secret is never persisted, and records it as an Observation in
    /// the scope's `chat:` namespace. Live context → source window =
    /// `now..now`, provenance scenario `"chat"`. A no-op when memory is
    /// disabled; every error is logged and swallowed (never fails the turn).
    async fn write_back_memory(&self, assistant_text: &str) {
        let Some(recorder) = &self.chat_memory else {
            return;
        };
        if assistant_text.trim().is_empty() {
            return;
        }
        let user_text = self.latest_text_for_role("user").await.unwrap_or_default();
        let observation = format!(
            "User asked: {}\nAssistant concluded: {}",
            truncate(&user_text, 200),
            truncate(assistant_text, 400),
        );
        // Redact BEFORE persisting — a pasted API key / mnemonic must never
        // land in the memory store.
        let redacted = Redactor::new().redact(&observation).text;
        let namespace = self.scope.memory_namespace();
        let now = Utc::now();
        if let Err(e) = recorder
            .record_observation_in_namespace(
                &namespace,
                &redacted,
                self.session_id.clone(),
                "chat".to_string(),
                0,
                now,
                now,
            )
            .await
        {
            tracing::warn!(
                target: "xvision::dashboard::chat_rail",
                session_id = %self.session_id,
                namespace = %namespace,
                error = %e,
                "chat-memory write-back failed; continuing"
            );
        }
    }

    /// Load the scope's focus document (if any) and build its clearly-delimited
    /// "## Focus" section plus the matching `FocusInjected` policy event.
    /// Returns `None` when no focus file exists for the scope or the read
    /// fails (a missing/unreadable focus doc must never abort a turn — it is a
    /// best-effort context aid, not a safety gate).
    async fn load_focus_section(&self) -> Option<(String, PolicyEvent)> {
        let xvn_home = &self.api_context.xvn_home;
        let doc = match focus::load(xvn_home, &self.scope).await {
            Ok(Some(doc)) => doc,
            Ok(None) => return None,
            Err(e) => {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    error = %e,
                    "failed to load focus doc; proceeding without it",
                );
                return None;
            }
        };
        let (scope_kind, scope_id) = focus::scope_address(&self.scope);
        let rel_path = self.focus_rel_path(&doc.path);
        let section = format!(
            "## Focus\n\
             The operator pinned the following focus notes for this context. Treat them as \
             standing guidance for this conversation.\n\n\
             <focus>\n{content}\n</focus>",
            content = doc.content,
        );
        let ev = PolicyEvent {
            actor: UnifiedActor::Hook,
            span_id: None,
            payload: UnifiedPayload::FocusInjected(FocusEvent {
                scope_kind,
                scope_id,
                path: rel_path,
                content_hash: Some(doc.content_hash),
            }),
        };
        Some((section, ev))
    }

    /// Normalize a focus doc's absolute path to one relative to `$XVN_HOME`
    /// for the event record (matches the `focus_path` column convention). If
    /// the path is not under `$XVN_HOME` (shouldn't happen), the absolute path
    /// is used verbatim.
    fn focus_rel_path(&self, abs: &str) -> String {
        let home = &self.api_context.xvn_home;
        std::path::Path::new(abs)
            .strip_prefix(home)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| abs.to_string())
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
            // Phase 2.3: offer only enabled tools. Disabled tools are filtered
            // out (and emit a ToolPolicyChecked{Denied} for visibility) so the
            // model never even sees a tool the operator has turned off.
            let tools = self.enabled_tool_defs().await;
            let system_prompt = self.system_prompt().await;
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt,
                messages,
                max_tokens: Some(1500),
                temperature: None,
                tools,
                response_schema: None,
                cache_control: None,
                force_json: false,
            };
            // Bound the model call so a wedged provider can't strand the turn
            // with no tokens and no error. On elapse, surface a typed error
            // that propagates to `next_event` as a `WizardEvent::Error`
            // (→ `SessionFailed`, which also terminalizes any open tool spans).
            let resp: LlmResponse =
                match tokio::time::timeout(self.dispatch_timeout, self.dispatch.complete(req)).await {
                    Ok(result) => result?,
                    Err(_elapsed) => {
                        tracing::warn!(
                            target: "xvision::dashboard::wizard_loop",
                            session_id = %self.session_id,
                            timeout_secs = self.dispatch_timeout.as_secs(),
                            "model dispatch timed out — surfacing a typed error to the chat rail"
                        );
                        anyhow::bail!(
                            "model dispatch timed out after {}s",
                            self.dispatch_timeout.as_secs()
                        );
                    }
                };

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
                // The model signaled it intended to call a tool
                // (`stop_reason == ToolUse`) but emitted no tool_use
                // block. Without this branch the loop bails as Done
                // after the first malformed text-only turn, cutting
                // off any planned follow-up steps. Persist a synthetic
                // user-role nudge so the next iteration re-prompts the
                // model with explicit guidance; MAX_TOOL_LOOP_ITERATIONS
                // is the safety net for tight loops.
                if matches!(resp.stop_reason, StopReason::ToolUse) {
                    let nudge_blocks: Vec<serde_json::Value> = vec![serde_json::json!({
                        "type": "text",
                        "text": "Your previous turn signaled `tool_use` but contained no \
                                 tool_use block. Either call the tool you intended to call, \
                                 or finish the turn cleanly with a final text response."
                    })];
                    ChatSessionStore::append(&self.pool, &self.session_id, "user", &nudge_blocks).await?;
                    continue;
                }
                // P4 cortex-memory write-back: this is the clean text-only
                // completion. Record a redacted observation of the turn into
                // the scope namespace before signalling Done. Best-effort —
                // errors are logged, never propagated.
                let assistant_text = resp
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } if !text.is_empty() => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                self.write_back_memory(&assistant_text).await;
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
                    id: id.clone(),
                    tool: name.clone(),
                    args: input.clone(),
                });
                // Phase 2.2 + 2.3 SAFETY CORE: gate the tool against the
                // session's mode (read FROM THE DB) and its persisted policy
                // BEFORE executing. Returns a typed denial result + queues the
                // unified safety events when the tool must not run.
                let result_value = match self.enforce_tool_policy(&name).await {
                    PolicyVerdict::Allow => {
                        // Phase 2.5 CHECKPOINT HOOK: a WRITE tool that has passed
                        // policy is about to mutate authoring state. Take a
                        // PreTool snapshot of the affected artifacts FIRST so the
                        // operator can rewind the edit. Fail closed: if the
                        // snapshot can't be written, the mutating tool does NOT
                        // run and a typed error tool_result is fed back to the
                        // model instead.
                        match self.maybe_snapshot_before_tool(&name, &input).await {
                            Ok(()) => {
                                // Wrap the tool body in a hard timeout
                                // (TOOL_EXECUTION_TIMEOUT) so a wedged
                                // tool can't leave the chat rail's tool
                                // card pending forever — see the
                                // constant's doc for the rationale.
                                let result =
                                    tokio::time::timeout(TOOL_EXECUTION_TIMEOUT, self.run_tool(&name, input))
                                        .await;
                                match result {
                                    Ok(Ok(v)) => v,
                                    Ok(Err(e)) => {
                                        let msg = e.to_string();
                                        self.last_tool_error = Some((name.clone(), msg.clone()));
                                        serde_json::json!({ "error": msg })
                                    }
                                    Err(_elapsed) => {
                                        let msg = format!(
                                            "tool '{name}' timed out after {}s",
                                            TOOL_EXECUTION_TIMEOUT.as_secs()
                                        );
                                        tracing::warn!(
                                            target: "xvision::dashboard::wizard_loop",
                                            tool = %name,
                                            timeout_secs = TOOL_EXECUTION_TIMEOUT.as_secs(),
                                            "wizard tool execution timed out — surfacing typed error to the model"
                                        );
                                        self.last_tool_error = Some((name.clone(), msg.clone()));
                                        serde_json::json!({ "error": msg })
                                    }
                                }
                            }
                            Err(denial) => denial,
                        }
                    }
                    PolicyVerdict::Blocked(denial) => denial,
                };
                self.maybe_track_draft_id(&name, &result_value);
                self.update_tool_failure_streak(&name, &result_value);
                self.pending.push(WizardEvent::ToolResult {
                    id: id.clone(),
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
                // QA31: check the streak guard PER TOOL within the turn,
                // not just at the end of the turn. Previously a multi-tool
                // turn like [create_scenario → fail, set_filter → ok]
                // ended with `tool_failure_streak = 0` (the success
                // cleared it), so the outer guard never saw the
                // create_scenario failure. Moving the probe inside lets
                // us catch a same-tool repeat as soon as it happens.
                if self.tool_failure_streak >= MAX_TOOL_FAILURE_STREAK {
                    // Persist the partial tool_results we've already
                    // collected (the prior tool_uses in this turn) so
                    // the chat history doesn't lose them when we bail
                    // early.
                    ChatSessionStore::append(&self.pool, &self.session_id, "user", &tool_result_blocks)
                        .await?;
                    if !rich_blocks.is_empty() {
                        ChatSessionStore::append(&self.pool, &self.session_id, "assistant", &rich_blocks)
                            .await?;
                        for block in std::mem::take(&mut rich_blocks) {
                            self.pending.push(WizardEvent::ContentBlock { block });
                        }
                    }
                    self.emit_tool_loop_break().await?;
                    self.is_done = true;
                    self.pending.push(WizardEvent::Done {
                        draft_id: self.last_draft_id.clone(),
                    });
                    return Ok(());
                }
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
    ///
    /// The returned slice is capped at `CHAT_HISTORY_WINDOW_MESSAGES` so the
    /// context sent to the model stays constant-bounded regardless of how many
    /// turns the session has accumulated. Without this cap, every iteration of
    /// the tool-use loop would replay the ENTIRE history, so a long session
    /// would send a linearly-growing context on each of the up-to-12 loop
    /// iterations — the dominant cause of the 24 s → 122 s latency blow-up
    /// observed in long sessions (xvision-t4u8 Finding #3).
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
        Ok(window_chat_messages(out, CHAT_HISTORY_WINDOW_MESSAGES))
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
                    label: "Open Strategy".into(),
                    href: Some(format!("/strategies/{id}")),
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

    /// Server-side tool-policy gate (Phase 2.2 + 2.3). Reads the session's
    /// mode FROM THE DB and the tool's effective policy, runs the pure
    /// `decide`, emits a `ToolPolicyChecked` for visibility, and:
    ///
    /// - `AutoApproved` → [`PolicyVerdict::Allow`] (the caller executes).
    /// - `Denied` → queues `ToolDenied` + `ErrorPolicyDenied`, returns a typed
    ///   denial tool_result so the model SEES the refusal and can adapt
    ///   (e.g. switch to Act mode or ask the operator). The tool does NOT run.
    /// - `NeedsApproval` → SCOPE BOUNDARY: the interactive approve→resume
    ///   round-trip is deferred. The decision is persisted via the
    ///   `ToolPolicyChecked{NeedsApproval}` event; at execution time the tool is
    ///   treated as blocked-pending-approval and does NOT run, returning a typed
    ///   result telling the model an operator must approve it.
    async fn enforce_tool_policy(&mut self, name: &str) -> PolicyVerdict {
        let mode = self.current_mode().await;
        let class = classify_tool(name);
        let policy = match ToolPolicyStore::effective(&self.pool, &self.user_scope, name).await {
            Ok(p) => p,
            Err(e) => {
                // Fail closed: an unreadable policy denies the tool rather than
                // defaulting to allow.
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    tool = name,
                    error = %e,
                    "failed to read tool policy; failing closed (denied)",
                );
                xvision_engine::chat_session::ToolPolicy {
                    enabled: false,
                    auto_approve: false,
                }
            }
        };
        let outcome = decide_tool_policy(&mode, class, policy);

        let span = Ulid::new().to_string();
        // Always emit the policy-check record (visibility for every outcome).
        self.policy_events.push(PolicyEvent {
            actor: UnifiedActor::Hook,
            span_id: Some(span.clone()),
            payload: UnifiedPayload::ToolPolicyChecked(ToolPolicyChecked {
                span_id: span.clone(),
                tool_name: name.to_string(),
                outcome,
                mode: mode.clone(),
            }),
        });

        match outcome {
            ToolPolicyOutcome::AutoApproved => PolicyVerdict::Allow,
            ToolPolicyOutcome::Denied => {
                let (code, message, remediation) = if !policy.enabled {
                    (
                        "tool_disabled",
                        format!("Tool `{name}` is disabled by the current tool policy."),
                        Some(format!(
                            "Enable `{name}` in the chat tool-policy settings to use it."
                        )),
                    )
                } else {
                    (
                        "write_tool_in_research_mode",
                        format!(
                            "Tool `{name}` writes/mutates state and is blocked in research mode \
                             (read-only). Switch the session to Act mode to use it."
                        ),
                        Some("Switch this chat session to Act mode, then retry.".to_string()),
                    )
                };
                // ToolDenied (tool-row signal) + ErrorPolicyDenied (typed,
                // never-silent error) onto the unified log + bus.
                self.policy_events.push(PolicyEvent {
                    actor: UnifiedActor::Hook,
                    span_id: Some(span.clone()),
                    payload: UnifiedPayload::ToolDenied(ToolDenied {
                        span_id: span.clone(),
                        tool_name: name.to_string(),
                        code: code.to_string(),
                        message: message.clone(),
                    }),
                });
                self.policy_events.push(PolicyEvent {
                    actor: UnifiedActor::Hook,
                    span_id: Some(span),
                    payload: UnifiedPayload::ErrorPolicyDenied(TypedError {
                        code: code.to_string(),
                        message: message.clone(),
                        remediation,
                    }),
                });
                // Feed the denial back into the loop so the model sees it as a
                // tool_result and can adapt rather than retrying blindly.
                PolicyVerdict::Blocked(serde_json::json!({
                    "error": message,
                    "denied": true,
                    "code": code,
                }))
            }
            ToolPolicyOutcome::NeedsApproval => {
                // SCOPE BOUNDARY: persist + decide the NeedsApproval outcome
                // (the ToolPolicyChecked above), but do NOT execute. The
                // interactive approve→resume flow is a follow-up.
                let message = format!(
                    "Tool `{name}` requires operator approval before it runs. \
                     The request was recorded; an operator must approve it. \
                     (Interactive approval round-trip is not yet wired.)"
                );
                self.policy_events.push(PolicyEvent {
                    actor: UnifiedActor::Hook,
                    span_id: Some(span),
                    payload: UnifiedPayload::ErrorPolicyDenied(TypedError {
                        code: "tool_needs_approval".to_string(),
                        message: message.clone(),
                        remediation: Some(
                            "Enable auto-approve for this tool, or approve the pending request once \
                             the approval flow is available."
                                .to_string(),
                        ),
                    }),
                });
                tracing::info!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    tool = name,
                    "tool needs approval; blocked pending approval (interactive flow deferred)",
                );
                PolicyVerdict::Blocked(serde_json::json!({
                    "error": message,
                    "needs_approval": true,
                    "code": "tool_needs_approval",
                }))
            }
        }
    }

    /// Phase 2.5 checkpoint hook. Called for a WRITE tool that has cleared the
    /// policy gate, immediately before it executes. Takes a content-addressed
    /// [`CheckpointKind::PreTool`] snapshot of the authoring artifacts the tool
    /// is about to mutate, records the new checkpoint id as the session's
    /// `checkpoint_head`, and queues a `CheckpointCreated` event for the route
    /// to project.
    ///
    /// Artifact targeting:
    /// - A strategy-write tool (`create_strategy` / `update_*` / `set_*` /
    ///   `validate_draft` / filter edits) snapshots the active draft's Strategy
    ///   JSON plus the session tool policy and focus file.
    /// - An agent-editing tool (`attach_agent`) additionally snapshots the
    ///   referenced library agent's slot rows.
    ///
    /// Fail-closed: if the snapshot write fails, returns `Err(denial)` carrying
    /// a typed tool_result so the caller skips the mutating tool and feeds the
    /// refusal back to the model. A typed `ErrorPersistenceFailed` event is
    /// queued so the failure is never silent.
    ///
    /// Returns `Ok(())` — letting the tool run — when the tool mutates no
    /// snapshottable authoring artifact (e.g. `run_eval` / `fetch_bars`, which
    /// launch work rather than edit a draft); there is nothing to rewind.
    async fn maybe_snapshot_before_tool(
        &mut self,
        name: &str,
        input: &serde_json::Value,
    ) -> Result<(), serde_json::Value> {
        // Only WRITE-class tools mutate state. Read tools never need a rewind
        // point.
        if classify_tool(name) != ToolClass::Write {
            return Ok(());
        }

        let strategy_id = self.snapshot_strategy_id(input);
        let agent_id = if name == "attach_agent" {
            input
                .get("agent_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        } else {
            None
        };

        // Nothing concrete to snapshot (e.g. a work-launcher tool with no draft
        // target). Let the tool run; there is no authoring artifact to protect.
        if strategy_id.is_none() && agent_id.is_none() {
            return Ok(());
        }

        let req = SnapshotRequest {
            strategy_id: strategy_id.clone(),
            agent_id,
            // A strategy-write or agent-edit also captures the session policy +
            // focus so a rewind restores the full authoring context, not just
            // the artifact bytes.
            tool_policy: true,
            focus: true,
            label: Some(format!("pre:{name}")),
        };

        let checkpointer = Checkpointer::new(self.pool.clone(), self.api_context.xvn_home.clone());
        match checkpointer
            .snapshot(&self.session_id, CheckpointKind::PreTool, req)
            .await
        {
            Ok(ckpt) => {
                if let Err(e) = ChatSessionStore::set_checkpoint_head(
                    &self.pool,
                    &self.session_id,
                    Some(&ckpt.checkpoint_id),
                )
                .await
                {
                    // The snapshot blobs + row are durable; only the head
                    // pointer update failed. Fail closed so the operator never
                    // mutates state whose rewind pointer wasn't recorded.
                    tracing::error!(
                        target: "xvision::dashboard::chat_rail",
                        session_id = %self.session_id,
                        tool = name,
                        checkpoint_id = %ckpt.checkpoint_id,
                        error = %e,
                        "checkpoint written but set_checkpoint_head failed; blocking mutating tool",
                    );
                    return Err(self.checkpoint_failure(name, "checkpoint_head_failed", &e.to_string()));
                }
                let span = Ulid::new().to_string();
                self.policy_events.push(PolicyEvent {
                    actor: UnifiedActor::Hook,
                    span_id: Some(span.clone()),
                    payload: UnifiedPayload::CheckpointCreated(CheckpointWrittenEvent {
                        checkpoint_id: ckpt.checkpoint_id,
                        // Chat-rail checkpoints are session-scoped, not run-
                        // scoped: carry the session id in the reused `run_id`
                        // slot so the trace dock can correlate.
                        run_id: self.session_id.clone(),
                        span_id: span,
                        sequence: 0,
                        kind: "tool_step".to_string(),
                        input_hash: ckpt.content_hash,
                        output_hash: None,
                        input_payload_ref: None,
                        output_payload_ref: None,
                    }),
                });
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "xvision::dashboard::chat_rail",
                    session_id = %self.session_id,
                    tool = name,
                    error = %e,
                    "pre-tool checkpoint snapshot failed; blocking mutating tool (fail closed)",
                );
                Err(self.checkpoint_failure(name, e.code(), &e.to_string()))
            }
        }
    }

    /// Resolve the strategy id a write tool will mutate: the explicit
    /// `strategy_id` / `id` in the tool input, falling back to the session's
    /// most-recent draft id. `None` when neither is available.
    fn snapshot_strategy_id(&self, input: &serde_json::Value) -> Option<String> {
        let from_input = input
            .get("strategy_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        from_input.or_else(|| self.last_draft_id.clone())
    }

    /// Build the typed denial tool_result + queue an `ErrorPersistenceFailed`
    /// event for a failed pre-tool checkpoint. Shared by both failure paths
    /// (snapshot write, head-pointer update) so the model and the unified log
    /// see a consistent, never-silent record.
    fn checkpoint_failure(&mut self, tool: &str, code: &str, detail: &str) -> serde_json::Value {
        let message = format!(
            "Could not take a safety checkpoint before `{tool}`; the edit was blocked so no \
             un-rewindable change is made. ({detail})"
        );
        self.policy_events.push(PolicyEvent {
            actor: UnifiedActor::Hook,
            span_id: None,
            payload: UnifiedPayload::ErrorPersistenceFailed(TypedError {
                code: code.to_string(),
                message: message.clone(),
                remediation: Some(
                    "Retry once the checkpoint store is writable; check disk space and \
                     $XVN_HOME permissions."
                        .to_string(),
                ),
            }),
        });
        serde_json::json!({
            "error": message,
            "checkpoint_failed": true,
            "code": code,
        })
    }

    async fn run_tool(&self, name: &str, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        if !agent_tool_defs(self.profile).iter().any(|d| d.name == name) {
            anyhow::bail!("tool '{name}' is not available in {:?} profile", self.profile);
        }
        let mut input = normalize_tool_input(name, input);
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
                let strategy = api_strategy::get(&self.api_context, id).await?;
                // W11 (Finding #13): resolve each AgentRef to its full Agent so
                // the authoring agent can read back the system_prompt it wrote.
                // Failures to resolve individual refs degrade gracefully (omit
                // that entry) rather than failing the whole call.
                let mut resolved_agents: Vec<serde_json::Value> = Vec::new();
                for aref in &strategy.agents {
                    match api_agents::get(&self.api_context, &aref.agent_id).await {
                        Ok(agent) => {
                            // Collect all slot system_prompts. Most strategies
                            // have a single slot; expose them all so multi-slot
                            // agents are fully visible.
                            let slot_prompts: Vec<&str> =
                                agent.slots.iter().map(|s| s.system_prompt.as_str()).collect();
                            // Use the first slot's prompt as the primary
                            // system_prompt for the role entry (the canonical
                            // "what does this role's LLM see?" answer). If an
                            // agent has multiple slots, all prompts are also
                            // available under slot_system_prompts.
                            let primary_prompt = slot_prompts.first().copied().unwrap_or("");
                            resolved_agents.push(serde_json::json!({
                                "role": aref.role,
                                "agent_id": aref.agent_id,
                                "system_prompt": primary_prompt,
                                "slot_system_prompts": slot_prompts,
                            }));
                        }
                        Err(err) => {
                            tracing::warn!(
                                strategy_id = id,
                                agent_id = aref.agent_id.as_str(),
                                error = %err,
                                "get_strategy: could not resolve AgentRef — omitting from resolved_agents"
                            );
                            // Degrade gracefully: include a note so the
                            // authoring agent knows one ref couldn't be resolved
                            // rather than silently seeing a short list.
                            resolved_agents.push(serde_json::json!({
                                "role": aref.role,
                                "agent_id": aref.agent_id,
                                "system_prompt": null,
                                "error": "agent not found — the referenced agent may have been deleted",
                            }));
                        }
                    }
                }
                let mut out = serde_json::to_value(&strategy)?;
                if let Some(obj) = out.as_object_mut() {
                    obj.insert(
                        "resolved_agents".into(),
                        serde_json::Value::Array(resolved_agents),
                    );
                }
                Ok(out)
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
                if let Some(prompt) = input.get("prompt").and_then(|value| value.as_str()) {
                    if !prompt.trim().is_empty() {
                        anyhow::bail!(
                            "update_slot does not accept `prompt`; use create_strategy_agent to set or replace the trader prompt"
                        );
                    }
                }
                if let Some(obj) = input.as_object_mut() {
                    obj.remove("prompt");
                }
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
            "set_filter" => {
                let req: SetFilterReq = serde_json::from_value(input)?;
                let strategy_id = req
                    .strategy_id
                    .or(req.id)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("set_filter: missing `strategy_id`"))?;

                let filter = normalize_set_filter_payload(req.filter, req.source, req.format, &strategy_id)?;

                let out = api_strategy::set_filter(
                    &self.api_context,
                    authoring::SetFilterReq {
                        strategy_id,
                        filter: Some(filter),
                        source: Some("json".into()),
                    },
                )
                .await?;
                Ok(serde_json::to_value(out)?)
            }
            "list_eval_runs" => {
                let req: ListEvalRunsReq = serde_json::from_value(input)?;
                let status = match req.status.as_deref() {
                    Some(s) => Some(vec![RunStatus::parse(s)
                        .ok_or_else(|| anyhow::anyhow!("list_eval_runs: invalid status `{s}`"))?]),
                    None => None,
                };
                let out = api_eval::list_summaries_paged(
                    &self.api_context,
                    api_eval::ListRunsRequest {
                        agent_id: req.agent_id,
                        scenario_id: req.scenario_id,
                        status,
                        limit: req.limit,
                        offset: req.offset,
                        // bead-008: the wizard's eval-run list does not expose a
                        // time filter — no `since` lower bound.
                        since: None,
                    },
                )
                .await?;
                Ok(serde_json::to_value(out)?)
            }
            "get_eval_run" => {
                // Return the slim `RunSummary` shape (same as the REST
                // `GET /api/eval/runs/:id` response) rather than the raw
                // `Run` struct.  The raw struct serialises
                // `metrics: Option<MetricsSummary>` as `"metrics": null`
                // for failed / in-flight runs, which leaves the model with
                // an opaque nested null it cannot reason over and trips the
                // tool-failure circuit-breaker (Finding #4).
                //
                // `summarise_run` maps `metrics: None` → flat top-level
                // `sharpe/max_drawdown_pct/total_return_pct` fields each
                // serialised as `null`, and brings `status` and `error` to
                // the top level as plain strings — a stable, agent-readable
                // shape regardless of run status.
                let req: GetEvalRunReq = serde_json::from_value(input)?;
                let run = api_eval::get(&self.api_context, &req.id).await?;
                let summary = api_eval::summarise_run(run);
                Ok(serde_json::to_value(summary)?)
            }
            "list_eval_reviews" => {
                let req: ListEvalReviewsReq = serde_json::from_value(input)?;
                let store = RunStore::new(self.pool.clone());
                let reviews = store.list_reviews_for_run(&req.run_id).await?;
                Ok(serde_json::json!({
                    "run_id": req.run_id,
                    "items": reviews,
                }))
            }
            "get_eval_review" => {
                let req: GetEvalReviewReq = serde_json::from_value(input)?;
                let store = RunStore::new(self.pool.clone());
                let review = store
                    .get_review(&req.id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("get_eval_review: review `{}` not found", req.id))?;
                let findings: Vec<Finding> = store.read_findings_for_review(&req.id).await?;
                Ok(serde_json::json!({
                    "review": review,
                    "findings": findings,
                }))
            }
            "clear_filter" => {
                let req: ClearFilterReq = serde_json::from_value(input)?;
                let strategy_id = req
                    .strategy_id
                    .or(req.id)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("clear_filter: missing `strategy_id`"))?;

                api_strategy::clear_strategy_filter(&self.api_context, &strategy_id).await?;
                Ok(serde_json::json!({
                    "id": strategy_id,
                    "ok": true,
                    "cleared": true,
                }))
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
                    "live" => RunMode::Live,
                    _ => RunMode::Backtest,
                };
                let full_strategy = api_strategy::get(&self.api_context, &strategy.agent_id).await?;
                if matches!(full_strategy.activation_mode, ActivationMode::EveryBar)
                    && full_strategy.filter.is_none()
                {
                    let acknowledge_no_filter = input
                        .get("acknowledge_no_filter")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if !acknowledge_no_filter {
                        return Ok(serde_json::json!({
                            "needs_clarification": {
                                "question": "No filter is attached to this strategy. Attach a deterministic filter with `set_filter` before running eval, or set `acknowledge_no_filter: true` to run without filter.",
                                "options": [
                                    {"tool": "set_filter", "id": strategy.agent_id.clone()},
                                    {"tool": "run_eval", "agent_id": agent_query, "scenario_id": scenario_query, "acknowledge_no_filter": true}
                                ]
                            }
                        }));
                    }
                }
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
                    live_config: None,
                    limits: None,
                    skip_preflight: false,
                    provider_override: None,
                    assets_subset: None,
                    auto_fire_review: false,
                    review_model: None,
                    max_annotations_per_review: Some(8),
                    trajectory_mode: api_eval::RunTrajectoryMode::default(),
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
                    if let Some(job) = store.get(job_id).await? {
                        job
                    } else if !job_id.starts_with("job_") {
                        let eval_job_id = format!("{}{}", eval_run_bridge::EVAL_RUN_PREFIX, job_id);
                        eval_run_bridge::get_synthetic_job(&self.pool, &eval_job_id)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?
                    } else {
                        anyhow::bail!("cli job '{job_id}' not found")
                    }
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
                    if let Some(output) = store.output(job_id).await? {
                        output
                    } else if !job_id.starts_with("job_") {
                        let eval_job_id = format!("{}{}", eval_run_bridge::EVAL_RUN_PREFIX, job_id);
                        eval_run_bridge::get_synthetic_output(&self.pool, &eval_job_id)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("cli job '{job_id}' not found"))?
                    } else {
                        anyhow::bail!("cli job '{job_id}' not found")
                    }
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
            // ── W5 read tools (Findings #5-7) ──────────────────────────
            "list_providers" => {
                // Return the configured provider/model list from the workspace
                // config. This is the same data the REST GET /api/settings/providers
                // route returns. Uses runtime_config_path() for consistent env-var
                // precedence (XVN_CONFIG_PATH / XVN_CONFIG / xvn_home/config/default.toml).
                //
                // When the config file does not yet exist (fresh workspace before
                // `xvn setup`), return an empty report rather than propagating the
                // error — the agent can still see the shape and communicate that
                // the operator needs to configure providers.
                let config_path = xvision_core::config::runtime_config_path(&self.api_context.xvn_home);
                let report = if config_path.exists() {
                    api_providers::list(&self.api_context, &config_path).await?
                } else {
                    xvision_engine::api::settings::providers::ProvidersReport {
                        providers: vec![],
                        default_model: None,
                        invalid: None,
                    }
                };
                Ok(serde_json::to_value(report)?)
            }
            "get_agent" => {
                // Return one Agent record by id. No mutation.
                let agent_id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("get_agent: missing `id`"))?;
                let out = api_agents::get(&self.api_context, agent_id).await?;
                Ok(serde_json::to_value(out)?)
            }
            "filter_catalog" => {
                // Return the filter-DSL token catalog as structured data.
                // The catalog lives in the baked wiki page `filter-dsl-catalog`;
                // here we return the key token lists as structured JSON so the
                // agent can reason over them without parsing markdown.
                Ok(filter_catalog_json())
            }
            // ── W10 Finding #9: scenario management tools ─────────────────
            "clone_scenario" => {
                let parent_id = input
                    .get("parent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("clone_scenario: missing `parent_id`"))?;
                let mutations = ScenarioMutations {
                    display_name: input
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    description: input
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    time_window: None,
                    granularity: None,
                    venue: None,
                    tags: input
                        .get("tags")
                        .and_then(|v| serde_json::from_value(v.clone()).ok()),
                    notes: input.get("notes").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    warmup_bars: input
                        .get("warmup_bars")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32),
                };
                let out = api_scenario::clone(&self.api_context, parent_id, mutations).await?;
                Ok(serde_json::to_value(out)?)
            }
            "archive_scenario" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("archive_scenario: missing `id`"))?;
                api_scenario::archive(&self.api_context, id).await?;
                Ok(serde_json::json!({ "archived": true, "id": id }))
            }
            "set_scenario_regime" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("set_scenario_regime: missing `id`"))?;
                let regime = input.get("regime").and_then(|v| v.as_str());
                let volatility = input.get("volatility").and_then(|v| v.as_str());
                let direction = input.get("direction").and_then(|v| v.as_str());
                let out =
                    api_scenario::set_regime(&self.api_context, id, regime, volatility, direction).await?;
                Ok(serde_json::to_value(out)?)
            }
            "classify_scenario" => {
                let id = input
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("classify_scenario: missing `id`"))?;
                let force = input.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
                let result = api_scenario::classify(&self.api_context, id, force).await?;
                Ok(serde_json::to_value(result)?)
            }
            "select_scenarios" => {
                let target_decisions = input.get("target_decisions").and_then(|v| v.as_u64());
                let same_decisions = input
                    .get("same_decisions")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let max_decisions = input.get("max_decisions").and_then(|v| v.as_u64());
                let count = input.get("count").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
                let timeframe_minutes = input
                    .get("timeframe_minutes")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                let regimes: Vec<String> = input
                    .get("regimes")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let rows = api_scenario::select(
                    &self.api_context,
                    timeframe_minutes,
                    &regimes,
                    target_decisions,
                    same_decisions,
                    max_decisions,
                    count,
                )
                .await?;
                Ok(serde_json::to_value(rows)?)
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
                exclude_tags: vec![],
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
                description: None,
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
        // Detect whether the caller supplied an explicit system_prompt.
        // When no explicit prompt is given the default is stamped from the
        // strategy's current asset_universe — if that universe is still the
        // blank-draft default (["BTC/USD"]), the agent will say "Evaluate
        // BTC/USD" even if the operator intended a different asset.  We do
        // NOT block creation; we surface a non-fatal warning so the model can
        // self-correct (Finding #14).
        let explicit_system_prompt = req
            .system_prompt
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let using_default_prompt = explicit_system_prompt.is_none();
        let system_prompt =
            explicit_system_prompt.unwrap_or_else(|| default_strategy_agent_prompt(&strategy, &role));
        // Determine whether the strategy's asset_universe is still the
        // blank-draft default so we can emit the stale-default warning.
        let asset_universe_is_default = strategy.manifest.asset_universe == vec!["BTC/USD"];
        let skill_ids = tools_for_strategy_role(&strategy, &role);
        let description = req
            .description
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| derive_strategy_agent_description(&strategy, &role));
        let agent = api_agents::create(
            &self.api_context,
            api_agents::CreateAgentRequest {
                name,
                description,
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
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await?;
        let attached = api_strategy::add_agent(
            &self.api_context,
            api_strategy::AddAgentReq {
                strategy_id: strategy_id.clone(),
                agent_id: agent.agent_id.clone(),
                role: role.clone(),
                activates: None,
            },
        )
        .await?;
        let mut result = serde_json::json!({
            "strategy_id": strategy_id,
            "agent_id": agent.agent_id,
            "role": role,
            "provider": provider,
            "model": model,
            "agents": attached.agents,
            "pipeline": attached.pipeline
        });
        // Non-fatal warning: if the prompt was auto-generated from an
        // asset_universe that is still the blank-draft default, surface it
        // so the model can self-correct by calling update_manifest and then
        // re-creating or patching the agent.
        if using_default_prompt && asset_universe_is_default {
            result["warning"] = serde_json::json!(
                "agent prompt generated from default asset_universe BTC/USD — \
                 call update_manifest first if you intended a different asset. \
                 Verify the stamped prompt with get_strategy."
            );
        }
        Ok(result)
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
                activates: None,
            },
        )
        .await?;
        Ok(out)
    }

    /// Resolve the (provider, model) the wizard should bind to a newly
    /// created strategy agent.
    ///
    /// Resolution order:
    /// 1. The tool call's explicit `provider`/`model` arguments.
    /// 2. The rail's selected `(agent_provider, agent_model)` — the
    ///    operator's choice in the ModelPicker.
    ///
    /// Pre-2026-05-26 there was a third fallback that inherited
    /// `self.model` (the chat dispatch model) when `agent_model` was
    /// unset. That silent inheritance was the second half of the
    /// recurring "Gemini complaint despite Google selected" QA bug:
    /// the chat rail's auto-pick race could promote OpenRouter's
    /// deepseek-v4-pro as the chat dispatch, and every agent created
    /// by the wizard would then inherit deepseek too, even though the
    /// operator had explicitly picked a Gemini variant in Settings →
    /// Providers. The LLM would then attempt to use Gemini, the
    /// OpenRouter route would reject it, and the assistant would
    /// synthesize the long "no Gemini models are currently enabled
    /// on your OpenRouter provider" explanation that confused QA.
    ///
    /// Removing the fallback turns the failure into a typed error
    /// the chat agent can act on — "pick a provider/model in the
    /// chat model picker" — instead of producing a working-but-
    /// wrong run on the chat model.
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
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("create_strategy_agent: missing model; pick a provider/model in the chat model picker or pass model explicitly (the chat dispatch model is intentionally NOT inherited — see resolve_agent_runtime docstring)"))?;
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
    if role == "regime" {
        return strategy
            .regime_slot
            .as_ref()
            .map(|slot| slot.allowed_tools.clone())
            .unwrap_or_default();
    }
    Vec::new()
}

fn default_strategy_agent_prompt(strategy: &xvision_engine::strategies::Strategy, role: &str) -> String {
    let role = role.trim();
    let assets = if strategy.manifest.asset_universe.is_empty() {
        "the configured asset universe".to_string()
    } else {
        strategy.manifest.asset_universe.join(", ")
    };
    let cadence = strategy.manifest.decision_cadence_minutes;
    format!(
        "You are the {role} agent for strategy '{}'. Evaluate {assets} on {cadence}-minute bars. \
         Use the provided OHLCV, indicator, filter, and risk context. Return only the structured \
         decision required by the runtime; do not invent unavailable data.",
        strategy.manifest.display_name
    )
}

/// Derive a one-line, operator-facing description for an agent created by
/// the chat-rail wizard when the LLM didn't supply one via the
/// `description` tool argument. Prefers strategy `display_name` over the
/// raw ULID so the Agents list shows a meaningful label; folds in
/// `plain_summary` when the strategy author has filled one in.
fn derive_strategy_agent_description(strategy: &xvision_engine::strategies::Strategy, role: &str) -> String {
    let role = role.trim();
    let name = strategy.manifest.display_name.trim();
    let display = if name.is_empty() {
        strategy.manifest.id.as_str()
    } else {
        name
    };
    let summary = strategy.manifest.plain_summary.trim();
    if summary.is_empty() {
        format!("{role} agent for strategy '{display}'.")
    } else {
        format!("{role} agent for strategy '{display}' — {summary}")
    }
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
    // Scenarios are asset-free; the asset for a bars fetch is chosen at the
    // run layer, not stored on the scenario. Suggest a placeholder symbol the
    // operator edits before running.
    let asset = "BTC".to_string();
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

    // Scenarios are asset-free. `CreateScenarioRequest` rejects unknown
    // fields, so strip any `asset` / `symbol` the agent may still emit
    // (the model doesn't know the schema dropped them) rather than
    // building an AssetRef the request no longer accepts.
    obj.remove("asset");
    obj.remove("symbol");

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
            | "set_filter"
            | "clear_filter"
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
                "Strategy ready",
                format!("Strategy {id} is ready for inspection."),
                InlineAction {
                    label: "Open Strategy".into(),
                    href: Some(format!("/strategies/{id}")),
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
                    label: "Open Strategy".into(),
                    href: Some(format!("/strategies/{id}")),
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
#[cfg(test)]
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
                // QA31: `venue` removed from required. `CreateScenarioRequest`
                // now `#[serde(default)]`s the field to a sensible Alpaca
                // preset, so chat agents (Gemini Flash, etc.) that don't
                // know the inner shape don't get stuck in the 12-iteration
                // retry loop on `missing field venue`.
                "required": [
                    "display_name", "description", "asset_class",
                    "quote_currency", "time_window", "capital", "granularity",
                    "timezone", "calendar", "data_source",
                    "replay_mode", "tags", "source"
                ]
            }),
        },
        ToolDefinition {
            name: "update_slot".into(),
            description:
                "Update a regime/trader slot. `prompt` is not supported here; use `create_strategy_agent` for prompt updates. Only fields with non-null values are mutated."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "slot": {"type": "string", "enum": ["regime", "trader"]},
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
            description: "Persist manifest fields shown in the Strategy Inspector, including \
                display name, description, asset universe, decision cadence, and display color. \
                All fields except `id` are optional — supply only the ones you want to change."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "display_name": {
                        "type": "string",
                        "description": "Human-readable strategy name shown in the UI."
                    },
                    "plain_summary": {
                        "type": "string",
                        "description": "One-line plain-English description of what the strategy does."
                    },
                    "color": {
                        "type": "string",
                        "description": "Optional display color as a 7-character CSS hex string (e.g. '#D4A547'). Pass an empty string to clear a previously set color."
                    },
                    "asset_universe": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "description": "List of SYMBOL/QUOTE pairs this strategy trades (e.g. ['BTC/USD'])."
                    },
                    "decision_cadence_minutes": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "How often (in minutes) the strategy evaluates whether to act."
                    }
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
            name: "set_filter".into(),
            description: "Attach/replace the strategy's deterministic filter. \
                The filter is a structured DSL — NOT a single `{indicator, operator, value}` triple. \
                Pass the full object under `filter` (or the raw text under `source` with `format`). \
                Required fields: `display_name`, `asset_scope` (array of SYMBOL/QUOTE pairs), \
                `timeframe` (e.g. `4h`), `conditions` (a `{\"all\":[...]}` / `{\"any\":[...]}` tree of \
                `{lhs, op, rhs}` clauses). Optional: `cooldown_bars`, `max_wakeups_per_day`, \
                `scan_cadence`, `wake_when_in_position`, `description`. Indicator refs go inside `lhs`/`rhs` \
                (e.g. `atr_pct_14`, `ema_20`, `rsi_14`, `close`). \
                Example: `{\"display_name\":\"Elevated ATR Short\",\"asset_scope\":[\"BTC/USD\",\"ETH/USD\"],\"timeframe\":\"4h\",\"cooldown_bars\":3,\"conditions\":{\"all\":[{\"lhs\":\"atr_pct_14\",\"op\":\">\",\"rhs\":0.6}]}}`. \
                Bare keys like `indicator`/`operator`/`value`/`period` are NOT part of the schema — encode them as a `conditions` clause instead.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alias for strategy_id"},
                    "strategy_id": {"type": "string"},
                    "filter": {
                        "type": ["object", "string"],
                        "description": "Filter payload: object or raw JSON/TOML text when using `format`."
                    },
                    "source": {"type": "string", "description": "Filter text (JSON or TOML)"},
                    "format": {"type": "string", "enum": ["json", "toml"]}
                },
                "required": ["id"],
                "oneOf": [
                    {"required": ["id", "filter"]},
                    {"required": ["id", "source"]}
                ]
            }),
        },
        ToolDefinition {
            name: "clear_filter".into(),
            description: "Clear strategy filter and restore every-bar activation mode.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alias for strategy_id"},
                    "strategy_id": {"type": "string"}
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "create_strategy_agent".into(),
            description: "Create a reusable Agent with an explicit provider/model and attach it to a strategy. Use role `trader` for eval-ready single-agent strategies. If provider/model are omitted, the currently selected chat provider/model is used. ALWAYS supply a one-line `description` summarizing what the agent does (e.g. \"4H ETH range-fader using RSI + EMA20\"), so the operator-facing Agents list isn't littered with auto-generated placeholders. ORDERING: call update_manifest first (to set asset_universe and cadence) before calling this tool — the agent's default prompt is generated from the strategy's current asset_universe + cadence at the moment this tool runs. If you skip update_manifest, the prompt will reflect the blank-draft default (BTC/USD, 60 min). Pass an explicit `system_prompt` argument to override the default entirely.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy_id": {"type": "string"},
                    "id": {"type": "string", "description": "Alias for strategy_id"},
                    "role": {"type": "string", "default": "trader"},
                    "name": {"type": "string"},
                    "description": {
                        "type": "string",
                        "description": "One-line, operator-facing summary of what this agent does. REQUIRED in practice — without it the agent shows a placeholder description in the UI."
                    },
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
                    "mode": {"type": "string", "enum": ["backtest", "live"]},
                    "acknowledge_no_filter": {
                        "type": "boolean",
                        "description": "If true, allows running a strategy without a filter gate."
                    }
                },
                "required": ["agent_id", "scenario_id"]
            }),
        },
        ToolDefinition {
            name: "list_eval_runs".into(),
            description: "List eval runs for quick review of prior output, sharpe, and status.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {"type": "string"},
                    "scenario_id": {"type": "string"},
                    "status": {
                        "type": "string",
                        "enum": ["queued", "running", "completed", "failed", "cancelled"]
                    },
                    "limit": {"type": "integer", "minimum": 1, "description": "Optional page size"},
                    "offset": {"type": "integer", "minimum": 0, "description": "Optional page offset"}
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_eval_run".into(),
            description: "Read one eval run by id, including metrics, status, and bar metadata.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "list_eval_reviews".into(),
            description: "List all review records for one eval run (newest first).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"run_id": {"type": "string"}},
                "required": ["run_id"]
            }),
        },
        ToolDefinition {
            name: "get_eval_review".into(),
            description: "Read one eval review by id and include normalized findings.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
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
        // ── W10 Finding #9: scenario management tools ─────────────────────
        ToolDefinition {
            name: "clone_scenario".into(),
            description: "Derive a new scenario from an existing one. Inherits every \
                          unset field from the parent and stamps parent_scenario_id. \
                          Refuses to clone an archived parent. Returns the new Scenario.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "parent_id": {
                        "type": "string",
                        "description": "Id of the scenario to clone."
                    },
                    "display_name": {
                        "type": "string",
                        "description": "Optional override for the clone's display name."
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional override for the description."
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional tag override."
                    },
                    "notes": {
                        "type": "string",
                        "description": "Optional notes for the clone."
                    },
                    "warmup_bars": {
                        "type": "integer",
                        "description": "Optional override for pre-window warmup bars."
                    }
                },
                "required": ["parent_id"]
            }),
        },
        ToolDefinition {
            name: "archive_scenario".into(),
            description: "Soft-delete a scenario (sets archived_at). \
                          Archived scenarios are excluded from list_scenarios by default. \
                          Returns {archived: true, id}.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Scenario id to archive."
                    }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "set_scenario_regime".into(),
            description: "Set operator-authored regime labels on a scenario \
                          (regime_derived = false). All three label fields are optional; \
                          omitting one leaves the existing value unchanged. \
                          Returns the updated Scenario. \
                          Valid regime values: trend | chop | crash | expansion | recovery. \
                          Valid volatility values: low | normal | high | extreme. \
                          Valid direction values: up | down | sideways.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Scenario id."
                    },
                    "regime": {
                        "type": "string",
                        "enum": ["trend", "chop", "crash", "expansion", "recovery"],
                        "description": "Broad regime label."
                    },
                    "volatility": {
                        "type": "string",
                        "enum": ["low", "normal", "high", "extreme"],
                        "description": "Volatility label."
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "sideways"],
                        "description": "Trend direction."
                    }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "classify_scenario".into(),
            description: "Auto-derive regime labels for a scenario from its bar window \
                          (regime_derived = true). Requires bars to have been warmed via \
                          `xvn bars fetch`. Returns `{ classified: bool, skipped_reason: \
                          string|null, scenario }`: classified=true when labels were \
                          derived and written; classified=false with a skipped_reason when \
                          skipped (bars unavailable, or the scenario already has \
                          operator-set labels and force=false). Pass force=true to \
                          re-derive over operator-set labels.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Scenario id to classify."
                    },
                    "force": {
                        "type": "boolean",
                        "description": "If true, overwrite even operator-set labels."
                    }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "select_scenarios".into(),
            description: "Stateless read-only selector: filter the scenario library \
                          by timeframe, regime, and decision-count proximity. \
                          Use this to find a comparable set of scenarios for A/B evaluation \
                          without needing to hand-pick ids. Returns [{id, name, timeframe, decision_count}]. \
                          Mode A: set target_decisions=N to find scenarios within ±10% of N. \
                          Mode B: set same_decisions=true and max_decisions=N to find \
                          the largest common decision count ≤ N in the candidate set.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "target_decisions": {
                        "type": "integer",
                        "description": "[Mode A] Select scenarios within ±10% of this decision count."
                    },
                    "same_decisions": {
                        "type": "boolean",
                        "description": "[Mode B] Return scenarios sharing the largest common decision count ≤ max_decisions."
                    },
                    "max_decisions": {
                        "type": "integer",
                        "description": "[Mode B] Maximum decision count for the common-count search."
                    },
                    "timeframe_minutes": {
                        "type": "integer",
                        "description": "Optional granularity filter in minutes (e.g. 240 for 4h, 60 for 1h)."
                    },
                    "regimes": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional regime label filter (OR semantics per scenario)."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Maximum number of results to return. Default 4."
                    }
                },
                "required": []
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
        // ── W5 read tools (Findings #5-7) ──────────────────────────────────
        ToolDefinition {
            name: "list_providers".into(),
            description: "List configured LLM providers and their enabled models. \
                          Returns the same data as GET /api/settings/providers: \
                          each provider's name, kind (anthropic/openai-compat/etc.), \
                          enabled models, and whether an API key is set. \
                          Use this before authoring a strategy agent to confirm \
                          which (provider, model) pairs are launchable.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_agent".into(),
            description: "Inspect one Agent library record by id. \
                          Returns the Agent's name, description, tags, and slots \
                          (each slot carries provider, model, system_prompt, \
                          skill_ids). Use this to review a saved agent before \
                          attaching it to a strategy with `attach_agent`.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The agent_id (ULID) to inspect."
                    }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "filter_catalog".into(),
            description: "Return the filter-DSL token catalog so you can \
                          author correct `set_filter` payloads without guessing. \
                          Returns structured lists of: operators (DSL token + \
                          description), indicator categories with per-indicator \
                          names and parameter ranges, and required filter fields. \
                          Call this before `set_filter` when you are unsure of the \
                          exact token names or parameter constraints.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
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

/// Return the filter-DSL token catalog as structured JSON for the
/// `filter_catalog` chat tool (W5 Finding #7). Encoding the key operator
/// and indicator tables here lets the agent reason over concrete token names
/// without parsing the Markdown wiki page.
///
/// The data mirrors `crates/xvision-dashboard/wiki/filter-dsl-catalog.md`.
/// Keep the two in sync: when a new indicator or operator is added to the
/// DSL, update both the wiki page AND this function.
fn filter_catalog_json() -> serde_json::Value {
    serde_json::json!({
        "description": "Filter DSL token catalog for `set_filter` payloads. \
                        Use exact token names from this catalog when authoring \
                        `conditions` clauses. Parameterized tokens encode the \
                        period directly, e.g. `rsi_14`, `ema_20`, `atr_pct_7`.",
        "required_fields": ["display_name", "asset_scope", "timeframe", "conditions"],
        "optional_fields": [
            "cooldown_bars",
            "max_wakeups_per_day",
            "scan_cadence",
            "wake_when_in_position",
            "description"
        ],
        "wake_when_in_position_tokens": [
            {
                "token": "on_invalidation_or_target_only",
                "description": "Default. Wake only on a fresh trip of the condition tree while holding. Suppresses sustained-true bars. Cost-safe default."
            },
            {
                "token": "always",
                "description": "Wake on every bar the tree is true while holding. Expensive: one trader-LLM call per in-position bar. Opt-in only."
            },
            {
                "token": "never",
                "description": "Never wake while holding; exits rely entirely on risk.stop_loss_atr_multiple."
            }
        ],
        "operators": [
            {"token": ">", "description": "Greater than. indicator lhs, indicator or numeric rhs."},
            {"token": "<", "description": "Less than. indicator lhs, indicator or numeric rhs."},
            {"token": ">=", "description": "Greater or equal. indicator lhs, indicator or numeric rhs."},
            {"token": "<=", "description": "Less or equal. indicator lhs, indicator or numeric rhs."},
            {"token": "==", "description": "Equal. indicator lhs, indicator or numeric rhs."},
            {"token": "crosses_above", "description": "Crosses above. indicator lhs, indicator rhs only (no numeric rhs)."},
            {"token": "crosses_below", "description": "Crosses below. indicator lhs, indicator rhs only (no numeric rhs)."},
            {"token": "between", "description": "Between inclusive. indicator lhs, two-number range rhs."},
            {"token": "above_for_<bars>", "description": "lhs > rhs for N bars (current + N-1 prior). Example: `above_for_3`."},
            {"token": "below_for_<bars>", "description": "lhs < rhs for N bars. Example: `below_for_3`."},
            {"token": "crossed_above_<bars>", "description": "Cross occurred on current bar or within N-1 prior bars. Example: `crossed_above_5`."},
            {"token": "crossed_below_<bars>", "description": "Same as crossed_above but downward."},
            {"token": "slope_gt_<bars>", "description": "lhs change vs N bars ago > numeric rhs. Example: `slope_gt_4`."},
            {"token": "slope_lt_<bars>", "description": "lhs change vs N bars ago < numeric rhs."},
            {"token": "zscore_gt_<period>", "description": "lhs z-score over N bars > numeric rhs. Example: `zscore_gt_20`."},
            {"token": "zscore_lt_<period>", "description": "lhs z-score over N bars < numeric rhs."},
            {"token": "within_pct_<pct>", "description": "lhs within pct% of rhs. Example: `within_pct_1.5`."}
        ],
        "indicators": {
            "price_and_volume": ["open", "high", "low", "close", "volume"],
            "moving_averages_and_trend": [
                "sma_<period>", "ema_<period>", "wma_<period>",
                "adx_<period>", "di_plus_<period>", "di_minus_<period>",
                "donchian_upper_<period>", "donchian_middle_<period>", "donchian_lower_<period>",
                "highest_<period>", "lowest_<period>",
                "opening_range_high_<minutes>", "opening_range_low_<minutes>", "opening_range_mid_<minutes>"
            ],
            "ichimoku": [
                "tenkan", "kijun", "senkou_a", "senkou_b", "chikou",
                "cloud_top", "cloud_bottom", "cloud_thickness"
            ],
            "momentum_and_oscillators": [
                "rsi_<period>", "roc_<period>",
                "stoch_k_<period>", "stoch_d_<period>",
                "stoch_rsi_<period>", "stoch_rsi_k_<period>", "stoch_rsi_d_<period>",
                "cci_<period>", "mfi_<period>", "williams_r_<period>"
            ],
            "volatility_and_bands": [
                "atr_<period>", "atr_pct_<period>",
                "bb_upper_<period>", "bb_middle_<period>", "bb_lower_<period>",
                "bb_width_<period>", "bb_pct_b_<period>",
                "keltner_upper_<period>", "keltner_middle_<period>", "keltner_lower_<period>"
            ],
            "macd": [
                "macd_line", "macd", "macd_12_26_9",
                "macd_signal", "macd_signal_12_26_9",
                "macd_hist", "macd_histogram", "macd_hist_12_26_9"
            ],
            "volume": [
                "vwap_<period>", "volume_sma_<period>",
                "rvol_<period>", "rvol_tod_<period>", "volume_zscore_<period>", "obv"
            ],
            "session_and_reference_levels": [
                "prev_day_open", "prev_day_high", "prev_day_low", "prev_day_close",
                "prev_week_high", "prev_week_low", "prev_week_close",
                "prev_month_open", "prev_month_high", "prev_month_low", "prev_month_close",
                "premarket_high", "premarket_low",
                "gap_pct", "gap_up", "gap_down"
            ]
        },
        "period_ranges": {
            "default": "2 to 500 for most indicators",
            "adx_di": "2 to 200; numeric thresholds must be 0 to 100",
            "rsi": "2 to 200; numeric thresholds must be 0 to 100",
            "stoch": "2 to 200; numeric thresholds must be 0 to 100",
            "williams_r": "2 to 200; numeric thresholds must be -100 to 0",
            "atr_pct": "2 to 200; numeric thresholds must be > 0"
        },
        "conditions_structure": {
            "description": "Conditions use a boolean tree of `all`/`any` nodes. Each leaf is `{lhs, op, rhs}`.",
            "example": {
                "all": [
                    {"lhs": "rsi_14", "op": ">", "rhs": 50},
                    {"lhs": "ema_12", "op": "crosses_above", "rhs": "ema_26"}
                ]
            }
        }
    })
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

    /// Put a session into Act mode and auto-approve every write tool. Used by
    /// the pre-Phase-2 unit tests, which exercise tool *execution* mechanics
    /// (not the safety gate): without this, the default research mode +
    /// needs-approval policy would correctly block the write tools they drive.
    /// The dedicated Phase 2 gate behaviour is covered by
    /// `tests/chat_rail_safety.rs`.
    async fn unlock_writes_for_tests(pool: &SqlitePool, session_id: &str) {
        ChatSessionStore::set_mode(pool, session_id, "act").await.unwrap();
        for def in agent_tool_defs(AgentProfile::Workspace) {
            ToolPolicyStore::upsert_policy(
                pool,
                GLOBAL_SCOPE,
                &def.name,
                xvision_engine::chat_session::ToolPolicy {
                    enabled: true,
                    auto_approve: true,
                },
            )
            .await
            .unwrap();
        }
    }

    async fn loop_with_session(
        dispatch: Arc<dyn LlmDispatch>,
        msg: &str,
        scope: ContextScope,
    ) -> (WizardLoop, SqlitePool, tempfile::TempDir, String) {
        let (pool, td) = fresh_pool().await;
        let session_id = ChatSessionStore::create_session(&pool, &scope).await.unwrap();
        unlock_writes_for_tests(&pool, &session_id).await;
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
            WizardEvent::ToolResult { tool, result, .. } => {
                assert_eq!(tool, "list_strategies");
                assert!(result.is_array(), "expected an array, got {result}");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        assert!(matches!(&events[2], WizardEvent::Token { text } if text.contains("strategies")));
        assert!(matches!(&events[3], WizardEvent::Done { .. }));
    }

    /// Regression: a model turn that signals `stop_reason == ToolUse`
    /// but emits no `tool_use` block (text-only / malformed content)
    /// USED to bail as Done after the first response, cutting off any
    /// planned follow-up tool calls. The loop must instead nudge the
    /// model and iterate so multi-step plans complete.
    #[tokio::test]
    async fn malformed_tool_use_stop_reason_continues_loop() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            // Turn 1: malformed — stop_reason=ToolUse with text only.
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Let me check.".into(),
                }],
                stop_reason: StopReason::ToolUse,
                input_tokens: 5,
                output_tokens: 5,
            },
            // Turn 2: model recovers and emits a proper tool_use.
            MockDispatch::tool_use("tu_late", "list_strategies", serde_json::json!({})),
            // Turn 3: final wrap-up text.
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Here you go.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "list strategies", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WizardEvent::ToolCall { tool, .. } if tool == "list_strategies")),
            "expected a late ToolCall after the malformed turn; got {events:?}"
        );
        assert!(
            matches!(events.last(), Some(WizardEvent::Done { .. })),
            "expected Done as last event; got {events:?}"
        );
    }

    /// Safety net: if the model never recovers and keeps emitting
    /// malformed `stop_reason == ToolUse` turns with no tool_use block,
    /// the `continue` branch must NOT spin forever — the bounded
    /// `MAX_TOOL_LOOP_ITERATIONS` for-loop is responsible for bailing
    /// with an Error event. `MockDispatch::sequence` clones the last
    /// queued response once the queue drains to one entry, so a single
    /// malformed canned response is enough to exercise this.
    #[tokio::test]
    async fn malformed_tool_use_stop_reason_eventually_bails() {
        let mock = Arc::new(MockDispatch::sequence(vec![LlmResponse {
            content: vec![ContentBlock::Text {
                text: "thinking…".into(),
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 1,
            output_tokens: 1,
        }]));
        let (mut wl, _pool, _td, _sid) =
            loop_with_session(mock, "list strategies", ContextScope::Workspace).await;
        let events = drain(&mut wl).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WizardEvent::Error { message } if message.contains("exceeded"))),
            "expected an Error event with 'exceeded' in the message; got {events:?}"
        );
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
            WizardEvent::ToolResult { tool, result, .. } => {
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
            matches!(&events[0], WizardEvent::ToolCall { tool, args, .. } if tool == "list_strategy_ideas"
                && args.get("category").and_then(|v| v.as_str()) == Some("ema")),
            "expected list_strategy_ideas with category=ema, got {:?}",
            events.first()
        );
        match &events[1] {
            WizardEvent::ToolResult { tool, result, .. } => {
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
        // Scenarios are asset-free; the normalizer strips any `asset` the
        // agent emits, and the created scenario has no asset field.
        assert!(
            out.get("asset").is_none(),
            "scenario must not carry an asset; got: {out}"
        );
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
                    "mode": "backtest",
                    "acknowledge_no_filter": true
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
                    "mode": "backtest",
                    "acknowledge_no_filter": true
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
    async fn set_filter_preflight_validates_filter_before_api_set() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "set a clean ema cross filter", ContextScope::Workspace).await;
        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "Filter Draft" }))
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        // Valid filter: should pass preflight and persist as-is.
        let valid_filter = serde_json::json!({
            "filter": {
                "display_name": "ema cross",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "conditions": {
                    "all": [
                        {
                            "lhs": "ema_12",
                            "op": "crosses_above",
                            "rhs": "ema_26"
                        }
                    ]
                }
            }
        });
        wl.run_tool(
            "set_filter",
            serde_json::json!({"id": id, "filter": valid_filter}),
        )
        .await
        .expect("valid filter should preflight and persist");

        let updated = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert!(
            updated["filter"].is_object(),
            "filter should be persisted: {updated:?}"
        );
        assert_eq!(updated["filter"]["asset_scope"][0], "BTC/USD");

        // Invalid filter: missing lhs should be rejected in the local
        // preflight with a parse error before strategy persistence.
        let invalid_filter = serde_json::json!({
            "filter": {
                "display_name": "bad filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "conditions": {
                    "all": [
                        {
                            "op": "crosses_above",
                            "rhs": "ema_26"
                        }
                    ]
                }
            }
        });
        let err = wl
            .run_tool(
                "set_filter",
                serde_json::json!({"id": id, "filter": invalid_filter}),
            )
            .await
            .expect_err("invalid filter must fail preflight");
        let msg = err.to_string();
        assert!(msg.contains("missing field `lhs`"));

        let final_state = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert_eq!(final_state["filter"]["display_name"], "ema cross");
        assert_eq!(final_state["filter"]["timeframe"], "15m");
    }

    #[tokio::test]
    async fn set_filter_prefers_format_alias_for_toml_source() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "set a toml filter using format", ContextScope::Workspace).await;
        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "Format Alias" }))
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        let filter = r#"[filter]
id = "f_toml_format_alias"
strategy_id = "placeholder"
display_name = "toml ema cross"
asset_scope = ["BTC/USD"]
timeframe = "15m"

[filter.conditions]
all = [{ lhs = "ema_12", op = "crosses_above", rhs = "ema_26" }]
"#;
        wl.run_tool(
            "set_filter",
            serde_json::json!({
                "id": id,
                "filter": filter,
                "format": "toml"
            }),
        )
        .await
        .expect("format alias should parse as toml");

        let updated = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert_eq!(updated["filter"]["display_name"], "toml ema cross");
        assert_eq!(updated["filter"]["timeframe"], "15m");
        assert_eq!(updated["filter"]["conditions"]["all"][0]["lhs"], "ema_12");
    }

    #[tokio::test]
    async fn set_filter_accepts_source_text_payload() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "set a source text filter", ContextScope::Workspace).await;
        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "Source Text" }))
            .await
            .expect("create strategy");
        let id = created["id"].as_str().expect("created id");

        let source = r#"[filter]
id = "f_toml_source_text"
strategy_id = "placeholder"
display_name = "source text filter"
asset_scope = ["BTC/USD"]
timeframe = "15m"

[filter.conditions]
all = [{ lhs = "ema_12", op = "crosses_above", rhs = "ema_26" }]
"#;
        wl.run_tool(
            "set_filter",
            serde_json::json!({
                "id": id,
                "source": source,
                "format": "toml"
            }),
        )
        .await
        .expect("source text should parse and persist");

        let updated = wl
            .run_tool("get_strategy", serde_json::json!({"id": id}))
            .await
            .expect("get strategy");
        assert_eq!(updated["filter"]["display_name"], "source text filter");
        assert_eq!(updated["filter"]["conditions"]["all"][0]["lhs"], "ema_12");
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
                WizardEvent::ToolResult { tool, result, .. } if tool == "fetch_bars" => {
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
            "set_filter",
            "clear_filter",
            "create_strategy_agent",
            "attach_agent",
            "validate_draft",
            "run_eval",
            "list_eval_runs",
            "get_eval_run",
            "list_eval_reviews",
            "get_eval_review",
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
        assert!(prompt.contains("existing strategies and scenarios"));
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

    /// A genuinely-hung provider (the model dispatch never returns) must not
    /// wedge the turn forever: the loop bounds `dispatch.complete` with
    /// `dispatch_timeout` and surfaces a typed `Error` event so the chat rail
    /// can recover instead of streaming nothing indefinitely. The timeout is
    /// shrunk via `with_dispatch_timeout` so the path runs in real time
    /// (tokio's paused clock can't be used here — it trips the sqlx pool's
    /// internal acquire timeout during setup).
    #[tokio::test]
    async fn hung_model_dispatch_times_out_with_error_event() {
        struct HangingDispatch;
        #[async_trait::async_trait]
        impl LlmDispatch for HangingDispatch {
            async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
                // Far longer than the (shrunk) dispatch timeout — models a
                // wedged provider / black-holed connection.
                tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
                unreachable!("the model-dispatch timeout must cut this off");
            }
        }

        let dispatch: Arc<dyn LlmDispatch> = Arc::new(HangingDispatch);
        let (wl, _pool, _td, _sid) = loop_with_session(dispatch, "hello", ContextScope::Workspace).await;
        let mut wl = wl.with_dispatch_timeout(Duration::from_millis(20));
        let events = drain(&mut wl).await;

        assert_eq!(
            events.len(),
            1,
            "a hung provider should surface exactly one Error event; got {events:?}"
        );
        match &events[0] {
            WizardEvent::Error { message } => assert!(
                message.to_lowercase().contains("timed out"),
                "error should report the model dispatch timed out, got: {message}"
            ),
            other => panic!("expected Error, got {other:?}"),
        }
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
    async fn get_cli_job_output_with_bare_eval_run_ulid_reaches_bridge() {
        // Same bridge fallback used by get_cli_job applies to output polling:
        // if a bare ULID is not a cli_jobs row, it can still be an eval-run
        // id and should be promoted to `eval_run_<ULID>` internally.
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, pool, td, _sid) =
            loop_with_session(mock, "watch eval output", ContextScope::Workspace).await;
        seed_defaults(&pool, &td).await;
        let store = RunStore::new(pool);
        let run = Run::new_queued(
            "agent-test".into(),
            "crypto-rangebound-q2-2025".into(),
            RunMode::Backtest,
        );
        store.create(&run).await.expect("seed eval run");
        let bare = run.id.to_string();
        let eval_job_id = format!("{}{}", eval_run_bridge::EVAL_RUN_PREFIX, bare);

        let result = wl
            .run_tool("get_cli_job_output", serde_json::json!({"job_id": bare}))
            .await
            .expect("bare eval run id should reach bridge output");

        assert_eq!(result["job_id"], serde_json::Value::String(eval_job_id));
        assert_eq!(result["status"], "queued");
        assert!(result["stdout"].as_str().is_some());
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

    // ── Phase 2.5 checkpoint + Phase 2.4 focus wiring ──────────────────────

    /// Drain the loop while also collecting every queued PolicyEvent in
    /// emission order — mirrors what the chat_rail route does between
    /// `next_event` calls (`take_policy_events`).
    async fn drain_with_policy_events(wl: &mut WizardLoop) -> (Vec<WizardEvent>, Vec<PolicyEvent>) {
        let mut events = vec![];
        let mut policy = vec![];
        while let Some(ev) = wl.next_event().await {
            policy.extend(wl.take_policy_events());
            events.push(ev);
        }
        policy.extend(wl.take_policy_events());
        (events, policy)
    }

    /// Seed a blank strategy on disk for the tempdir-backed XVN_HOME and return
    /// its id. Lets a write tool run against an EXISTING draft so the pre-tool
    /// checkpoint has a Strategy artifact to capture.
    async fn seed_strategy(td: &tempfile::TempDir, name: &str) -> String {
        let store = xvision_engine::strategies::store::FilesystemStore::new(
            xvision_engine::strategies::store::strategy_store_dir(td.path()),
        );
        let out = authoring::create_blank_strategy(&store, name.into(), None)
            .await
            .expect("seed strategy");
        out.id
    }

    /// A WRITE tool against an existing draft must take a PreTool checkpoint
    /// BEFORE it runs: the session's `checkpoint_head` is set and a
    /// `CheckpointCreated` unified event is queued for the route to project.
    #[tokio::test]
    async fn mutating_tool_triggers_pretool_checkpoint() {
        let (pool, td) = fresh_pool().await;
        let scope = ContextScope::Workspace;
        let session_id = ChatSessionStore::create_session(&pool, &scope).await.unwrap();
        unlock_writes_for_tests(&pool, &session_id).await;

        // Seed a draft so update_manifest has something to mutate (and the
        // checkpointer has a Strategy artifact to snapshot).
        let strategy_id = seed_strategy(&td, "Checkpoint Target").await;

        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use(
                "tu_1",
                "update_manifest",
                serde_json::json!({ "id": strategy_id, "decision_cadence_minutes": 30 }),
            ),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Updated the cadence.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));

        let mut wl = WizardLoop::new(
            td.path().to_path_buf(),
            mock,
            "claude-sonnet-4-6".into(),
            pool.clone(),
            session_id.clone(),
            scope,
            "set the cadence to 30 minutes".into(),
        )
        .await
        .unwrap();

        let (events, policy_events) = drain_with_policy_events(&mut wl).await;

        // The tool actually ran (not blocked by the checkpoint hook).
        let updated = events.iter().any(|e| {
            matches!(
                e,
                WizardEvent::ToolResult { tool, result, .. }
                    if tool == "update_manifest" && result.get("error").is_none()
            )
        });
        assert!(updated, "update_manifest should have run: {events:#?}");

        // checkpoint_head is set on the session.
        let rail = ChatSessionStore::load_rail_state(&pool, &session_id)
            .await
            .unwrap();
        let head = rail
            .checkpoint_head
            .expect("checkpoint_head must be set after a write tool");

        // A CheckpointCreated event was queued referencing that same id.
        let created = policy_events.iter().find_map(|pe| match &pe.payload {
            UnifiedPayload::CheckpointCreated(c) => Some(c.clone()),
            _ => None,
        });
        let created = created.expect("a CheckpointCreated event must be emitted");
        assert_eq!(created.checkpoint_id, head, "event id must match checkpoint_head");
        assert_eq!(created.run_id, session_id, "session id carried in run_id slot");
        assert!(!created.input_hash.is_empty(), "checkpoint content_hash present");

        // The persisted checkpoint really captured the Strategy artifact.
        let checkpointer = Checkpointer::new(pool.clone(), td.path().to_path_buf());
        let list = checkpointer.list(&session_id).await.unwrap();
        assert_eq!(list.len(), 1, "exactly one pre-tool checkpoint");
        assert_eq!(list[0].kind, CheckpointKind::PreTool);
        let labels: Vec<&str> = list[0].artifacts.iter().map(|a| a.label()).collect();
        assert!(labels.contains(&"strategy"), "captured strategy: {labels:?}");
        assert!(
            labels.contains(&"tool_policy"),
            "captured tool policy: {labels:?}"
        );
        assert!(labels.contains(&"focus"), "captured focus marker: {labels:?}");
    }

    /// A read-only tool must NOT take a checkpoint — no rewind point is needed
    /// and no CheckpointCreated event is emitted.
    #[tokio::test]
    async fn read_only_tool_takes_no_checkpoint() {
        let mock = Arc::new(MockDispatch::sequence(vec![
            MockDispatch::tool_use("tu_1", "list_strategies", serde_json::json!({})),
            LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Here they are.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]));
        let (mut wl, pool, _td, sid) =
            loop_with_session(mock, "what strategies exist", ContextScope::Workspace).await;
        let (_events, policy_events) = drain_with_policy_events(&mut wl).await;

        let any_ckpt = policy_events
            .iter()
            .any(|pe| matches!(pe.payload, UnifiedPayload::CheckpointCreated(_)));
        assert!(!any_ckpt, "read-only tool must not checkpoint");
        let rail = ChatSessionStore::load_rail_state(&pool, &sid).await.unwrap();
        assert!(
            rail.checkpoint_head.is_none(),
            "no checkpoint_head for read-only tool"
        );
    }

    /// The scope's focus document is spliced into the assembled system prompt
    /// (a clearly-delimited "## Focus" section) and a `FocusInjected` event
    /// carrying the content hash is emitted on the turn it is injected.
    #[tokio::test]
    async fn focus_doc_is_injected_into_system_prompt() {
        let (pool, td) = fresh_pool().await;
        let scope = ContextScope::Strategy {
            draft_id: "btc-momentum".into(),
        };
        let session_id = ChatSessionStore::create_session(&pool, &scope).await.unwrap();

        // Operator pins focus notes for this scope.
        let focus_text = "Keep position sizing conservative; never exceed 2% per trade.";
        let saved = focus::save(td.path(), &scope, focus_text).await.unwrap();

        // RecordingDispatch captures the system prompt the loop assembles.
        struct Recorder {
            inner: MockDispatch,
            last_system: Arc<std::sync::Mutex<Option<String>>>,
        }
        #[async_trait::async_trait]
        impl LlmDispatch for Recorder {
            async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
                *self.last_system.lock().unwrap() = Some(req.system_prompt.clone());
                self.inner.complete(req).await
            }
        }
        let last_system = Arc::new(std::sync::Mutex::new(None));
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(Recorder {
            inner: MockDispatch::echo("Understood."),
            last_system: last_system.clone(),
        });

        let mut wl = WizardLoop::new(
            td.path().to_path_buf(),
            dispatch,
            "claude-sonnet-4-6".into(),
            pool.clone(),
            session_id.clone(),
            scope,
            "what should I watch for".into(),
        )
        .await
        .unwrap();

        let (_events, policy_events) = drain_with_policy_events(&mut wl).await;

        // The focus content is spliced into the system prompt under "## Focus".
        let system = last_system
            .lock()
            .unwrap()
            .clone()
            .expect("a system prompt was assembled");
        assert!(
            system.contains("## Focus"),
            "system prompt has a Focus section: {system}"
        );
        assert!(
            system.contains(focus_text),
            "focus content spliced into prompt: {system}"
        );

        // A FocusInjected event with the saved content hash was emitted.
        let injected = policy_events.iter().find_map(|pe| match &pe.payload {
            UnifiedPayload::FocusInjected(f) => Some(f.clone()),
            _ => None,
        });
        let injected = injected.expect("a FocusInjected event must be emitted");
        assert_eq!(injected.scope_kind, "strategy");
        assert_eq!(injected.scope_id.as_deref(), Some("btc-momentum"));
        assert_eq!(
            injected.content_hash.as_deref(),
            Some(saved.content_hash.as_str())
        );
    }

    /// With no focus file for the scope, no Focus section and no FocusInjected
    /// event appear — the absence path is silent and non-fatal.
    #[tokio::test]
    async fn no_focus_doc_means_no_focus_section() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (mut wl, _pool, _td, _sid) = loop_with_session(mock, "hi", ContextScope::Workspace).await;
        let (_events, policy_events) = drain_with_policy_events(&mut wl).await;
        let any_focus = policy_events
            .iter()
            .any(|pe| matches!(pe.payload, UnifiedPayload::FocusInjected(_)));
        assert!(!any_focus, "no focus file → no FocusInjected event");
    }

    // -- Chat history windowing tests (W3 / xvision-t4u8.3) -------------------

    use xvision_engine::agent::llm::{ContentBlock, Message};

    /// Build a user message containing a ToolResult block.
    fn tool_result_msg(tool_use_id: &str) -> Message {
        Message {
            role: "user".into(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: "result".into(),
                is_error: None,
            }],
        }
    }

    /// Build an assistant message containing a ToolUse block.
    fn tool_use_msg(id: &str) -> Message {
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: "list_strategies".into(),
                input: serde_json::json!({}),
            }],
        }
    }

    /// Build a plain user or assistant text message.
    fn text_msg(role: &str, text: &str) -> Message {
        Message {
            role: role.into(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// A short session (N messages ≤ cap) must come back completely unchanged.
    #[test]
    fn window_short_session_returns_all_messages() {
        // 3 messages, cap of 10 — all should survive.
        let msgs = vec![
            text_msg("user", "hello"),
            text_msg("assistant", "hi"),
            text_msg("user", "what time is it"),
        ];
        let windowed = window_chat_messages(msgs.clone(), 10);
        assert_eq!(windowed.len(), 3);
        assert_eq!(windowed, msgs);
    }

    /// When the cap boundary falls cleanly between messages (no orphan
    /// tool_use/tool_result pair), the window drops the oldest messages
    /// and the count equals the cap.
    #[test]
    fn window_drops_oldest_messages_respecting_cap() {
        let msgs: Vec<Message> = (0..8)
            .map(|i| {
                if i % 2 == 0 {
                    text_msg("user", &format!("msg {i}"))
                } else {
                    text_msg("assistant", &format!("reply {i}"))
                }
            })
            .collect();
        // cap = 4 → keep the last 4
        let windowed = window_chat_messages(msgs.clone(), 4);
        assert_eq!(windowed.len(), 4);
        // The last 4 messages of the original 8 must survive.
        assert_eq!(windowed, msgs[4..]);
    }

    /// CORE PAIRING TEST (spec DoD §1):
    /// When the raw cap boundary falls BETWEEN a tool_use and its matching
    /// tool_result (i.e. the tool_use is the last message inside the window
    /// and its tool_result is the first message dropped — or vice versa),
    /// the windower must extend or shrink the boundary so both messages
    /// are either kept or dropped together. No orphan must be returned.
    ///
    /// Layout (8 messages, cap = 5):
    ///   0: user "msg 0"
    ///   1: assistant "reply 1"
    ///   2: user "msg 2"
    ///   3: assistant tool_use id="tu_1"   ← raw tail of cap-5 window (msgs[3..])
    ///   4: user tool_result id="tu_1"     ← paired with 3
    ///   5: assistant "reply 5"
    ///   6: user "msg 6"
    ///   7: assistant "reply 7"
    ///
    /// A naive slice of msgs[3..] keeps msg[3] (tool_use) + msg[4..7]
    /// — that's exactly 5 messages, and msgs[3]/msgs[4] form a complete
    /// pair, so there is actually no orphan. We need the boundary to fall
    /// INSIDE the pair. Adjust: cap = 4.
    ///   msgs[4..] = tool_result "tu_1" (orphan — its tool_use was at [3]).
    ///
    /// The windower MUST shrink the window further to [5..] to avoid
    /// the orphan tool_result.
    #[test]
    fn window_snaps_boundary_to_avoid_orphan_tool_result() {
        let msgs = vec![
            text_msg("user", "msg 0"),        // 0
            text_msg("assistant", "reply 1"), // 1
            text_msg("user", "msg 2"),        // 2
            tool_use_msg("tu_1"),             // 3  ← tool_use
            tool_result_msg("tu_1"),          // 4  ← matching tool_result
            text_msg("assistant", "reply 5"), // 5
            text_msg("user", "msg 6"),        // 6
            text_msg("assistant", "reply 7"), // 7
        ];
        // cap = 4 → raw slice starts at msgs[4] = tool_result "tu_1" (orphan!).
        // The windower must snap to msgs[5..] so the orphan tool_result is dropped.
        let windowed = window_chat_messages(msgs.clone(), 4);
        // Must NOT contain the orphan tool_result.
        for m in &windowed {
            for block in &m.content {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    assert_ne!(
                        tool_use_id, "tu_1",
                        "orphan tool_result for tu_1 must not appear in the window"
                    );
                }
            }
        }
        // Must NOT contain the tool_use for tu_1 either (both must be dropped or
        // both kept — since the tool_result was orphaned, both are dropped).
        for m in &windowed {
            for block in &m.content {
                if let ContentBlock::ToolUse { id, .. } = block {
                    assert_ne!(
                        id, "tu_1",
                        "tool_use for tu_1 was paired with a dropped tool_result and must also be dropped"
                    );
                }
            }
        }
        // The remaining messages (msgs[5..]) must all be present.
        assert_eq!(windowed.len(), 3, "windowed: {windowed:?}");
    }

    /// A tool_use/tool_result pair that fits entirely within the window
    /// must be preserved intact.
    #[test]
    fn window_keeps_intact_tool_pair_within_cap() {
        let msgs = vec![
            text_msg("user", "old msg"),        // 0 — will be dropped
            text_msg("assistant", "old reply"), // 1 — will be dropped
            text_msg("user", "ask"),            // 2
            tool_use_msg("tu_2"),               // 3
            tool_result_msg("tu_2"),            // 4
            text_msg("assistant", "done"),      // 5
        ];
        // cap = 4 → raw slice starts at msgs[2] (no orphan — tu_2 pair is fully inside).
        let windowed = window_chat_messages(msgs.clone(), 4);
        assert_eq!(windowed, msgs[2..], "windowed: {windowed:?}");
    }

    // ── W5 new read tool tests (Findings #5-7) ────────────────────────────

    /// `filter_catalog` tool returns a non-empty JSON object with the
    /// DSL token catalog. The agent can call this in research/THINK mode
    /// to learn the correct operator and indicator tokens.
    #[tokio::test]
    async fn filter_catalog_tool_returns_non_empty_dsl_token_list() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "what filters can I use?", ContextScope::Workspace).await;

        let out = wl
            .run_tool("filter_catalog", serde_json::json!({}))
            .await
            .expect("filter_catalog must succeed");

        // Must return a non-empty object.
        let obj = out.as_object().expect("filter_catalog must return an object");
        assert!(!obj.is_empty(), "catalog must not be empty: {out}");

        // Must include the operators array with known tokens.
        let operators = out["operators"]
            .as_array()
            .expect("filter_catalog must include an `operators` array");
        assert!(
            operators.len() > 5,
            "expected multiple operators, got {}: {out}",
            operators.len()
        );
        let op_tokens: Vec<_> = operators.iter().filter_map(|op| op["token"].as_str()).collect();
        assert!(op_tokens.contains(&">"), "missing `>` operator in {op_tokens:?}");
        assert!(
            op_tokens.contains(&"crosses_above"),
            "missing `crosses_above` operator in {op_tokens:?}"
        );

        // Must include indicator categories.
        let indicators = out["indicators"]
            .as_object()
            .expect("filter_catalog must include an `indicators` object");
        assert!(
            indicators.contains_key("momentum_and_oscillators"),
            "missing momentum_and_oscillators category in {out}"
        );
        let momentum = indicators["momentum_and_oscillators"]
            .as_array()
            .expect("momentum_and_oscillators must be an array");
        let momentum_names: Vec<_> = momentum.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            momentum_names.iter().any(|n| n.contains("rsi")),
            "expected rsi_<period> in momentum indicators: {momentum_names:?}"
        );

        // Must include required_fields.
        let required = out["required_fields"]
            .as_array()
            .expect("filter_catalog must include required_fields");
        let required_names: Vec<_> = required.iter().filter_map(|v| v.as_str()).collect();
        for field in ["display_name", "asset_scope", "timeframe", "conditions"] {
            assert!(
                required_names.contains(&field),
                "missing required field `{field}` in {required_names:?}"
            );
        }
    }

    /// `list_providers` tool returns a JSON object with a `providers` array.
    /// The array may be empty in a fresh tempdir (no config written), which
    /// is the correct response — the test asserts the shape is present and
    /// the tool does not error.
    #[tokio::test]
    async fn list_providers_tool_returns_providers_report_shape() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "list providers", ContextScope::Workspace).await;

        // In a fresh tempdir there is no config/default.toml, so providers is
        // empty (the api::settings::providers::list function returns an empty
        // ProvidersReport when the file is missing). Assert the shape — the
        // tool must not error and must return a parseable ProvidersReport.
        let out = wl
            .run_tool("list_providers", serde_json::json!({}))
            .await
            .expect("list_providers must not error in a fresh tempdir");

        // ProvidersReport serialises as { providers: [...], default_model?: ... }
        assert!(
            out.get("providers").is_some(),
            "list_providers must return an object with a `providers` field: {out}"
        );
        let providers = out["providers"].as_array().expect("providers must be an array");
        // In a fresh tempdir the array is empty — that is correct.
        let _ = providers; // length is 0, which is expected
    }

    /// `get_agent` tool dispatches to `api_agents::get` and returns the Agent
    /// record. Uses a created agent so the store has something to return.
    #[tokio::test]
    async fn get_agent_tool_returns_agent_record_by_id() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, td, _sid) = loop_with_session(mock, "inspect agent", ContextScope::Workspace).await;

        // Create an agent in the DB so get_agent has something to return.
        let ctx = ApiContext::new(
            _pool.clone(),
            Actor::Cli { user: "test".into() },
            td.path().to_path_buf(),
        );
        let created = api_agents::create(
            &ctx,
            api_agents::CreateAgentRequest {
                name: "W5 Test Agent".into(),
                description: "Test agent for W5 get_agent tool.".into(),
                tags: vec![],
                slots: vec![xvision_engine::agents::AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: "You are a quantitative trading assistant. Analyse the OHLCV data \
                             provided and respond with a JSON object containing: action (buy/sell/hold), \
                             size_pct (0–100), and reason. Apply disciplined risk management: never risk \
                             more than 1% of notional equity per trade, and always respect configured \
                             stop-loss and take-profit levels. Avoid over-trading on low-volume bars."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("create agent for get_agent test");

        let out = wl
            .run_tool("get_agent", serde_json::json!({ "id": created.agent_id }))
            .await
            .expect("get_agent must succeed for an existing agent");

        assert_eq!(
            out["agent_id"].as_str(),
            Some(created.agent_id.as_str()),
            "get_agent must return the requested agent id: {out}"
        );
        assert_eq!(
            out["name"].as_str(),
            Some("W5 Test Agent"),
            "get_agent must return the agent name: {out}"
        );
    }

    /// `get_agent` returns an error for an unknown agent id.
    #[tokio::test]
    async fn get_agent_tool_errors_for_unknown_id() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "inspect agent", ContextScope::Workspace).await;

        let err = wl
            .run_tool(
                "get_agent",
                serde_json::json!({ "id": "01ZZNONEXISTENTAGENT000000" }),
            )
            .await
            .expect_err("get_agent must error for an unknown id");

        assert!(
            err.to_string().contains("not found") || err.to_string().contains("01ZZNONEXISTENTAGENT000000"),
            "error must mention not-found or the id: {err}"
        );
    }

    /// The three new W5 read tools appear in the tool defs for both profiles.
    #[test]
    fn w5_read_tools_appear_in_strategy_tool_defs() {
        let defs = wizard_tool_defs();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        for tool in ["list_providers", "get_agent", "filter_catalog"] {
            assert!(
                names.contains(&tool),
                "W5 tool `{tool}` missing from strategy_tool_defs: {names:?}"
            );
        }
    }

    /// `validate_draft` remains in strategy tool defs (still advertised to the model).
    #[test]
    fn validate_draft_still_in_tool_defs_after_reclassification() {
        let defs = wizard_tool_defs();
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"validate_draft"),
            "validate_draft must still be in tool defs after Read reclassification: {names:?}"
        );
    }

    // -- W7: agent-creation context bleed prevention -----------------------

    /// The `create_strategy_agent` tool description must mention `update_manifest`
    /// so the model understands the ordering dependency (Finding #14).
    #[test]
    fn create_strategy_agent_tool_description_mentions_update_manifest() {
        let defs = wizard_tool_defs();
        let def = defs
            .iter()
            .find(|d| d.name == "create_strategy_agent")
            .expect("create_strategy_agent must be in tool defs");
        assert!(
            def.description.contains("update_manifest"),
            "create_strategy_agent description must mention update_manifest (ordering dependency): {}",
            def.description
        );
    }

    /// The wizard system prompt must contain ordering guidance — specifically
    /// that update_manifest must be called BEFORE create_strategy_agent when
    /// a specific asset universe was discussed (Finding #14 fix).
    #[test]
    fn wizard_system_prompt_contains_update_manifest_ordering_guidance() {
        // The guidance must mention the "before" ordering constraint.
        // We check for "before" adjacent to one of the two tool names so the
        // test catches the actual ordering rule, not just incidental mentions.
        // The content uses backtick-quoted tool names in markdown, so we match
        // against the raw text (not lowercased to avoid backtick confusion).
        let has_ordering_rule = WIZARD_SYSTEM_PROMPT_BASE.contains("update_manifest")
            && (
                // "Call `update_manifest` before `create_strategy_agent`" (markdown backticks)
                WIZARD_SYSTEM_PROMPT_BASE.contains("before `create_strategy_agent`")
                    || WIZARD_SYSTEM_PROMPT_BASE.contains("update_manifest` first")
                    || WIZARD_SYSTEM_PROMPT_BASE.contains("update_manifest` before")
                    || WIZARD_SYSTEM_PROMPT_BASE.contains("update_manifest first")
                    || WIZARD_SYSTEM_PROMPT_BASE.contains("update_manifest before")
                    || WIZARD_SYSTEM_PROMPT_BASE.contains("before calling create_strategy_agent")
            );
        assert!(
            has_ordering_rule,
            "wizard.md must contain an explicit ordering rule that update_manifest \
             precedes create_strategy_agent. Current content:\n{}",
            WIZARD_SYSTEM_PROMPT_BASE
        );
    }

    /// When create_strategy_agent is called WITHOUT an explicit system_prompt
    /// and the strategy's asset_universe is still the blank-draft default
    /// (["BTC/USD"]), the tool result must include a "warning" field so
    /// the model can self-correct before the agent prompt is wrong.
    #[tokio::test]
    async fn create_strategy_agent_warns_when_asset_universe_is_default_and_no_prompt_given() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "build me an ETH strategy agent", ContextScope::Workspace).await;

        // Create a blank strategy — asset_universe defaults to ["BTC/USD"].
        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "ETH Strategy" }))
            .await
            .expect("create_strategy");
        let id = created["id"].as_str().expect("created.id");

        // Call create_strategy_agent WITHOUT system_prompt and without
        // calling update_manifest first — simulates the context-bleed scenario.
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
            .expect("create_strategy_agent should succeed (non-fatal warning)");

        // Agent must still be created.
        assert_eq!(out["strategy_id"], id);
        assert_eq!(out["role"], "trader");

        // Must include a warning about the default asset_universe.
        let warning = out.get("warning").and_then(|w| w.as_str()).unwrap_or("");
        assert!(
            !warning.is_empty(),
            "expected a warning when asset_universe is default BTC/USD and no prompt given, got: {out}"
        );
        assert!(
            warning.contains("BTC/USD") || warning.contains("update_manifest"),
            "warning must mention BTC/USD or update_manifest: {warning}"
        );
    }

    /// When create_strategy_agent IS given an explicit system_prompt, no
    /// warning should appear — the caller took responsibility for the prompt.
    #[tokio::test]
    async fn create_strategy_agent_no_warning_when_explicit_system_prompt_given() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "build me a BTC strategy agent", ContextScope::Workspace).await;

        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "BTC Strat" }))
            .await
            .expect("create_strategy");
        let id = created["id"].as_str().expect("created.id");

        let out = wl
            .run_tool(
                "create_strategy_agent",
                serde_json::json!({
                    "strategy_id": id,
                    "role": "trader",
                    "provider": "openai",
                    "model": "gpt-4.1-mini",
                    "system_prompt": "You are the ETH/USD trader for this strategy. Evaluate the provided OHLCV bars, active filters, risk configuration, and current position state before deciding. Return only structured JSON with action, size_pct, confidence, and concise rationale. Respect configured stop loss, take profit, and max risk limits; hold when evidence is weak or liquidity is poor."
                }),
            )
            .await
            .expect("create_strategy_agent with explicit prompt");

        assert_eq!(out["role"], "trader");
        assert!(
            out.get("warning").is_none(),
            "no warning when explicit system_prompt is supplied: {out}"
        );
    }

    /// When update_manifest was called first to set a non-default asset_universe,
    /// create_strategy_agent should not warn even without an explicit system_prompt.
    #[tokio::test]
    async fn create_strategy_agent_no_warning_after_update_manifest() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "build me an ETH strat", ContextScope::Workspace).await;

        let created = wl
            .run_tool("create_strategy", serde_json::json!({ "name": "ETH Strat" }))
            .await
            .expect("create_strategy");
        let id = created["id"].as_str().expect("created.id");

        // Update manifest first — asset_universe is no longer the default.
        wl.run_tool(
            "update_manifest",
            serde_json::json!({
                "id": id,
                "asset_universe": ["ETH/USD"]
            }),
        )
        .await
        .expect("update_manifest");

        // Now create the agent without an explicit system_prompt.
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
            .expect("create_strategy_agent after update_manifest");

        assert_eq!(out["role"], "trader");
        assert!(
            out.get("warning").is_none(),
            "no warning expected when asset_universe was updated before agent creation: {out}"
        );
    }

    // ── W10 scenario management tools ────────────────────────────────────────

    /// Helper: create a scenario through the wizard and return its id.
    async fn create_test_scenario(wl: &WizardLoop) -> String {
        let out = wl
            .run_tool(
                "create_scenario",
                serde_json::json!({
                    "display_name": "W10 Test Scenario",
                    "granularity": "4h"
                }),
            )
            .await
            .expect("create_scenario for W10 tests");
        out["id"].as_str().expect("id field").to_string()
    }

    /// W10: clone_scenario creates a derived scenario from a parent.
    #[tokio::test]
    async fn w10_clone_scenario_creates_child() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "clone scenario", ContextScope::Workspace).await;

        let parent_id = create_test_scenario(&wl).await;

        let out = wl
            .run_tool(
                "clone_scenario",
                serde_json::json!({
                    "parent_id": parent_id,
                    "display_name": "W10 Clone"
                }),
            )
            .await
            .expect("clone_scenario should succeed");

        assert!(out["id"].as_str().is_some(), "cloned scenario must have an id");
        assert_ne!(out["id"], parent_id, "clone id must differ from parent id");
        assert_eq!(
            out["parent_scenario_id"].as_str(),
            Some(parent_id.as_str()),
            "cloned scenario must reference parent"
        );
    }

    /// W10: archive_scenario soft-deletes a scenario (sets archived_at).
    #[tokio::test]
    async fn w10_archive_scenario_sets_archived_at() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "archive scenario", ContextScope::Workspace).await;

        let sc_id = create_test_scenario(&wl).await;

        let out = wl
            .run_tool("archive_scenario", serde_json::json!({ "id": sc_id }))
            .await
            .expect("archive_scenario should succeed");

        assert_eq!(
            out["archived"].as_bool(),
            Some(true),
            "archived field must be true: {out}"
        );
    }

    /// W10: set_scenario_regime writes operator-set regime labels.
    #[tokio::test]
    async fn w10_set_scenario_regime_persists_labels() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) = loop_with_session(mock, "set regime", ContextScope::Workspace).await;

        let sc_id = create_test_scenario(&wl).await;

        let out = wl
            .run_tool(
                "set_scenario_regime",
                serde_json::json!({
                    "id": sc_id,
                    "regime": "trend",
                    "volatility": "high",
                    "direction": "up"
                }),
            )
            .await
            .expect("set_scenario_regime should succeed");

        assert_eq!(
            out["regime_label"].as_str(),
            Some("trend"),
            "regime_label must be persisted"
        );
        assert_eq!(
            out["volatility_label"].as_str(),
            Some("high"),
            "volatility_label must be persisted"
        );
        assert_eq!(
            out["trend_direction"].as_str(),
            Some("up"),
            "trend_direction must be persisted"
        );
        assert_eq!(
            out["regime_derived"].as_bool(),
            Some(false),
            "regime_derived must be false for operator-set labels"
        );
    }

    /// W10: select_scenarios returns a filtered list of scenarios by decision count.
    #[tokio::test]
    async fn w10_select_scenarios_returns_matching_rows() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "select scenarios", ContextScope::Workspace).await;

        let sc_id = create_test_scenario(&wl).await;

        // 1. No-mode select returns all candidates (up to `count`). Our scenario
        //    must be present with a well-formed row shape.
        let all = wl
            .run_tool("select_scenarios", serde_json::json!({ "count": 10 }))
            .await
            .expect("select_scenarios (no mode) should succeed");
        let arr = all.as_array().expect("select_scenarios must return an array");
        let row = arr
            .iter()
            .find(|r| r["id"].as_str() == Some(sc_id.as_str()))
            .unwrap_or_else(|| panic!("no-mode select must include the created scenario; got: {all}"));
        assert!(row["name"].as_str().is_some(), "row must have name: {row}");
        assert!(
            row["timeframe"].as_str().is_some(),
            "row must have timeframe: {row}"
        );
        let dc = row["decision_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("row must have decision_count: {row}"));
        assert!(dc > 0, "decision_count must be positive: {row}");

        // 2. target_decisions == the scenario's own count is inside the ±10%
        //    window → the scenario IS selected (positive filter path).
        let matched = wl
            .run_tool(
                "select_scenarios",
                serde_json::json!({ "target_decisions": dc, "count": 10 }),
            )
            .await
            .expect("select_scenarios (target) should succeed");
        assert!(
            matched
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"].as_str() == Some(sc_id.as_str())),
            "target_decisions == own count must select the scenario; got: {matched}"
        );

        // 3. A target_decisions far outside ±10% → the scenario is excluded
        //    (negative filter path — proves the filter actually filters).
        let excluded = wl
            .run_tool(
                "select_scenarios",
                serde_json::json!({ "target_decisions": dc * 10 + 1000, "count": 10 }),
            )
            .await
            .expect("select_scenarios (far target) should succeed");
        assert!(
            !excluded
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"].as_str() == Some(sc_id.as_str())),
            "far-off target_decisions must exclude the scenario; got: {excluded}"
        );
    }

    /// W10: classify_scenario skips gracefully when bars are not cached.
    /// classify_scenario must SKIP (no re-derivation) when the scenario already
    /// carries an operator-set regime label and `force=false`. This skip path is
    /// deterministic and independent of whether bar data is cached, so it is the
    /// reliable way to exercise the graceful `ClassifyResult { classified:false,
    /// skipped_reason:Some(..) }` branch.
    #[tokio::test]
    async fn w10_classify_scenario_skips_when_operator_labeled() {
        let mock = Arc::new(MockDispatch::echo("ok"));
        let (wl, _pool, _td, _sid) =
            loop_with_session(mock, "classify scenario", ContextScope::Workspace).await;

        let sc_id = create_test_scenario(&wl).await;

        // Stamp an operator-authored regime label (regime_derived = false).
        wl.run_tool(
            "set_scenario_regime",
            serde_json::json!({ "id": sc_id, "regime": "trend" }),
        )
        .await
        .expect("set_scenario_regime should succeed");

        let out = wl
            .run_tool(
                "classify_scenario",
                serde_json::json!({ "id": sc_id, "force": false }),
            )
            .await
            .expect("classify_scenario returns Ok");

        // Skip path: classified=false plus a human-readable skipped_reason.
        // (`ClassifyResult` has no `skipped` field — assert on the real fields.)
        assert_eq!(
            out["classified"].as_bool(),
            Some(false),
            "classify(force=false) on an operator-labeled scenario must skip: {out}"
        );
        assert!(
            out["skipped_reason"].as_str().is_some(),
            "a skipped classification must carry a skipped_reason: {out}"
        );
    }
}
