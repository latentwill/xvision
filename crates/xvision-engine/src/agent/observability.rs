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
    AssistantTextDeltaEvent, BrokerCallFinishedEvent, BrokerCallOutcome, BrokerCallStartedEvent,
    BrokerSide, ModelCallFinishedEvent, RunEvent, RunEventBus, RunFinishedEvent, RunStartedEvent,
    RunStatus, SpanFinishedEvent, SpanKind, SpanStartedEvent, SpanStatus,
};

/// Retention policy carried on `ObsEmitter` so producers can gate
/// payload-bearing events (e.g. `AssistantTextDelta.delta_text`)
/// without round-tripping through `ApiContext::obs_config` per call.
/// Defaults are deny-by-default so a caller that constructs an
/// `ObsEmitter` without setting a policy never leaks raw bodies.
#[derive(Clone, Copy, Debug)]
pub struct ObsRetentionPolicy {
    pub store_responses: bool,
    pub mode_is_full_debug: bool,
    pub max_payload_bytes: usize,
}

impl Default for ObsRetentionPolicy {
    fn default() -> Self {
        // Deny-by-default. Callers (engine eval handler) opt in by
        // calling `with_retention(...)` after reading the resolved
        // ObservabilityConfig off `ApiContext`.
        Self {
            store_responses: false,
            mode_is_full_debug: false,
            max_payload_bytes: 0,
        }
    }
}

impl ObsRetentionPolicy {
    pub fn from_config(cfg: &xvision_observability::ObservabilityConfig) -> Self {
        use xvision_observability::RetentionMode;
        Self {
            store_responses: cfg.retention.store_responses,
            mode_is_full_debug: cfg.retention.mode == RetentionMode::FullDebug,
            max_payload_bytes: cfg.retention.max_payload_bytes as usize,
        }
    }

    /// Whether assistant body text is allowed on the wire. Bodies only
    /// stream when the operator opted into FullDebug AND
    /// store_responses; otherwise we suppress to avoid leaking raw
    /// payloads over SSE to the dashboard.
    pub fn allow_assistant_body(&self) -> bool {
        self.mode_is_full_debug && self.store_responses
    }

    /// Apply the policy to a candidate `delta_text`. Returns the
    /// bounded text — empty when emission is disallowed, truncated to
    /// `max_payload_bytes` with a trailing `…` marker when too long.
    /// Pure; safe to call without a tokio runtime.
    pub fn apply_to_body(&self, delta_text: &str) -> String {
        if !self.allow_assistant_body() {
            return String::new();
        }
        let cap = self.max_payload_bytes;
        if cap == 0 || delta_text.len() <= cap {
            return delta_text.to_string();
        }
        let mut end = cap;
        while end > 0 && !delta_text.is_char_boundary(end) {
            end -= 1;
        }
        let mut s = delta_text[..end].to_string();
        s.push('…');
        s
    }
}

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
    retention: ObsRetentionPolicy,
}

impl ObsEmitter {
    /// Construct an emitter bound to a specific run id. The eval
    /// executor calls this once per run and clones into per-slot
    /// `SlotInput`s.
    pub fn new(bus: Arc<RunEventBus>, run_id: impl Into<String>) -> Self {
        Self {
            bus,
            run_id: run_id.into(),
            retention: ObsRetentionPolicy::default(),
        }
    }

    /// Attach a resolved retention policy. Without this call the
    /// emitter denies all payload-bearing emissions (assistant body
    /// text). The eval handler reads the policy off
    /// `ctx.obs_config` and wires it in.
    pub fn with_retention(mut self, policy: ObsRetentionPolicy) -> Self {
        self.retention = policy;
        self
    }

    /// Read-only accessor for the active retention policy.
    pub fn retention(&self) -> ObsRetentionPolicy {
        self.retention
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

    /// Best-effort live-token chunk. The recorder discards by default;
    /// SSE subscribers receive it directly and the trace dock's
    /// `SpanInspector` accumulates the chunks into the live body. The
    /// final response payload is still persisted via
    /// `emit_model_call_finished`; this method does not write to disk.
    ///
    /// Retention gate (see `bound_delta_text`): hash_only / redacted /
    /// any non-FullDebug policy suppresses the raw text but still
    /// publishes the event so the dashboard's span counts stay
    /// accurate.
    pub async fn emit_assistant_text_delta(&self, span_id: &str, delta_text: &str) {
        let bounded = self.retention.apply_to_body(delta_text);
        self.bus
            .publish(RunEvent::AssistantTextDelta(AssistantTextDeltaEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                delta_len: delta_text.chars().count(),
                delta_text: bounded,
            }))
            .await;
    }

