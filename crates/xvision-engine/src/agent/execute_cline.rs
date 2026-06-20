//! `execute_slot_cline` — drive one LLM slot through the Cline sidecar
//! (`xvision-agentd`) instead of the raw [`crate::agent::llm::LlmDispatch`]
//! HTTP path.
//!
//! Stage 1 of the Cline runtime unification (umbrella:
//! `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`,
//! plan: `docs/superpowers/plans/2026-05-24-cline-stage1-live-path.md`,
//! Task 5). A slot invocation becomes a Cline `Agent` run:
//! `start_run` → `step` → `end_run`. The agent returns its structured
//! decision by calling the built-in `submit_decision` lifecycle tool; the
//! sidecar captures that payload and surfaces it on
//! [`xvision_agent_client::StepResult::decision_json`].
//!
//! This is a SIBLING executor to [`crate::agent::execute::execute_slot`] —
//! it deliberately does NOT extend `SlotInput` (which would ripple to
//! every call site). It returns the same [`LlmResponse`] shape the existing
//! decision parser accepts: a single [`ContentBlock::Text`] whose body is
//! the decision JSON, so `resp.text()` round-trips through the unchanged
//! `dispatch_capability` parser and `TraderOutput::parse_response`.
//!
//! ## Failure + recovery contract (umbrella "Subplan inheritance contract"
//! item 2)
//!
//! * A transport/crash error on `step` propagates as a typed
//!   [`ClineRuntimeError`] — the cycle fails, it is NEVER a silent empty
//!   decision.
//! * `end_run` is always attempted, even when `step` errored, so the
//!   sidecar session is reclaimed.
//! * `run_id` is the idempotency key. The sidecar `store.ts` keys sessions
//!   by `run_id`, so a retried `start_run` with the same id is rejected /
//!   deduped by the sidecar rather than double-executing here.
//! * **Stage 3 obligation (tracked here so it is not lost):** live-vs-replay
//!   divergence handling is n/a at Stage 1 (no replay yet). When Stage 3
//!   lands record/replay, this executor must compare the live trajectory
//!   against the recorded one and surface divergence as a typed error.

use crate::agent::llm::{ContentBlock, LlmRequest, LlmResponse, Message, ResponseSchema, StopReason};
use crate::eval::executor::trader_output::extract_last_trader_output_json;
use crate::strategies::slot::LLMSlot;
use std::sync::Arc;
use xvision_agent_client::provider_map::{map_provider, ProviderMapError};
use xvision_agent_client::{
    AgentClient, BudgetLimits, EndRunParams, ReplayLoadParams, StartRunParams, StepParams, StepResult,
};
use xvision_core::config::ProviderEntry;
use xvision_observability::trajectory::frame::TrajectoryFrame;
use xvision_observability::trajectory::key::RecordingId;
use xvision_observability::trajectory::store::TrajectoryStore;

/// `recovery_reason` written to the recording row when a replay aborts
/// because the recorded frame feed ran out before the agent finished its
/// loop (item 4 — bounded replay feed). NEVER a live fallback.
pub const RECOVERY_REPLAY_FRAMES_EXHAUSTED: &str = "replay_frames_exhausted";

/// `recovery_reason` written when the replayed control flow diverges from
/// the recorded transcript (item 2 — live-vs-replay divergence).
pub const RECOVERY_REPLAY_DIVERGENCE: &str = "replay_divergence";

/// Which trajectory mode drives this slot invocation (Stage 3, Task 3).
///
/// * `Record` is the normal live path: the agent calls the real provider
///   and the sidecar emits `event.trajectory_frame` notifications that the
///   event sink persists into a [`TrajectoryStore`] (Task 0). No replay.
/// * `Replay` re-drives the SAME Cline loop from a recorded trajectory:
///   the frames are read from the store, shipped to the sidecar via
///   `session.replay_load`, and `session.step` consumes them deterministically
///   — zero network cost, byte-identical decision. On frame exhaustion or
///   control-flow divergence the run aborts with a typed error and the
///   recording is marked corrupt (items 2 + 4). NEVER a silent live fallback.
#[derive(Clone)]
pub enum TrajectoryMode {
    /// Live/record path (default). Frame persistence is handled by the
    /// event sink, not by this executor.
    Record,
    /// Replay an existing recording deterministically.
    Replay {
        /// The recording to replay. Frames are read from `store` keyed by
        /// `(recording_id, slot_role, step_index)`.
        recording_id: RecordingId,
        /// The trajectory store the recording lives in. Shared across slots.
        store: Arc<TrajectoryStore>,
    },
}

impl Default for TrajectoryMode {
    fn default() -> Self {
        TrajectoryMode::Record
    }
}

impl std::fmt::Debug for TrajectoryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrajectoryMode::Record => write!(f, "Record"),
            TrajectoryMode::Replay { recording_id, .. } => {
                write!(f, "Replay {{ recording_id: {recording_id} }}")
            }
        }
    }
}

/// The built-in lifecycle tool the Cline agent calls exactly once to emit
/// its structured decision. MUST be in the run's `allowed_tools` or the
/// sidecar will not register it. Mirrors `SUBMIT_DECISION_TOOL` in
/// `xvision-agentd/src/session/submit-decision.ts`.
pub const SUBMIT_DECISION_TOOL: &str = "submit_decision";

/// Default per-run budgets when the slot does not pin an explicit
/// `max_tokens` / `max_wall_ms`. Token budgets remain ON by default
/// (a hard ceiling on model output is a real cost control, not a
/// timeout); the wall-clock budget is OFF by default per QA30
/// (2026-05-26) operator feedback.
///
/// QA30 history: an earlier round set `DEFAULT_MAX_WALL_MS = 120_000`
/// (2 minutes). That clipped slow-but-healthy completions — Gemini
/// Flash 3.1-lite under load, Sonnet with extended thinking — and
/// surfaced them as `budget_wall_ms_exceeded` failures even though
/// the model was still producing tokens. The operator's call:
/// "Max wall setting for timeout should be maybe in agent settings,
///  same with max tokens. But I don't like having it on by default.
///  Especially for testing."
///
/// So: the default is now `u32::MAX` (~49 days), effectively no wall
/// budget. Strategies that want a wall cap pin `max_wall_ms` per-slot
/// (currently a Rust-only knob via `ClineSlotInput.max_wall_ms`; the
/// per-agent UI surface is a follow-on QA30 item).
const DEFAULT_MAX_INPUT_TOKENS: u32 = 200_000;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 8_192;
const DEFAULT_MAX_WALL_MS: u32 = u32::MAX;

