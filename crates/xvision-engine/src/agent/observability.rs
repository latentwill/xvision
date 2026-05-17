//! Engine-side bridge to `xvision_observability::RunEventBus`. Closes
//! the `qa-eval-observability-wiring` gap: eval LLM calls now emit
//! `SpanStarted` / `SpanFinished` (+ `ModelCallFinished`) so failures
//! surface in `/api/agent-runs/<eval_run_id>` and render in the trace
//! dock's `SpanInspector`.
//!
//! Design: thin wrapper, not a re-export. Engine code calls
//! `ObsEmitter::emit_*` methods; under the hood each method assembles
//! the relevant `RunEvent` variant and publishes to the wrapped bus.
//! When the emitter is `None` (CLI, unit tests, every call site that
//! doesn't opt in), every method is a no-op — keeping the engine free
//! to construct emitters by default and letting only the dashboard
//! production path inject a real bus.
//!
//! The wrapped bus is the same `xvision_observability::RunEventBus`
//! the sidecar/agent-run path already uses; this module does not
//! introduce a parallel bus.

use std::sync::Arc;

use chrono::Utc;

use xvision_observability::{
    AssistantTextDeltaEvent, ModelCallFinishedEvent, RunEvent, RunEventBus, RunFinishedEvent,
    RunStartedEvent, RunStatus, SpanFinishedEvent, SpanKind, SpanStartedEvent, SpanStatus,
};

/// Engine-side helper that emits observability events around LLM
/// dispatches and tool invocations. `None` means observability is
/// disabled for this call site (default for unit tests and the CLI).
///
/// The type is `Clone` because it's threaded into multiple async
/// tasks (`SlotInput`, pipeline iteration); cloning is `Arc`-cheap.
#[derive(Clone)]
pub struct ObsEmitter {
    bus: Arc<RunEventBus>,
    run_id: String,
}

impl ObsEmitter {
    /// Construct an emitter bound to a specific run id. The eval
    /// executor calls this once per run and clones into per-slot
    /// `SlotInput`s.
    pub fn new(bus: Arc<RunEventBus>, run_id: impl Into<String>) -> Self {
        Self {
            bus,
            run_id: run_id.into(),
        }
    }

    /// Run id the emitter is bound to.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Inject this run into `agent_runs` so the spans we're about to
    /// emit have a valid FK target. Idempotent at the SQL layer via
    /// the recorder's `INSERT` — re-running yields an error the
    /// recorder logs but the publish itself never panics.
    pub async fn emit_run_started(
        &self,
        objective: impl Into<String>,
        retention_mode: impl Into<String>,
    ) {
        self.bus
            .publish(RunEvent::RunStarted(RunStartedEvent {
                run_id: self.run_id.clone(),
                objective: objective.into(),
                strategy_id: None,
                eval_run_id: Some(self.run_id.clone()),
                source_cli_job_id: None,
                started_at: Utc::now(),
                retention_mode: retention_mode.into(),
                sidecar_version: None,
                cline_sdk_version: None,
                protocol_version: None,
                skills_json: None,
                mcp_servers_json: None,
            }))
            .await;
    }

    /// Mark the run terminal. `status` should be `Completed` /
    /// `Failed` / `Cancelled` to match the recorder's
    /// `agent_runs.status` text vocabulary.
    pub async fn emit_run_finished(&self, status: RunStatus, error: Option<String>) {
        self.bus
            .publish(RunEvent::RunFinished(RunFinishedEvent {
                run_id: self.run_id.clone(),
                finished_at: Utc::now(),
                status,
                final_artifact_id: None,
                error,
            }))
            .await;
    }

    /// Open a `ModelCall` span. Caller pairs this with exactly one
    /// `emit_span_finished_*` call carrying the same `span_id`.
    pub async fn emit_model_call_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        provider: &str,
        model: &str,
    ) {
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::ModelCall,
                name: format!("{provider}/{model}"),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: None,
            }))
            .await;
    }

    /// Close a span as `Ok`. Companion `emit_model_call_finished`
    /// must be called separately for `ModelCall` spans so the
    /// `model_calls` join row gets written.
    pub async fn emit_span_finished_ok(&self, span_id: &str) {
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: SpanStatus::Ok,
                error_json: None,
            }))
            .await;
    }

    /// Close a span as `Error`. `error_message` is wrapped into the
    /// recorder's standard `{"message": "..."}` shape that
    /// `SpanInspector.parseErrorJson` (PR #238) already understands.
    pub async fn emit_span_finished_error(&self, span_id: &str, error_message: &str) {
        let error_json = serde_json::json!({ "message": error_message }).to_string();
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: SpanStatus::Error,
                error_json: Some(error_json),
            }))
            .await;
    }

    /// Side-detail for a model-call span. Pair with
    /// `emit_model_call_started` (same `span_id`). Tokens / cost
    /// values are `None` when the provider didn't report them.
    pub async fn emit_model_call_finished(
        &self,
        span_id: &str,
        provider: &str,
        model: &str,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        cost_usd: Option<f64>,
    ) {
        let prompt_hash = format!("eval:{run}:{span}", run = self.run_id, span = span_id);
        self.bus
            .publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
                span_id: span_id.to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                input_token_count: input_tokens.map(i64::from),
                output_token_count: output_tokens.map(i64::from),
                cost_usd,
                prompt_hash,
                response_hash: None,
                prompt_payload_ref: None,
                response_payload_ref: None,
                tool_calls_requested: None,
                capability_path: None,
            }))
            .await;
    }

    /// Best-effort live-token marker. The recorder discards by default;
    /// SSE subscribers receive it directly. Used when the dispatcher
    /// supports streaming and we want to drive the trace dock's
    /// `STREAMING` badge.
    #[allow(dead_code)] // wired in a follow-up when streaming dispatchers land
    pub async fn emit_assistant_text_delta(&self, span_id: &str, delta_len: usize) {
        self.bus
            .publish(RunEvent::AssistantTextDelta(AssistantTextDeltaEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                delta_len,
            }))
            .await;
    }
}

/// Generate a fresh span id. ULID-shaped, time-prefixed so spans sort
/// chronologically without an explicit timestamp join. Lives next to
/// the emitter so callers don't grow an extra `ulid` import.
pub fn fresh_span_id() -> String {
    ulid::Ulid::new().to_string()
}
