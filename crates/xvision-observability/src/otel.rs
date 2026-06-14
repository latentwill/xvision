//! `OtelTeeRecorder` — opt-in OpenTelemetry tee on top of any
//! [`AgentRunRecorder`].
//!
//! Compiled only with `--features otel`. Per the observability plan's
//! "OpenTelemetry boundary" section, OTel is a derived sink: SQLite is
//! canonical, and the tee subscribes to the same [`crate::RunEventBus`]
//! so events cannot drift between sinks.
//!
//! # What the tee emits
//!
//! For every [`RunEvent`] the inner recorder accepts, the tee also opens
//! a short-lived `tracing::span!()` and attaches a small set of
//! attributes derived from the event payload. The `tracing-opentelemetry`
//! layer (configured by [`init_otel_pipeline`]) maps each tracing span
//! to an OpenTelemetry span on the configured OTLP exporter.
//!
//! # Hard rule — never export payload strings
//!
//! OTel collectors are commonly remote; the plan says:
//!
//! > Full prompts and full tool payloads never leave the local
//! > SQLite/blob store via OTel. OTel exports may carry hashes and
//! > attribute bags only.
//!
//! This file enforces that at the type level: every attribute setter
//! takes [`Attribute`] (the existing recorder-attribute enum), which
//! has no `From<&str>` / `From<String>` impl. The lint test in
//! `tests/otel_no_payload_lint.rs` asserts the public surface cannot
//! accept raw payload strings.
//!
//! # Trace / span ID propagation
//!
//! `SpanStartedEvent` carries optional `otel_trace_id` / `otel_span_id`
//! columns. With this feature on, the emitting end (the
//! `xvision-agent-client` IPC handler in Phase B) is expected to populate
//! them from the currently-active tracing span via
//! [`OtelIds::from_current`]. The recorder will then write them into the
//! `spans.otel_trace_id` / `spans.otel_span_id` columns so SQLite rows
//! can be joined to a Jaeger trace by ID, satisfying the acceptance
//! criterion that those columns be populated when the OTel feature is on.

use crate::events::RunEvent;
use crate::recorder::{AgentRunRecorder, Attribute, RecorderError};
use async_trait::async_trait;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Config as SdkTraceConfig, Tracer as SdkTracer};
use opentelemetry_sdk::Resource;
use std::env;
use std::sync::Arc;
use tracing::{span, Level, Span};
use tracing_opentelemetry::{OpenTelemetryLayer, OpenTelemetrySpanExt};

/// Environment variable contract — these are the upstream OTel
/// standard names. Documented in
/// `docs/runbook/observability-otel.md`.
pub const ENV_OTLP_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
pub const ENV_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";
pub const ENV_RESOURCE_ATTRIBUTES: &str = "OTEL_RESOURCE_ATTRIBUTES";

/// Initialise an OTLP-over-gRPC tracer wired into `tracing` via
/// `tracing-opentelemetry`. Returns an [`OpenTelemetryLayer`] the caller
/// can register on a [`tracing_subscriber::Registry`].
///
/// Reads `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, and
/// `OTEL_RESOURCE_ATTRIBUTES` per the standard contract. Defaults:
/// endpoint `http://localhost:4317`, service name `xvision`.
///
/// Production wiring lives in `xvision-engine`; this helper exists so
/// `tests/otel_tee_smoke.rs` can build a real OTel pipeline backed by an
/// in-memory exporter without duplicating env logic.
pub fn init_otel_pipeline<S>() -> Result<OpenTelemetryLayer<S, SdkTracer>, OtelInitError>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    let endpoint = env::var(ENV_OTLP_ENDPOINT).unwrap_or_else(|_| "http://localhost:4317".to_owned());
    let service_name = env::var(ENV_SERVICE_NAME).unwrap_or_else(|_| "xvn".to_owned());
    let resource = build_resource(&service_name);

    let exporter = opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint);

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(SdkTraceConfig::default().with_resource(resource))
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .map_err(|e| OtelInitError::Pipeline(e.to_string()))?;

    Ok(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Parse `OTEL_RESOURCE_ATTRIBUTES` (the standard `key=value,key=value`
/// form) and merge with the service name into an OTel [`Resource`].
/// Exposed so tests / alternate pipelines can reuse the same parsing.
pub fn build_resource(service_name: &str) -> Resource {
    let mut kvs: Vec<KeyValue> = vec![KeyValue::new("service.name", service_name.to_owned())];
    if let Ok(raw) = env::var(ENV_RESOURCE_ATTRIBUTES) {
        for pair in raw.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if !k.is_empty() {
                    kvs.push(KeyValue::new(k.to_owned(), v.to_owned()));
                }
            }
        }
    }
    Resource::new(kvs)
}