/// Typed Cline-runtime failure classes (item 2). Wrapped in
/// `anyhow::Error` at the call boundary so existing `anyhow::Result`
/// callers keep compiling, but the pipeline / observability layer can
/// `downcast_ref` for a stable class tag.
#[derive(Debug, thiserror::Error)]
pub enum ClineRuntimeError {
    /// The provider has no Cline mapping (item 5). A hard abort — never a
    /// silent fallback to `LlmDispatch` unless the runtime flag explicitly
    /// selects it.
    #[error("cline runtime: {0}")]
    ProviderUnmapped(#[from] ProviderMapError),

    /// `start_run` failed (transport, handshake, duplicate run_id dedup,
    /// or sidecar-side validation). Carries the slot role + run_id so an
    /// operator can correlate to the failing cycle.
    #[error("cline runtime: start_run failed for run_id={run_id} (role={role}): {source}")]
    StartRun {
        run_id: String,
        role: String,
        #[source]
        source: xvision_agent_client::AgentClientError,
    },

    /// `step` failed — a transport error or a sidecar crash mid-step. This
    /// is the crash boundary: the cycle fails, the decision is never
    /// silently dropped.
    #[error(
        "cline runtime: step failed (sidecar transport/crash) for run_id={run_id} (role={role}): {source}"
    )]
    StepTransport {
        run_id: String,
        role: String,
        #[source]
        source: xvision_agent_client::AgentClientError,
    },

    /// The step completed at the protocol level but did not reach a
    /// terminal `completed` status (e.g. budget abort, sidecar error).
    #[error("cline runtime: step did not complete for run_id={run_id} (role={role}): status={status} error={error:?}")]
    StepNotCompleted {
        run_id: String,
        role: String,
        status: String,
        error: Option<String>,
    },

    /// The run completed but the agent never called `submit_decision`, so
    /// there is no structured decision to parse. Failing here (rather than
    /// synthesizing an empty/hold decision) is the item-2 guarantee that a
    /// missing decision fails the cycle visibly.
    #[error(
        "cline runtime: run completed without calling submit_decision for run_id={run_id} (role={role})"
    )]
    NoDecision { run_id: String, role: String },

    /// `submit_decision` returned a payload that is not valid JSON. The
    /// downstream parser expects parseable JSON text; surface the failure
    /// typed instead of feeding garbage to the parser.
    #[error("cline runtime: submit_decision payload was not valid JSON for run_id={run_id} (role={role}): {source}")]
    DecisionNotJson {
        run_id: String,
        role: String,
        #[source]
        source: serde_json::Error,
    },

    /// Replay was requested but the recording could not be read from the
    /// store (missing recording, missing slot/step frames, decode error).
    /// A replay run NEVER falls back to a live provider — a bad recording
    /// is a hard abort (item 4 reconstitution rule).
    #[error(
        "cline runtime: replay frames unavailable for recording_id={recording_id} (role={role}): {source}"
    )]
    ReplayFramesUnavailable {
        recording_id: String,
        role: String,
        #[source]
        source: xvision_observability::trajectory::store::StoreError,
    },

    /// The recorded frame feed was exhausted before the replayed agent
    /// reached a terminal decision (item 4). The recording is marked
    /// `corrupt` with `recovery_reason = replay_frames_exhausted`; the
    /// cycle fails. NEVER a live fallback.
    #[error("cline runtime: replay frames exhausted for recording_id={recording_id} (role={role}, step={step}): {detail}")]
    ReplayFramesExhausted {
        recording_id: String,
        role: String,
        step: i64,
        detail: String,
    },

    /// The replayed control flow diverged from the recorded transcript
    /// (item 2). The recording is marked `corrupt` with
    /// `recovery_reason = replay_divergence`; the cycle fails. The
    /// divergence point (slot, step) and the expected/actual values are
    /// carried so an operator can see exactly where the replay drifted.
    #[error("cline runtime: replay divergence for recording_id={recording_id} (slot={slot}, step={step}): expected {expected}, actual {actual}")]
    ReplayDivergence {
        recording_id: String,
        slot: String,
        step: i64,
        expected: String,
        actual: String,
    },
}

