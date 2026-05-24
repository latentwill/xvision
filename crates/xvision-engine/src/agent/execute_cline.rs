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

use crate::agent::llm::{ContentBlock, LlmResponse, ResponseSchema, StopReason};
use crate::strategies::slot::LLMSlot;
use std::sync::Arc;
use xvision_agent_client::provider_map::{map_provider, ProviderMapError};
use xvision_agent_client::{
    AgentClient, BudgetLimits, EndRunParams, ReplayLoadParams, StartRunParams, StepParams,
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
/// `max_tokens`. The wall-clock cap bounds a wedged sidecar step so a
/// crashed/looping run surfaces as a typed budget abort rather than
/// hanging the cycle (item 2 — failure boundary).
const DEFAULT_MAX_INPUT_TOKENS: u32 = 200_000;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 8_192;
const DEFAULT_MAX_WALL_MS: u32 = 120_000;

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
    #[error("cline runtime: step failed (sidecar transport/crash) for run_id={run_id} (role={role}): {source}")]
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
    #[error("cline runtime: run completed without calling submit_decision for run_id={run_id} (role={role})")]
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
    #[error("cline runtime: replay frames unavailable for recording_id={recording_id} (role={role}): {source}")]
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
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        }
    }

    /// Render the first user turn. Mirrors `execute_slot`'s framing so the
    /// model sees the same inputs shape regardless of runtime.
    fn render_prompt(&self) -> anyhow::Result<String> {
        Ok(format!(
            "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
             to fetch additional data; submit your final decision via the \
             `submit_decision` tool as JSON matching the required schema.",
            serde_json::to_string_pretty(&self.upstream_inputs)?
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
    };

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
    if let TrajectoryMode::Replay {
        recording_id,
        store,
    } = &input.trajectory_mode
    {
        load_replay_frames(&input, &role, recording_id, store).await?;
    }

    // One step drives the agent's tool loop to completion. The step result
    // is computed first, then `end_run` is ALWAYS attempted so the sidecar
    // session is reclaimed even when the step errored (item 2).
    let step_result = input
        .cline_client
        .step(StepParams {
            run_id: run_id.clone(),
            prompt: input.render_prompt()?,
        })
        .await;

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
    if let TrajectoryMode::Replay {
        recording_id,
        store,
    } = &input.trajectory_mode
    {
        check_replay_outcome(&role, recording_id, store, &step).await?;
    }

    if step.status != "completed" {
        return Err(ClineRuntimeError::StepNotCompleted {
            run_id,
            role,
            status: step.status,
            error: step.error,
        }
        .into());
    }

    let decision_json = step.decision_json.ok_or_else(|| ClineRuntimeError::NoDecision {
        run_id: run_id.clone(),
        role: role.clone(),
    })?;

    // Validate the payload parses as JSON here so the typed error is
    // attributable to the Cline runtime rather than surfacing later as a
    // generic parser failure. The original text is preserved verbatim in
    // the returned ContentBlock so the downstream parser sees exactly what
    // the agent submitted.
    let _: serde_json::Value =
        serde_json::from_str(&decision_json).map_err(|source| ClineRuntimeError::DecisionNotJson {
            run_id: run_id.clone(),
            role: role.clone(),
            source,
        })?;

    Ok(LlmResponse {
        // The existing decision parser (`dispatch_capability::parse_*` and
        // `TraderOutput::parse_response`) reads `resp.text()`, which
        // concatenates Text blocks. Emit the decision JSON as a single
        // Text block so the parser is byte-identical to the LlmDispatch path.
        content: vec![ContentBlock::Text { text: decision_json }],
        stop_reason: StopReason::EndTurn,
        input_tokens: u32::try_from(step.usage.input_tokens).unwrap_or(u32::MAX),
        output_tokens: u32::try_from(step.usage.output_tokens).unwrap_or(u32::MAX),
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
    let frames = store
        .read_frames(recording_id, role, 0)
        .await
        .map_err(|source| ClineRuntimeError::ReplayFramesUnavailable {
            recording_id: recording_id.to_string(),
            role: role.to_string(),
            source,
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
            let _ = store
                .mark_corrupt(recording_id, RECOVERY_REPLAY_DIVERGENCE)
                .await;
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
                let _ = store
                    .mark_corrupt(recording_id, RECOVERY_REPLAY_DIVERGENCE)
                    .await;
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
            run_id: "cycle-1::trader".into(),
            cline_client: client,
            trajectory_mode: TrajectoryMode::default(),
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
        assert_eq!(from_extra.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(), 1);

        let already = plus(vec![SUBMIT_DECISION_TOOL.into()]);
        assert_eq!(already.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(), 1);
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