#[derive(Debug, thiserror::Error)]
pub enum OtelInitError {
    #[error("otel pipeline: {0}")]
    Pipeline(String),
}

/// Public OTel attribute API: every setter accepts [`Attribute`], never
/// `&str`. This is the load-bearing constraint that keeps full prompts
/// and tool payloads out of remote OTel collectors. The compile-fail
/// doc tests on [`Attribute`] plus `tests/otel_no_payload_lint.rs` lock
/// it in.
///
/// `key` is `&'static str` because OTel attribute keys are a bounded,
/// schema-defined vocabulary; callers should reach for one of the
/// `attr::*` constants in [`attr`] rather than inventing new keys.
pub fn add_attribute(span: &Span, key: &'static str, value: Attribute) {
    let kv = attribute_to_kv(key, &value);
    // `tracing-opentelemetry` exposes per-span OTel KV attachment through
    // `OpenTelemetrySpanExt::set_attribute`.
    span.set_attribute(kv.key, kv.value);
}

/// Convert an [`Attribute`] into an OTel [`KeyValue`]. Hashes and ids
/// surface as strings (already-tokenised — never raw payloads); counts
/// surface as i64; flags as bool. The mapping is total — there is no
/// path that constructs a `Value::String` from a payload field.
pub fn attribute_to_kv(key: &'static str, attr: &Attribute) -> KeyValue {
    match attr {
        Attribute::Hash(h) => KeyValue::new(key, h.clone()),
        Attribute::Id(id) => KeyValue::new(key, id.clone()),
        Attribute::Count(c) => KeyValue::new(key, *c),
        Attribute::Flag(b) => KeyValue::new(key, *b),
    }
}

/// Bounded, schema-defined attribute keys we attach to OTel spans. New
/// keys must be added here, not inlined at call sites, so the OTel
/// schema stays auditable.
pub mod attr {
    pub const RUN_ID: &str = "xvision.run.id";
    pub const SPAN_ID: &str = "xvision.span.id";
    pub const PARENT_SPAN_ID: &str = "xvision.span.parent_id";
    pub const SPAN_KIND: &str = "xvision.span.kind";
    pub const SPAN_STATUS: &str = "xvision.span.status";
    pub const STRATEGY_ID: &str = "xvision.strategy.id";
    pub const EVAL_RUN_ID: &str = "xvision.eval.run_id";
    pub const RETENTION_MODE: &str = "xvision.retention.mode";

    pub const MODEL_PROVIDER: &str = "xvision.model.provider";
    pub const MODEL_NAME: &str = "xvision.model.name";
    pub const MODEL_INPUT_TOKENS: &str = "xvision.model.input_tokens";
    pub const MODEL_OUTPUT_TOKENS: &str = "xvision.model.output_tokens";
    pub const MODEL_PROMPT_HASH: &str = "xvision.model.prompt_hash";
    pub const MODEL_RESPONSE_HASH: &str = "xvision.model.response_hash";

    pub const TOOL_NAME: &str = "xvision.tool.name";
    pub const TOOL_ORIGIN: &str = "xvision.tool.origin";
    pub const TOOL_INPUT_HASH: &str = "xvision.tool.input_hash";
    pub const TOOL_OUTPUT_HASH: &str = "xvision.tool.output_hash";
    pub const TOOL_EXIT_CODE: &str = "xvision.tool.exit_code";
    pub const TOOL_REQUIRES_APPROVAL: &str = "xvision.tool.requires_approval";
    pub const TOOL_IS_RUN_TERMINATOR: &str = "xvision.tool.is_run_terminator";