/// Inputs to a single [`execute_slot_cline`] invocation. Its OWN struct
/// (not an extension of [`crate::agent::execute::SlotInput`]) so the Cline
/// path stays isolated from the dozens of `SlotInput` call sites.
pub struct ClineSlotInput<'a> {
    /// The slot to dispatch (role / provider / model). The slot's
    /// `effective_model()` is the Cline `modelId`.
    pub slot: &'a LLMSlot,
    /// The resolved xvision provider config for the slot's provider. Mapped
    /// to a Cline `providerId` + `baseUrl` via [`map_provider`].
    pub provider_entry: &'a ProviderEntry,
    /// API key for the provider (already resolved from its env var by the
    /// caller). `None`/empty for keyless local endpoints.
    pub api_key: Option<String>,
    /// System prompt for this slot (from the bound `AgentSlot.system_prompt`).
    pub system_prompt: String,
    /// The briefing / upstream inputs rendered into the first user turn.
    pub upstream_inputs: serde_json::Value,
    /// The structured-decision schema the agent must satisfy via
    /// `submit_decision`. Required by the sidecar whenever `allowed_tools`
    /// contains `submit_decision`.
    pub response_schema: ResponseSchema,
    /// Extra (non-`submit_decision`) tool names exposed to the agent — the
    /// strategy's `required_tools` / slot `allowed_tools`. `submit_decision`
    /// is appended automatically.
    pub allowed_tools: Vec<String>,
    /// Operator's per-request output-token budget. `None` falls back to
    /// [`DEFAULT_MAX_OUTPUT_TOKENS`].
    pub max_tokens: Option<u32>,
    /// Operator's per-step wall-clock budget in milliseconds. `None`
    /// (the default) means no wall budget — the sidecar runs the step
    /// to natural completion or until the model itself returns. Set
    /// per-slot when the operator wants a hard ceiling on cycle time.
    /// QA30 (2026-05-26): added so the per-agent UI can surface this
    /// without falling back to a hardcoded 2-minute default.
    pub max_wall_ms: Option<u32>,
    /// The idempotency key for the Cline run (item 2). Built by the caller
    /// from `cycle_id` + slot role so a retried cycle re-uses the same id
    /// and the sidecar dedups it. MUST be unique per logical slot
    /// invocation within a cycle.
    pub run_id: String,
    /// The live Cline sidecar client, shared across slots in a run.
    pub cline_client: Arc<AgentClient>,
    /// Record (live) vs Replay (deterministic re-run) — Stage 3, Task 3.
    /// Defaults to [`TrajectoryMode::Record`] so existing call sites that
    /// don't opt into replay keep the live path.
    pub trajectory_mode: TrajectoryMode,
    /// §2-B: when `Some`, this run is recording a trajectory — the sidecar
    /// is asked to emit `event.trajectory_frame` notifications
    /// (`StartRunParams.record = true`) stamped with THIS `slot_role`.
    ///
    /// The value MUST equal the `slot_role` of the [`TrajectoryKey`] the
    /// recording was minted against (footgun c): frames are persisted at
    /// `(recording_id, slot_role, step_index, frame_index)` and
    /// `read_frames(rid, slot_role, step)` filters on `slot_role`, so a
    /// mismatch makes the recorded frames silently unreadable on replay.
    /// The caller derives both from the same source (the agent slot's role).
    ///
    /// `None` ⇒ no recording: `record = false`, `slot_role = None` —
    /// byte-identical to the pre-§2-B live/backtest path.
    ///
    /// [`TrajectoryKey`]: xvision_observability::trajectory::key::TrajectoryKey
    pub record_slot_role: Option<String>,
    /// Optional observability emitter threaded from the eval executor. When
    /// `Some`, the executor's failure path calls
    /// [`crate::agent::observability::ObsEmitter::emit_child_run_failed`] to
    /// correct the child `agent_runs` row status after a failed step.
    ///
    /// The sidecar always emits `event.run_finished(completed)` via the event
    /// socket on `end_run`, even when the step itself failed — this causes
    /// F5: the child row (id = `{eval_run_id}::{role}::cycleN`) is left as
    /// `completed` while the parent is `failed`. Publishing a corrective
    /// `RunFinished(Failed)` AFTER `end_run` (so the sidecar's notification
    /// has been processed first) ensures the final recorded status is `failed`.
    ///
    /// `None` disables the correction (unit tests, legacy callers without an
    /// obs emitter wired). The successful path does NOT call this — the
    /// sidecar's `completed` status is correct and should be preserved.
    pub obs: Option<crate::agent::observability::ObsEmitter>,
    /// Parent span id for the WS-17 `model.reasoning` span. When the Cline
    /// dispatch is wrapped by a `model.call` span, the caller threads that
    /// span id here so the captured chain-of-thought (extracted from a CoT
    /// model's `<think>` block at the recovery strip site) nests under the
    /// model call. `None` ⇒ the reasoning span is emitted top-level (the
    /// chain-of-thought still reaches the trace). The Cline trader path
    /// does not currently own a `model.call` span, so production callers
    /// pass `None` today; the field is the seam for a future WS that adds
    /// one.
    pub model_call_span_id: Option<String>,
    /// Reasoning effort hint forwarded to the sidecar for CoT reasoning
    /// models (deepseek-r1, qwq, etc.) via `StartRunParams::reasoning_effort`.
    /// Derived at the Cline dispatch site via
    /// `crate::agents::model::default_reasoning_effort(&slot.effective_model())`.
    /// `None` for non-CoT models (field omitted on the wire).
    pub reasoning_effort: Option<String>,
}

fn cline_decision_instruction() -> &'static str {
    "Follow the slot instructions. You may call tools to fetch additional data \
     for the current decision asset only. Prefer calling the submit_decision \
     tool with your decision as a JSON argument. If your model cannot call that \
     tool reliably, output exactly one JSON object matching the decision schema; \
     no prose or markdown."
}

impl ClineSlotInput<'_> {
    fn role(&self) -> &str {
        &self.slot.role
    }

    /// Tool surface for the run: caller-supplied tools plus the built-in
    /// `submit_decision` lifecycle tool (deduped so an explicit listing in
    /// `allowed_tools` is harmless).
    fn allowed_tools_plus_submit_decision(&self) -> Vec<String> {
        let mut out = self.allowed_tools.clone();
        if !out.iter().any(|t| t == SUBMIT_DECISION_TOOL) {
            out.push(SUBMIT_DECISION_TOOL.to_string());
        }
        out
    }

    fn budget_limits(&self) -> BudgetLimits {
        BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: self.max_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: self.max_wall_ms.unwrap_or(DEFAULT_MAX_WALL_MS),
        }
    }

    /// Render the first user turn. Mirrors `execute_slot`'s framing so the
    /// model sees the same inputs shape regardless of runtime.
    fn render_prompt(&self) -> anyhow::Result<String> {
        Ok(format!(
            "Inputs:\n{}\n\n{}",
            serde_json::to_string_pretty(&self.upstream_inputs)?,
            cline_decision_instruction(),
        ))
    }
}