    /// Open a `broker.call` span around one `BrokerSurface::submit_order`
    /// invocation. The eval executor pairs this with exactly one
    /// `emit_broker_call_finished` call carrying the same `span_id`.
    ///
    /// Adds the trace-fidelity row the operator asked for in round-2
    /// (#8, #14): Buy / Sell / Close / Short submissions are now
    /// auditable on the trace dock alongside model.call rows.
    #[allow(clippy::too_many_arguments)]
    pub async fn emit_broker_call_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        side: BrokerSide,
        symbol: impl Into<String>,
        qty: f64,
        intended_price: Option<f64>,
        order_type: impl Into<String>,
        venue: impl Into<String>,
        idempotency_key: Option<String>,
    ) {
        let symbol = symbol.into();
        let venue = venue.into();
        let order_type = order_type.into();
        let name = format!("{venue} {symbol} {side:?}");
        // Persist the broker payload on the span row's `attributes_json`
        // so the dashboard read path can project a `broker_call`
        // payload onto the wire span without joining a second table.
        // `qa-trace-broker-spans` deliberately doesn't add a
        // `broker_calls` table (the contract forbids migrations);
        // attributes_json is the durable carrier.
        let started_attrs = serde_json::json!({
            "broker_call": {
                "side": side,
                "symbol": symbol,
                "qty": qty,
                "intended_price": intended_price,
                "order_type": order_type,
                "venue": venue,
                "idempotency_key": idempotency_key,
            }
        });
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::BrokerCall,
                name,
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(started_attrs.to_string()),
            }))
            .await;
        self.bus
            .publish(RunEvent::BrokerCallStarted(BrokerCallStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                side,
                symbol,
                qty,
                intended_price,
                order_type,
                venue,
                idempotency_key,
            }))
            .await;
    }

    /// Close a `broker.call` span with the broker's terminal state.
    /// Always emits BOTH `BrokerCallFinished` AND a span-level
    /// `SpanFinished` so the recorder can stamp the close timestamp
    /// without parsing the broker payload.
    #[allow(clippy::too_many_arguments)]
    pub async fn emit_broker_call_finished(
        &self,
        span_id: &str,
        outcome: BrokerCallOutcome,
        fill_price: Option<f64>,
        fill_qty: Option<f64>,
        fee: Option<f64>,
        broker_order_id: Option<String>,
        error_class: Option<String>,
        error_message: Option<String>,
        severity: Option<&'static str>,
    ) {
        // Recoverable broker errors land as `Rejected` outcome +
        // `severity = "warn"` so the trace dock can render them
        // visually distinct from `Failed` (which is the fatal /
        // run-terminating path). agent-error-feedback-self-healing.
        let span_status = match outcome {
            BrokerCallOutcome::Filled => SpanStatus::Ok,
            BrokerCallOutcome::Rejected if severity == Some("warn") => SpanStatus::Ok,
            BrokerCallOutcome::Rejected
            | BrokerCallOutcome::Cancelled
            | BrokerCallOutcome::Failed => SpanStatus::Error,
        };
        self.bus
            .publish(RunEvent::BrokerCallFinished(BrokerCallFinishedEvent {
                span_id: span_id.to_string(),
                outcome,
                fill_price,
                fill_qty,
                fee,
                broker_order_id,
                error_class: error_class.clone(),
                error_message: error_message.clone(),
                severity: severity.map(|s| s.to_string()),
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: span_status,
                error_json: error_message.map(|m| {
                    serde_json::json!({
                        "class": error_class,
                        "message": m,
                    })
                    .to_string()
                }),
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

#[cfg(test)]
mod retention_tests {
    use super::*;
    use xvision_observability::{ObservabilityConfig, RetentionMode};

    fn full_debug_policy(max_bytes: usize) -> ObsRetentionPolicy {
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::FullDebug;
        cfg.retention.store_responses = true;
        cfg.retention.max_payload_bytes = max_bytes as u64;
        ObsRetentionPolicy::from_config(&cfg)
    }

    fn hash_only_policy() -> ObsRetentionPolicy {
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::HashOnly;
        cfg.retention.store_responses = false;
        ObsRetentionPolicy::from_config(&cfg)
    }

    #[test]
    fn full_debug_passes_text_through_unchanged() {
        let p = full_debug_policy(1024);
        assert_eq!(p.apply_to_body("hello world"), "hello world");
    }

    #[test]
    fn hash_only_suppresses_body() {
        let p = hash_only_policy();
        assert_eq!(p.apply_to_body("secret prompt body"), "");
    }

    #[test]
    fn default_policy_denies_body() {
        // ObsRetentionPolicy::default is deny-by-default. ObsEmitter::new
        // installs this until the caller wires a real policy.
        let p = ObsRetentionPolicy::default();
        assert_eq!(p.apply_to_body("should not leak"), "");
    }

    #[test]
    fn body_truncated_to_max_payload_bytes_with_marker() {
        let p = full_debug_policy(10);
        let out = p.apply_to_body("abcdefghijklmnopqrstuvwxyz");
        assert!(out.ends_with('…'), "expected truncation marker, got: {out:?}");
        assert!(out.starts_with("abcdefghij"), "expected first 10 bytes, got: {out:?}");
    }

    #[test]
    fn truncation_walks_back_to_utf8_char_boundary() {
        // A 4-byte char "🎯" at byte position 8..12; cap=10 falls
        // mid-char, so the bound must walk back to byte 8 and emit
        // 8 bytes + ellipsis (no mojibake).
        let p = full_debug_policy(10);
        let out = p.apply_to_body("12345678🎯end");
        assert!(out.ends_with('…'));
        assert_eq!(&out[..out.len() - '…'.len_utf8()], "12345678");
    }
}