    pub const CHECKPOINT_ID: &str = "xvision.checkpoint.id";
    pub const CHECKPOINT_SEQUENCE: &str = "xvision.checkpoint.sequence";

    pub const ARTIFACT_ID: &str = "xvision.artifact.id";
    pub const ARTIFACT_KIND: &str = "xvision.artifact.kind";

    pub const DROPPED_COUNT: &str = "xvision.bus.dropped";
}

/// Helper for the producer side (Phase B `xvision-agent-client`): pull
/// the trace_id / span_id of the currently-active tracing span so the
/// producer can stamp them onto `SpanStartedEvent.otel_trace_id` /
/// `otel_span_id`. The recorder then writes them to the
/// `spans.otel_trace_id` / `spans.otel_span_id` columns — satisfying the
/// "populated on every recorder write when the OTel feature is on"
/// acceptance criterion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtelIds {
    pub trace_id: String,
    pub span_id: String,
}

impl OtelIds {
    /// Returns the OTel ids attached to the currently-entered tracing
    /// span, or `None` when no OTel context is active (e.g. tests
    /// without a configured tracer, or feature on but pipeline not
    /// installed).
    pub fn from_current() -> Option<Self> {
        let span = Span::current();
        let cx = span.context();
        let otel_span = cx.span();
        let sc = otel_span.span_context();
        if !sc.is_valid() {
            return None;
        }
        Some(Self {
            trace_id: sc.trace_id().to_string(),
            span_id: sc.span_id().to_string(),
        })
    }
}

/// Tee recorder: forwards every event to `inner` and emits a parallel
/// `tracing::span!()` per event for the OTel exporter.
pub struct OtelTeeRecorder {
    inner: Arc<dyn AgentRunRecorder>,
}

impl std::fmt::Debug for OtelTeeRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtelTeeRecorder").finish()
    }
}

impl OtelTeeRecorder {
    pub fn new(inner: Arc<dyn AgentRunRecorder>) -> Self {
        Self { inner }
    }

    /// Drop the OTel sink and return the wrapped recorder. Used by
    /// shutdown paths that want to disable OTel without rebuilding the
    /// bus subscriber list.
    pub fn into_inner(self) -> Arc<dyn AgentRunRecorder> {
        self.inner
    }
}

#[async_trait]
impl AgentRunRecorder for OtelTeeRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        self.inner.handle_event(event).await?;
        emit_otel_for(event);
        Ok(())
    }

    async fn mark_interrupted(&self, run_id: &str) -> Result<(), RecorderError> {
        let span = span!(Level::INFO, "xvision.run.mark_interrupted");
        let _enter = span.enter();
        add_attribute(&span, attr::RUN_ID, Attribute::id(run_id));
        self.inner.mark_interrupted(run_id).await
    }
}