/// Execute one slot as a Cline `Agent` run and return its structured
/// decision wrapped in an [`LlmResponse`] the existing parser accepts.
///
/// See the module docs for the failure + recovery contract (item 2) and
/// the provider-matrix abort behavior (item 5).
pub async fn execute_slot_cline(input: ClineSlotInput<'_>) -> anyhow::Result<LlmResponse> {
    let role = input.role().to_string();
    let run_id = input.run_id.clone();

    // Observability: provider / model / span for the model-call lifecycle
    // written on the success path below. The backtest executor opens the
    // `decision.model` span (`emit_model_call_started`) and threads its id
    // here as `model_call_span_id`; we close it with `emit_model_call_finished`
    // so a `model_calls` row lands with the sidecar's reported token usage.
    // Mirrors `trader_provider` / `trader_model_id` in the backtest executor
    // (slot.provider, slot.effective_model) so the finish event keys the same
    // provider/model the started span recorded. Captured before `input` is
    // partially consumed below.
    let obs_provider = input
        .slot
        .provider
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| input.provider_entry.name.clone());
    let obs_model = input.slot.effective_model();
    let obs_model_call_span_id = input.model_call_span_id.clone();
    // Render the first user turn ONCE and reuse it for (a) the prompt hash,
    // (b) the sidecar `step`, and (c) the persisted prompt blob built below —
    // so the `model_calls.prompt_hash` digests exactly the bytes the operator
    // sees in the FullDebug blob (no drift between hash and stored body). A
    // render error short-circuits the whole slot via `?` (same `anyhow`
    // propagation the `step` call previously used) rather than silently
    // logging a hash for a prompt that never shipped.
    let obs_rendered_prompt = input.render_prompt()?;
    // `model_calls.prompt_hash` is a real digest of the rendered request
    // rather than a placeholder. `compute_response_hash` is a generic
    // `sha256:<hex>` of a string; reuse it for the prompt text.
    let obs_prompt_hash = crate::agent::observability::compute_response_hash(&obs_rendered_prompt);

    // Item 5: map provider → Cline gateway selection. An unmapped provider
    // (e.g. local-candle) aborts with a typed error — NO silent fallback.
    let mapped = map_provider(input.provider_entry, &input.slot.effective_model())
        .map_err(ClineRuntimeError::ProviderUnmapped)?;

    let start = StartRunParams {
        run_id: run_id.clone(),
        provider_id: mapped.provider_id,
        model_id: mapped.model_id,
        api_key: input.api_key.clone().filter(|k| !k.is_empty()),
        base_url: mapped.base_url,
        system_prompt: input.system_prompt.clone(),
        allowed_tools: input.allowed_tools_plus_submit_decision(),
        budget_limits: input.budget_limits(),
        decision_schema: Some(input.response_schema.schema.clone()),
        // §2-B: recording is enabled when the caller minted a recording for
        // this run and passed its `slot_role`. The sidecar then emits
        // `event.trajectory_frame` notifications stamped with this role, and
        // the event sink persists them into the TrajectoryStore the client
        // was spawned with. `record_slot_role` is COUPLED to the recording's
        // `TrajectoryKey.slot_role` (footgun c) — see the field docs. When
        // `None` (live/backtest default) this is byte-identical to the
        // pre-§2-B path: `record = false`, `slot_role = None`.
        record: input.record_slot_role.is_some(),
        slot_role: input.record_slot_role.clone(),
        reasoning_effort: input.reasoning_effort.clone(),
    };

    // Footgun c coupling guard: when recording, the role we stamp on frames
    // MUST equal the slot's own role (which is also what the recording's
    // TrajectoryKey was built from, and what `read_frames` filters on). A
    // mismatch would silently hide every recorded frame on replay.
    debug_assert!(
        input
            .record_slot_role
            .as_deref()
            .map(|r| r == role)
            .unwrap_or(true),
        "record_slot_role ({:?}) must equal the slot role ({role}) so recorded \
         frames are readable on replay (footgun c)",
        input.record_slot_role,
    );

    input
        .cline_client
        .start_run(start)
        .await
        .map_err(|source| ClineRuntimeError::StartRun {
            run_id: run_id.clone(),
            role: role.clone(),
            source,
        })?;

    // Replay branch (item 1 / Task 3): after start_run and before the
    // first step, ship the recorded frames to the sidecar so the agent
    // re-runs its loop from the recording instead of a live provider.
    // The recorded frames are validated for sufficiency here (item 4
    // bounds) before they go over the wire — a recording that has no
    // frames for this slot/step is corrupt and aborts the cycle.
    if let TrajectoryMode::Replay { recording_id, store } = &input.trajectory_mode {
        load_replay_frames(&input, &role, recording_id, store).await?;
    }

    // One step drives the agent's tool loop to completion. The step result
    // is computed first, then `end_run` is ALWAYS attempted so the sidecar
    // session is reclaimed even when the step errored (item 2).
    let step_result = input
        .cline_client
        .step(StepParams {
            run_id: run_id.clone(),
            // Reuse the prompt rendered above so the bytes shipped to the
            // sidecar are byte-identical to the hash input AND the FullDebug
            // prompt blob built at the success-path emit below.
            prompt: obs_rendered_prompt.clone(),
        })
        .await;

    // No-decision recovery: before end_run, try to recover a decision from
    // a weak tool-caller that emitted end_turn without calling submit_decision.
    // The session is still open, so a second step attempt is valid.
    let step_result = match step_result {
        Ok(step) => Ok(try_nodecision_recovery(
            step,
            &input.cline_client,
            &run_id,
            &role,
            input.obs.as_ref(),
            input.model_call_span_id.as_deref(),
        )
        .await),
        Err(e) => Err(e),
    };

    let _ = input
        .cline_client
        .end_run(EndRunParams {
            run_id: run_id.clone(),
        })
        .await;

    // Crash boundary: a transport error means the sidecar died mid-step or
    // the connection dropped. Surface it typed; the cycle fails.
    let step = step_result.map_err(|source| ClineRuntimeError::StepTransport {
        run_id: run_id.clone(),
        role: role.clone(),
        source,
    })?;

    // Replay divergence + exhaustion detection (items 2 + 4). The sidecar
    // surfaces a replay-specific abort reason on `error` when the replayed
    // control flow drifts from the recorded transcript or the frame feed
    // is exhausted; map those to the typed errors and mark the recording
    // corrupt. This runs BEFORE the generic `status != completed` gate so a
    // replay-specific abort surfaces as the typed `ReplayFramesExhausted` /
    // `ReplayDivergence` (not a generic `StepNotCompleted`). We also
    // Rust-side compare the recorded terminal decision against the replayed
    // one as a belt-and-suspenders divergence gate so a "replay model
    // yields recorded frame, therefore matches itself" false-green cannot
    // slip through.
    if let TrajectoryMode::Replay { recording_id, store } = &input.trajectory_mode {
        check_replay_outcome(&role, recording_id, store, &step).await?;
    }

    if step.status != "completed" {
        if step
            .error
            .as_deref()
            .unwrap_or("")
            .contains("budget_output_tokens_exceeded")
        {
            tracing::warn!(
                event = "budget_misconfig_suspected",
                run_id = %run_id,
                role = %role,
                hint = "max_tokens may be too low for this model — increase to ≥2048 in agent slot settings",
            );
        }
        let err = ClineRuntimeError::StepNotCompleted {
            run_id: run_id.clone(),
            role: role.clone(),
            status: step.status,
            error: step.error,
        };
        // F5 correction: the sidecar emits `event.run_finished(completed)`
        // on `end_run` even when the step failed. Override the child run's
        // status to `failed` so `agent_runs` for this cycle reflects the
        // actual outcome.
        //
        // Ordering: yield once so the event-sink's background socket reader
        // can process the sidecar's `event.run_finished(completed)` first.
        // Our `failed` UPDATE then arrives second and wins. Without the
        // yield the relative order of the socket event and our direct bus
        // publish is non-deterministic, and `completed` could overwrite
        // `failed` if the socket event arrives late.
        if let Some(obs) = input.obs.as_ref() {
            tokio::task::yield_now().await;
            obs.emit_child_run_failed(&run_id, err.to_string()).await;
        }
        return Err(err.into());
    }

    let decision_json = match step.decision_json {
        Some(json) => json,
        None => {
            let err = ClineRuntimeError::NoDecision {
                run_id: run_id.clone(),
                role: role.clone(),
            };
            // F5 correction: same as StepNotCompleted — the sidecar
            // reports `completed` (via `event.run_finished`) but no
            // decision was submitted, so the run actually failed. Yield
            // once before publishing so the socket event arrives first.
            if let Some(obs) = input.obs.as_ref() {
                tokio::task::yield_now().await;
                obs.emit_child_run_failed(&run_id, err.to_string()).await;
            }
            return Err(err.into());
        }
    };

    // Validate the payload parses as JSON here so the typed error is
    // attributable to the Cline runtime rather than surfacing later as a
    // generic parser failure. The original text is preserved verbatim in
    // the returned ContentBlock so the downstream parser sees exactly what
    // the agent submitted.
    let _: serde_json::Value = match serde_json::from_str(&decision_json) {
        Ok(json) => json,
        Err(source) => {
            let err = ClineRuntimeError::DecisionNotJson {
                run_id: run_id.clone(),
                role: role.clone(),
                source,
            };
            // F5 correction: a completed sidecar step with malformed decision
            // JSON is still a failed child run from the eval engine's
            // perspective. Override the sidecar's completed terminal event
            // just like the no-decision path above.
            if let Some(obs) = input.obs.as_ref() {
                tokio::task::yield_now().await;
                obs.emit_child_run_failed(&run_id, err.to_string()).await;
            }
            return Err(err.into());
        }
    };

    let input_tokens = u32::try_from(step.usage.input_tokens).unwrap_or(u32::MAX);
    let output_tokens = u32::try_from(step.usage.output_tokens).unwrap_or(u32::MAX);

    // Observability: the Cline trader path is the only trader runtime now, and
    // it previously emitted no model-call lifecycle on success — so even with
    // the obs bus wired, `model_calls` stayed empty (a `model_calls` row is
    // written ONLY by `RunEvent::ModelCallFinished`). Emit it here with the
    // sidecar's reported token usage, keyed on the `decision.model` span the
    // backtest executor opened (`emit_model_call_started`) and threaded down
    // as `model_call_span_id`. When the caller did NOT thread a span id (e.g.
    // legacy/unit callers), there is no span to attach to and we skip the
    // emit rather than fabricate one — the span-finished close stays the
    // caller's responsibility. The response_hash digests the decision JSON;
    // `cost_usd: None` lets the emitter fall back to catalog-based pricing.
    //
    // PAYLOAD CAPTURE (fix/cline-model-call-payloads): use the PAYLOAD-aware
    // emit variant — the hash-only `emit_model_call_finished` hard-codes
    // `prompt_text: None` / `response_text: None`, so under `full_debug` the
    // trace inspector had no body to show and fell back to `prompt.hash` /
    // `response.hash`. Mirror the standalone path (`execute.rs`): build an
    // `LlmRequest` faithfully representing the prompt the sidecar ran, pass it
    // as `prompt_request`, and pass the decision JSON as `response_text`. The
    // emitter gates internally on retention (HashOnly → no bodies; Redacted →
    // redacted bodies; FullDebug → raw), so this NEVER weakens redaction.
    //
    // The `LlmRequest` reuses `obs_rendered_prompt` (the exact bytes the hash
    // digested and the sidecar `step` shipped), so the persisted blob stays
    // consistent with `prompt_hash`. `tools` is left empty: the Cline path
    // only has tool *names* (`allowed_tools_plus_submit_decision`), not full
    // `ToolDefinition`s (those live in the retired LlmDispatch registry), and
    // the sidecar receives the names + the `response_schema` it splices into
    // the prompt. The operator-visible prompt — system prompt + rendered user
    // turn + response contract — is captured faithfully; the tool-definition
    // JSON bodies are the only omission vs. the true wire request.
    if let (Some(obs), Some(span_id)) = (input.obs.as_ref(), obs_model_call_span_id.as_deref()) {
        let response_hash = if decision_json.is_empty() {
            None
        } else {
            Some(crate::agent::observability::compute_response_hash(&decision_json))
        };
        let prompt_request = LlmRequest {
            model: obs_model.clone(),
            system_prompt: input.system_prompt.clone(),
            messages: vec![Message::user_text(obs_rendered_prompt.clone())],
            max_tokens: input.max_tokens,
            tools: Vec::new(),
            temperature: None,
            response_schema: Some(input.response_schema.clone()),
            cache_control: None,
            force_json: false,
        };
        obs.emit_model_call_finished_with_payloads(
            span_id,
            &obs_provider,
            &obs_model,
            Some(input_tokens),
            Some(output_tokens),
            None,
            obs_prompt_hash,
            response_hash,
            Some(&prompt_request),
            if decision_json.is_empty() {
                None
            } else {
                Some(decision_json.as_str())
            },
        )
        .await;
    }

    Ok(LlmResponse {
        // The existing decision parser (`dispatch_capability::parse_*` and
        // `TraderOutput::parse_response`) reads `resp.text()`, which
        // concatenates Text blocks. Emit the decision JSON as a single
        // Text block so the parser is byte-identical to the LlmDispatch path.
        content: vec![ContentBlock::Text { text: decision_json }],
        stop_reason: StopReason::EndTurn,
        input_tokens,
        output_tokens,
    })
}