/// Emit the per-event OTel span. Centralised so the mapping is one
/// place to audit; tests assert no payload strings sneak in.
fn emit_otel_for(event: &RunEvent) {
    match event {
        RunEvent::RunStarted(e) => {
            let s = span!(Level::INFO, "xvision.run.started");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            if let Some(sid) = &e.strategy_id {
                add_attribute(&s, attr::STRATEGY_ID, Attribute::id(sid));
            }
            if let Some(erid) = &e.eval_run_id {
                add_attribute(&s, attr::EVAL_RUN_ID, Attribute::id(erid));
            }
            add_attribute(&s, attr::RETENTION_MODE, Attribute::id(&e.retention_mode));
        }
        RunEvent::RunFinished(e) => {
            let s = span!(Level::INFO, "xvision.run.finished");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, attr::SPAN_STATUS, Attribute::id(e.status.as_db_str()));
        }
        RunEvent::RunInterrupted(e) => {
            let s = span!(Level::WARN, "xvision.run.interrupted");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
        }
        RunEvent::SpanStarted(e) => {
            let s = span!(Level::INFO, "xvision.span.started");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, attr::SPAN_KIND, Attribute::id(e.kind.as_db_str()));
            if let Some(parent) = &e.parent_span_id {
                add_attribute(&s, attr::PARENT_SPAN_ID, Attribute::id(parent));
            }
            // NOTE: e.name and e.attributes_json may contain operator
            // text; they are NOT mirrored to OTel. SQLite keeps them.
        }
        RunEvent::SpanFinished(e) => {
            let s = span!(Level::INFO, "xvision.span.finished");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::SPAN_STATUS, Attribute::id(e.status.as_db_str()));
        }
        RunEvent::ModelCallFinished(e) => {
            let s = span!(Level::INFO, "xvision.model.call");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::MODEL_PROVIDER, Attribute::id(&e.provider));
            add_attribute(&s, attr::MODEL_NAME, Attribute::id(&e.model));
            if let Some(n) = e.input_token_count {
                add_attribute(&s, attr::MODEL_INPUT_TOKENS, Attribute::count(n));
            }
            if let Some(n) = e.output_token_count {
                add_attribute(&s, attr::MODEL_OUTPUT_TOKENS, Attribute::count(n));
            }
            add_attribute(&s, attr::MODEL_PROMPT_HASH, Attribute::hash(&e.prompt_hash));
            if let Some(h) = &e.response_hash {
                add_attribute(&s, attr::MODEL_RESPONSE_HASH, Attribute::hash(h));
            }
            // e.tool_calls_requested is JSON of requested calls — that's
            // attribute-bag data, fine for SQLite, but NOT mirrored to
            // OTel: shape is unbounded and could leak tool input args.
        }
        RunEvent::ToolCallStarted(e) => {
            let s = span!(Level::INFO, "xvision.tool.call.started");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::TOOL_NAME, Attribute::id(&e.tool_name));
            add_attribute(&s, attr::TOOL_ORIGIN, Attribute::id(e.origin.as_db_string()));
            add_attribute(&s, attr::TOOL_INPUT_HASH, Attribute::hash(&e.input_hash));
            add_attribute(
                &s,
                attr::TOOL_REQUIRES_APPROVAL,
                Attribute::flag(e.requires_approval),
            );
            add_attribute(
                &s,
                attr::TOOL_IS_RUN_TERMINATOR,
                Attribute::flag(e.is_run_terminator),
            );
        }
        RunEvent::ToolCallFinished(e) => {
            let s = span!(Level::INFO, "xvision.tool.call.finished");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            if let Some(h) = &e.output_hash {
                add_attribute(&s, attr::TOOL_OUTPUT_HASH, Attribute::hash(h));
            }
            if let Some(c) = e.exit_code {
                add_attribute(&s, attr::TOOL_EXIT_CODE, Attribute::count(c));
            }
        }
        RunEvent::ToolCallFailed(e) => {
            let s = span!(Level::WARN, "xvision.tool.call.failed");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            // error_json is a structured JSON blob — SQLite-only.
        }
        RunEvent::ToolCallCancelled(e) => {
            let s = span!(Level::INFO, "xvision.tool.call.cancelled");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
        }
        RunEvent::BrokerCallStarted(e) => {
            // qa-trace-broker-spans: broker submits get a distinct
            // OTel span so the trace dock's `broker.call` row has a
            // matching OTel waterfall entry. Side / symbol surface as
            // attributes; the full structured payload stays on the
            // SQLite-side `attributes_json` (see sqlite.rs).
            let s = span!(Level::INFO, "xvision.broker.call.started");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.broker.symbol", Attribute::id(&e.symbol));
            add_attribute(&s, "xvision.broker.venue", Attribute::id(&e.venue));
        }
        RunEvent::BrokerCallFinished(e) => {
            let s = span!(Level::INFO, "xvision.broker.call.finished");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            if let Some(class) = &e.error_class {
                add_attribute(&s, "xvision.broker.error_class", Attribute::id(class));
            }
        }
        RunEvent::CheckpointWritten(e) => {
            let s = span!(Level::INFO, "xvision.checkpoint");
            let _g = s.enter();
            add_attribute(&s, attr::CHECKPOINT_ID, Attribute::id(&e.checkpoint_id));
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, attr::CHECKPOINT_SEQUENCE, Attribute::count(e.sequence));
            add_attribute(&s, "xvision.checkpoint.kind", Attribute::id(&e.kind));
            add_attribute(&s, attr::TOOL_INPUT_HASH, Attribute::hash(&e.input_hash));
            if let Some(h) = &e.output_hash {
                add_attribute(&s, attr::TOOL_OUTPUT_HASH, Attribute::hash(h));
            }
        }
        RunEvent::AssistantTextDelta(e) => {
            // Stream-only; emit a token-count event with NO text content.
            let s = span!(Level::TRACE, "xvision.assistant.delta");
            let _g = s.enter();
            add_attribute(&s, attr::SPAN_ID, Attribute::id(&e.span_id));
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(
                &s,
                attr::MODEL_OUTPUT_TOKENS,
                Attribute::count(e.delta_len as i64),
            );
        }
        RunEvent::SupervisorNote(e) => {
            // Supervisor notes ARE operator-authored text; SQLite is the
            // place for that. OTel gets a marker with role only.
            let s = span!(Level::INFO, "xvision.supervisor.note");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.supervisor.role", Attribute::id(&e.role));
            add_attribute(&s, "xvision.supervisor.severity", Attribute::id(&e.severity));
        }
        RunEvent::ArtifactWritten(e) => {
            let s = span!(Level::INFO, "xvision.artifact.written");
            let _g = s.enter();
            add_attribute(&s, attr::ARTIFACT_ID, Attribute::id(&e.artifact_id));
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, attr::ARTIFACT_KIND, Attribute::id(&e.kind));
        }
        RunEvent::SidecarError(e) => {
            let s = span!(Level::ERROR, "xvision.sidecar.error");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.sidecar.severity", Attribute::id(&e.severity));
        }
        RunEvent::BackpressureDropped(e) => {
            let s = span!(Level::WARN, "xvision.bus.backpressure_dropped");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, attr::DROPPED_COUNT, Attribute::count(e.dropped as i64));
        }
        // Memory + engine events carry operator/agent text (recall item
        // previews, memory bodies, engine payload_json) — those bodies
        // belong in SQLite, NEVER OTel. The tee emits id / count
        // markers only so the per-event payload text never crosses the
        // OTel boundary (enforced by `tests/otel_no_payload_lint.rs`).
        RunEvent::MemoryRecall(e) => {
            let s = span!(Level::INFO, "xvision.memory.recall");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.memory.namespace", Attribute::id(&e.namespace));
            add_attribute(&s, "xvision.memory.decision_id", Attribute::count(e.decision_id));
            add_attribute(
                &s,
                "xvision.memory.recall_count",
                Attribute::count(e.items.len() as i64),
            );
        }
        RunEvent::MemoryWrite(e) => {
            let s = span!(Level::INFO, "xvision.memory.write");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.memory.namespace", Attribute::id(&e.namespace));
            add_attribute(&s, "xvision.memory.decision_id", Attribute::count(e.decision_id));
            add_attribute(&s, "xvision.memory.item_id", Attribute::id(&e.memory_item_id));
        }
        RunEvent::EngineEvent(e) => {
            let s = span!(Level::INFO, "xvision.engine.event");
            let _g = s.enter();
            add_attribute(&s, attr::RUN_ID, Attribute::id(&e.run_id));
            add_attribute(&s, "xvision.engine.kind", Attribute::id(&e.kind));
            if let Some(span_id) = e.span_id.as_ref() {
                add_attribute(&s, attr::SPAN_ID, Attribute::id(span_id));
            }
        }
    }
}

/// Shut down the global OTel tracer provider, flushing any pending
/// batches. Call from process shutdown so we don't drop spans on exit.
pub fn shutdown_otel_pipeline() {
    global::shutdown_tracer_provider();
}