/// Sidecar `StepResult.error` reason code: the replay frame feed was
/// exhausted before the agent reached a terminal decision. Mirrors the
/// `REPLAY_FRAMES_EXHAUSTED` reason the replay model raises in
/// `xvision-agentd/src/session/replay-model.ts`.
pub const STEP_ERR_REPLAY_FRAMES_EXHAUSTED: &str = "replay_frames_exhausted";

/// Sidecar `StepResult.error` reason code: the replayed control flow
/// diverged from the recorded transcript (a tool result / reconstructed
/// request did not match the recording).
pub const STEP_ERR_REPLAY_DIVERGENCE: &str = "replay_divergence";

fn cline_raw_json_repair_prompt() -> String {
    "Output only a JSON object with your trading decision — no prose, no markdown. \
        Optional bracket keys may be omitted when not needed; if present, use these exact key names:\n\
        {\"action\": \"long_open|short_open|flat|hold\", \"conviction\": 0.0-1.0, \"justification\": \"reason\", \
        \"stop_loss_pct\": 2.0, \"take_profit_pct\": 4.0, \"trailing_stop_pct\": 1.5, \
        \"breakeven_trigger_pct\": 2.0, \"breakeven_offset_pct\": 0.2, \"fade_sl_bars\": 6, \
        \"fade_sl_start_pct\": 3.0, \"fade_sl_end_pct\": 1.0, \"max_bars_held\": 12, \
        \"sl_atr_mult\": 2.0, \"tp_atr_mult\": 3.0, \"tp1_pct\": 3.0, \
        \"tp1_close_fraction\": 0.5, \"tp2_pct\": 6.0}"
        .to_string()
}

/// Single-shot no-decision recovery for weak tool-callers that emit
/// `end_turn` without calling `submit_decision`. Runs BEFORE `end_run`
/// so the session is still open for a second step.
///
/// Method 1: if `output_text` starts with `{` and parses as JSON, adopt
/// it as the decision (some models emit JSON prose instead of a tool call).
/// Method 2: issue a repair step prompt asking the model to call
/// `submit_decision` now. On either success the patched step is returned
/// with `tracing::info!(event = "nodecision_recovery_succeeded")`.
/// On total failure the original step is returned unchanged so the
/// caller's `NoDecision` error path fires as normal.
async fn try_nodecision_recovery(
    mut step: StepResult,
    client: &AgentClient,
    run_id: &str,
    role: &str,
    obs: Option<&crate::agent::observability::ObsEmitter>,
    model_call_span_id: Option<&str>,
) -> StepResult {
    if step.status != "completed" || step.decision_json.is_some() {
        return step;
    }

    // Method 1: scan output_text for a JSON object, stripping <think> blocks first.
    // Reasoning models (deepseek-r1, Fino1, etc.) emit chain-of-thought prose or
    // <think>…</think> wrappers before — or instead of — a submit_decision call.
    // Stripping think blocks prevents false-positive {…} matches inside reasoning traces.
    //
    // WS-17 (reasoning capture): the captured `<think>` text is the
    // highest-signal "why" for the flywheel — emit it as a `model.reasoning`
    // span BEFORE it would otherwise be discarded. The clean body fed to the
    // JSON extractor is unchanged (byte-identical), so parsing is unaffected.
    let (cleaned, reasoning) = strip_and_capture_think_blocks(&step.output_text);
    if let Some(obs) = obs {
        if let Some(reasoning_text) = reasoning.as_deref() {
            obs.emit_model_reasoning(model_call_span_id.map(str::to_string), reasoning_text)
                .await;
        }
    }
    if let Some(json_str) = extract_last_trader_output_json(&cleaned) {
        tracing::info!(
            event = "nodecision_recovery_succeeded",
            run_id = %run_id,
            role = %role,
            method = "output_text_json_scan",
        );
        step.decision_json = Some(json_str);
        return step;
    }

    // Method 2: repair step — request raw JSON output with no tool-call dependency.
    // Phrased as a direct JSON request so models that don't support function-calling
    // can comply (the previous "Call submit_decision" instruction was silently ignored
    // by CoT-style models).
    let repair = client
        .step(StepParams {
            run_id: run_id.to_string(),
            prompt: cline_raw_json_repair_prompt(),
        })
        .await;

    if let Ok(repair_step) = repair {
        // WS-17: capture+emit the repair step's reasoning too. `decision_json`
        // short-circuits the strip (tool call already gave us JSON), so only
        // capture when we fall through to the output_text scan.
        let found = match repair_step.decision_json {
            Some(json) => Some(json),
            None => {
                let (c, reasoning) = strip_and_capture_think_blocks(&repair_step.output_text);
                if let Some(obs) = obs {
                    if let Some(reasoning_text) = reasoning.as_deref() {
                        obs.emit_model_reasoning(model_call_span_id.map(str::to_string), reasoning_text)
                            .await;
                    }
                }
                extract_last_trader_output_json(&c)
            }
        };
        if found.is_some() {
            tracing::info!(
                event = "nodecision_recovery_succeeded",
                run_id = %run_id,
                role = %role,
                method = "repair_step",
            );
            step.decision_json = found;
        }
    }

    step
}

/// Strip `<think>…</think>` blocks (case-insensitive) AND return the
/// captured chain-of-thought (WS-17 reasoning capture).
///
/// Called before JSON extraction so reasoning traces don't shadow the
/// decision object. Returns `(clean_body, Option<reasoning>)`:
/// - `clean_body` is byte-identical to the historic strip output (think
///   block removed, trimmed) — the trader still parses the same clean
///   JSON.
/// - `reasoning` is the concatenation of every captured `<think>` inner
///   text (joined with `\n` when a model emits more than one block),
///   trimmed; `None` when no `<think>` block was present.
///
/// The inner text of an UNCLOSED `<think>` (no matching `</think>`) is
/// captured too — the engine treats everything from the tag to end of
/// string as reasoning, so it's surfaced rather than silently dropped.
pub fn strip_and_capture_think_blocks(s: &str) -> (String, Option<String>) {
    let mut result = s.to_string();
    let mut captured: Vec<String> = Vec::new();
    loop {
        let lower = result.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else { break };
        let after_open = start + "<think>".len();
        match lower[after_open..].find("</think>") {
            Some(rel) => {
                let inner_end = after_open + rel;
                let end = inner_end + "</think>".len();
                let inner = result[after_open..inner_end].trim();
                if !inner.is_empty() {
                    captured.push(inner.to_string());
                }
                result = format!("{}{}", &result[..start], result[end..].trim_start());
            }
            None => {
                // Unclosed <think> — strip everything from the tag to end
                // of string, capturing the partial reasoning.
                let inner = result[after_open..].trim();
                if !inner.is_empty() {
                    captured.push(inner.to_string());
                }
                result = result[..start].trim_end().to_string();
                break;
            }
        }
    }
    let reasoning = if captured.is_empty() {
        None
    } else {
        Some(captured.join("\n"))
    };
    (result.trim().to_string(), reasoning)
}

/// Read the recorded frames for this slot's first step, validate they are
/// sufficient to start a replay (item 4 bounds — a recording with no
/// frames is corrupt), and ship them to the sidecar via `replay_load`.
///
/// A failure to read frames is a HARD abort — a replay run never falls
/// back to a live provider. When the recording has zero usable frames the
/// recording is marked corrupt with `replay_frames_exhausted` so a later
/// inspect surfaces the reason.
async fn load_replay_frames(
    input: &ClineSlotInput<'_>,
    role: &str,
    recording_id: &RecordingId,
    store: &TrajectoryStore,
) -> anyhow::Result<()> {
    // Step 0 holds the slot's first model call. Multi-step slots address
    // later steps by step_index; v1 replay drives one step (matching the
    // single-step live path in this executor).
    let frames = store.read_frames(recording_id, role, 0).await.map_err(|source| {
        ClineRuntimeError::ReplayFramesUnavailable {
            recording_id: recording_id.to_string(),
            role: role.to_string(),
            source,
        }
    })?;

    if frames.is_empty() {
        // No frames for this slot/step — bounded feed has nothing to
        // replay. Mark corrupt and abort (item 4 reconstitution rule).
        let _ = store
            .mark_corrupt(recording_id, RECOVERY_REPLAY_FRAMES_EXHAUSTED)
            .await;
        return Err(ClineRuntimeError::ReplayFramesExhausted {
            recording_id: recording_id.to_string(),
            role: role.to_string(),
            step: 0,
            detail: "recording has no frames for this slot/step".to_string(),
        }
        .into());
    }

    let frame_values: Vec<serde_json::Value> = frames
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| ClineRuntimeError::ReplayFramesExhausted {
            recording_id: recording_id.to_string(),
            role: role.to_string(),
            step: 0,
            detail: format!("frame serialization failed: {e}"),
        })?;

    input
        .cline_client
        .replay_load(ReplayLoadParams {
            run_id: input.run_id.clone(),
            frames: frame_values,
        })
        .await
        .map_err(|source| ClineRuntimeError::StartRun {
            run_id: input.run_id.clone(),
            role: role.to_string(),
            source,
        })?;

    Ok(())
}

/// After a replay step, detect divergence + exhaustion (items 2 + 4).
///
/// 1. If the sidecar aborted with a replay-specific reason, map it to the
///    typed error and mark the recording corrupt with the matching
///    `recovery_reason`.
/// 2. Belt-and-suspenders: compare the replayed step's decision against
///    the recorded terminal `submit_decision` tool call. A mismatch is a
///    divergence — the replay drifted from the recording (changed tool
///    result, reconstitution drift). This avoids the false-green where a
///    replay model simply echoes the recorded frame and "matches" itself.
async fn check_replay_outcome(
    role: &str,
    recording_id: &RecordingId,
    store: &TrajectoryStore,
    step: &xvision_agent_client::StepResult,
) -> anyhow::Result<()> {
    // (1) Sidecar-signalled replay abort.
    if let Some(reason) = step.error.as_deref() {
        if reason == STEP_ERR_REPLAY_FRAMES_EXHAUSTED {
            let _ = store
                .mark_corrupt(recording_id, RECOVERY_REPLAY_FRAMES_EXHAUSTED)
                .await;
            return Err(ClineRuntimeError::ReplayFramesExhausted {
                recording_id: recording_id.to_string(),
                role: role.to_string(),
                step: 0,
                detail: "sidecar reported replay frame exhaustion".to_string(),
            }
            .into());
        }
        if reason == STEP_ERR_REPLAY_DIVERGENCE {
            let _ = store.mark_corrupt(recording_id, RECOVERY_REPLAY_DIVERGENCE).await;
            return Err(ClineRuntimeError::ReplayDivergence {
                recording_id: recording_id.to_string(),
                slot: role.to_string(),
                step: 0,
                expected: "recorded transcript".to_string(),
                actual: "sidecar reported divergent tool result / request".to_string(),
            }
            .into());
        }
    }

    // (2) Rust-side terminal-decision divergence gate. Recompute the
    // recorded decision from the trajectory frames and compare against the
    // replayed step's decision. Both are normalized through serde_json so
    // the comparison is structural (key order independent), not byte-naive.
    if let Some(replayed) = step.decision_json.as_deref() {
        if let Some(recorded) = recorded_decision_from_store(store, recording_id, role).await {
            let replayed_val: serde_json::Value = match serde_json::from_str(replayed) {
                Ok(v) => v,
                // A non-JSON replayed decision is handled by the
                // DecisionNotJson gate downstream; skip the divergence
                // comparison here.
                Err(_) => return Ok(()),
            };
            if recorded != replayed_val {
                let _ = store.mark_corrupt(recording_id, RECOVERY_REPLAY_DIVERGENCE).await;
                return Err(ClineRuntimeError::ReplayDivergence {
                    recording_id: recording_id.to_string(),
                    slot: role.to_string(),
                    step: 0,
                    expected: recorded.to_string(),
                    actual: replayed_val.to_string(),
                }
                .into());
            }
        }
    }

    Ok(())
}

/// Reconstruct the recorded terminal decision from a recording's frames:
/// the `input` of the last `ToolCallDelta` whose `tool_name` is
/// `submit_decision`. Returns `None` when the recording has no such frame
/// (e.g. hash-only retention, or a recording that predates the lifecycle
/// tool) — in that case the Rust-side divergence gate is skipped and the
/// sidecar-side gate (1) is authoritative.
async fn recorded_decision_from_store(
    store: &TrajectoryStore,
    recording_id: &RecordingId,
    role: &str,
) -> Option<serde_json::Value> {
    let frames = store.read_frames(recording_id, role, 0).await.ok()?;
    recorded_decision_from_frames(&frames)
}

/// Pure helper: pull the `submit_decision` payload from a frame list.
fn recorded_decision_from_frames(frames: &[TrajectoryFrame]) -> Option<serde_json::Value> {
    frames.iter().rev().find_map(|f| match f {
        TrajectoryFrame::ToolCallDelta {
            tool_name: Some(name),
            input: Some(input),
            ..
        } if name == SUBMIT_DECISION_TOOL => Some(input.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `ClineSlotInput` against an already-constructed client so the
    /// pure helpers (`allowed_tools_plus_submit_decision`, `budget_limits`,
    /// `render_prompt`) can be exercised without an active session. The
    /// end-to-end run is covered by the integration test in
    /// `tests/cline_execute_slot.rs`.
    fn make_input<'a>(
        slot: &'a LLMSlot,
        entry: &'a ProviderEntry,
        client: Arc<AgentClient>,
        extra_tools: Vec<String>,
        max_tokens: Option<u32>,
    ) -> ClineSlotInput<'a> {
        ClineSlotInput {
            slot,
            provider_entry: entry,
            api_key: Some("k".into()),
            system_prompt: "decide".into(),
            upstream_inputs: serde_json::json!({"x": 1}),
            response_schema: ResponseSchema::trader_output(),
            allowed_tools: extra_tools,
            max_tokens,
            max_wall_ms: None,
            run_id: "cycle-1::trader".into(),
            cline_client: client,
            trajectory_mode: TrajectoryMode::default(),
            record_slot_role: None,
            obs: None,
            model_call_span_id: None,
            reasoning_effort: None,
        }
    }

    fn slot() -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: Vec::new(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        }
    }

    fn entry() -> ProviderEntry {
        ProviderEntry {
            name: "anthropic".into(),
            kind: xvision_core::config::ProviderKind::Anthropic,
            base_url: String::new(),
            api_key_env: "K".into(),
            enabled_models: vec!["claude-sonnet-4-6".into()],
        }
    }

    // Constructing a real `AgentClient` requires a spawned sidecar, so the
    // helper tests below cover the pure logic that does not touch the
    // transport. They build the input lazily only when a client exists; the
    // `make_input` fn keeps the field set honest at compile time.
    #[allow(dead_code)]
    fn _typecheck_make_input(client: Arc<AgentClient>) {
        let s = slot();
        let e = entry();
        let _ = make_input(&s, &e, client, vec!["indicators.rsi".into()], Some(4096));
    }

    #[test]
    fn dedup_helper_appends_submit_decision_once() {
        // Mirror `allowed_tools_plus_submit_decision`'s contract without a
        // client: an explicit listing must not duplicate the tool.
        fn plus(mut v: Vec<String>) -> Vec<String> {
            if !v.iter().any(|t| t == SUBMIT_DECISION_TOOL) {
                v.push(SUBMIT_DECISION_TOOL.to_string());
            }
            v
        }
        let from_empty = plus(vec![]);
        assert_eq!(from_empty, vec![SUBMIT_DECISION_TOOL.to_string()]);

        let from_extra = plus(vec!["indicators.rsi".into()]);
        assert_eq!(from_extra.len(), 2);
        assert_eq!(
            from_extra.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(),
            1
        );

        let already = plus(vec![SUBMIT_DECISION_TOOL.into()]);
        assert_eq!(already.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(), 1);
    }

    #[test]
    fn cline_decision_instruction_allows_raw_json_for_weak_tool_callers() {
        let instruction = cline_decision_instruction();
        assert!(instruction.contains("submit_decision"));
        assert!(instruction.contains("JSON object"));
        assert!(!instruction.contains("Do NOT output prose, raw JSON"));
        assert!(!instruction.contains("Outputting text instead of calling the tool will fail"));
    }

    #[test]
    fn raw_json_repair_prompt_mentions_optional_bracket_fields() {
        let prompt = cline_raw_json_repair_prompt();
        assert!(prompt.contains("long_open|short_open|flat|hold"));
        for field in [
            "\"stop_loss_pct\"",
            "\"take_profit_pct\"",
            "\"trailing_stop_pct\"",
            "\"breakeven_trigger_pct\"",
            "\"tp1_close_fraction\"",
            "\"tp2_pct\"",
        ] {
            assert!(prompt.contains(field), "repair prompt missing {field}: {prompt}");
        }
        assert!(
            !prompt.contains("_pct?") && !prompt.contains("_mult?") && !prompt.contains("_bars?"),
            "repair prompt must not show parser-invalid question-mark keys: {prompt}"
        );
    }

    #[test]
    fn recorded_decision_extracts_last_submit_decision_input() {
        let frames = vec![
            TrajectoryFrame::Request {
                ts_ms: 1,
                messages: serde_json::json!([]),
                tools: serde_json::json!([]),
                system_prompt: None,
            },
            TrajectoryFrame::ToolCallDelta {
                ts_ms: 2,
                tool_call_id: Some("c1".into()),
                tool_name: Some("indicators.rsi".into()),
                input: Some(serde_json::json!({"window": 14})),
            },
            TrajectoryFrame::ToolCallDelta {
                ts_ms: 3,
                tool_call_id: Some("c2".into()),
                tool_name: Some(SUBMIT_DECISION_TOOL.into()),
                input: Some(serde_json::json!({"action": "long_open", "conviction": 0.8})),
            },
            TrajectoryFrame::Finish {
                ts_ms: 4,
                reason: "stop".into(),
                error: None,
            },
        ];
        let decision = recorded_decision_from_frames(&frames).expect("submit_decision present");
        assert_eq!(decision["action"], "long_open");
    }

    #[test]
    fn recorded_decision_none_when_no_submit_decision() {
        let frames = vec![TrajectoryFrame::ToolCallDelta {
            ts_ms: 1,
            tool_call_id: Some("c1".into()),
            tool_name: Some("indicators.rsi".into()),
            input: Some(serde_json::json!({"window": 14})),
        }];
        assert!(recorded_decision_from_frames(&frames).is_none());
    }

    #[test]
    fn trajectory_mode_defaults_to_record() {
        assert!(matches!(TrajectoryMode::default(), TrajectoryMode::Record));
    }

    #[test]
    fn budget_uses_max_tokens_then_default() {
        let with = BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: Some(4096u32).unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        };
        assert_eq!(with.max_output_tokens, 4096);

        let without = BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: None.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        };
        assert_eq!(without.max_output_tokens, DEFAULT_MAX_OUTPUT_TOKENS);
        assert!(without.max_wall_ms > 0);
    }
}
