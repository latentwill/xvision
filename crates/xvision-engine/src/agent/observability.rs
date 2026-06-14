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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::agent::llm::{LlmRequest, Message, ToolDefinition};
use crate::eval::cost::compute_token_cost_usd_from_catalog;
use xvision_core::providers::Catalog;
use xvision_observability::{
    AssistantTextDeltaEvent, BlobStore, BrokerCallFinishedEvent, BrokerCallOutcome, BrokerCallStartedEvent,
    BrokerSide, EngineEvent, MemoryRecallEvent, MemoryRecallItem, MemoryWriteEvent, ModelCallFinishedEvent,
    Redactor, RetentionMode, RiskLevel, RunEvent, RunEventBus, RunFinishedEvent, RunStartedEvent, RunStatus,
    SideEffectLevel, SpanAttributes, SpanFinishedEvent, SpanKind, SpanStartedEvent, SpanStatus,
    SupervisorNoteEvent, ToolCallFailedEvent, ToolCallFinishedEvent, ToolCallStartedEvent, ToolOrigin,
};

/// Serializable digest input for `compute_prompt_hash`. Private —
/// callers should never need to construct it directly. Field order is
/// load-bearing because `serde_json::to_vec` is order-preserving and
/// the digest must be stable across identical prompts.
///
/// Reasoning / thinking blocks are NOT a `ContentBlock` variant in our
/// domain — `AnthropicDispatch::complete` strips them at the wire
/// boundary, so by the time messages reach an `LlmRequest` they're
/// already reasoning-free. If a future ContentBlock variant for
/// thinking is added, that work must extend this helper to strip
/// before hashing.
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PromptDigestInput<'a> {
    system_prompt: &'a str,
    messages: &'a [Message],
    tools: &'a [ToolDefinition],
}

/// SHA-256 digest of an `LlmRequest`'s prompt content. Returns
/// `sha256:<64-hex-lowercase>` so the algorithm is explicit and
/// future-migratable. Two requests with identical (system_prompt,
/// messages, tools) produce identical hashes regardless of
/// run_id / span_id / model / sampling params.
pub fn compute_prompt_hash(req: &LlmRequest) -> String {
    let bytes = canonical_prompt_bytes(req);
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

/// Canonical byte serialization of an `LlmRequest`'s prompt content.
/// The same `PromptDigestInput` shape that `compute_prompt_hash`
/// digests — so the hash stays stable across runtime knobs (model,
/// sampling, max_tokens) that don't change prompt semantics. Used for
/// dedup/cache-key only; the persisted blob uses
/// [`canonical_request_bytes`] instead so operators reading the trace
/// see the full request (including `response_schema`, which Anthropic
/// appends into the system prompt at dispatch time).
pub fn canonical_prompt_bytes(req: &LlmRequest) -> Vec<u8> {
    let input = PromptDigestInput {
        system_prompt: &req.system_prompt,
        messages: &req.messages,
        tools: &req.tools,
    };
    // serde_json::to_vec is deterministic for our domain types
    // (no HashMap; structs serialize in declaration order). If
    // serialization ever fails it indicates a programming error in
    // the domain types, not a runtime condition — fall back to a
    // marker payload so the trace ledger stays non-null.
    serde_json::to_vec(&input).unwrap_or_else(|_| b"prompt-digest-serialize-error".to_vec())
}

/// Full-request byte serialization used for the persisted prompt
/// blob. Includes every field the provider receives — model,
/// system_prompt, messages, max_tokens, tools, temperature,
/// response_schema — so a FullDebug trace can reconstruct what was
/// sent. Critically, this preserves `response_schema`: Anthropic's
/// dispatcher splices it into the system prompt at the wire boundary,
/// so omitting it (as the hash input does) would leave trader-call
/// blobs missing the schema instructions. Two prompts that hash
/// identically can still produce different request blobs.
pub fn canonical_request_bytes(req: &LlmRequest) -> Vec<u8> {
    // LlmRequest is itself Serialize with deny_unknown_fields, so this
    // round-trips losslessly. Same serialize-error fallback as
    // canonical_prompt_bytes — programming error, not runtime.
    serde_json::to_vec(req).unwrap_or_else(|_| b"request-serialize-error".to_vec())
}

/// Truncate a payload to `cap` bytes for blob persistence, respecting
/// UTF-8 char boundaries when present and appending an ellipsis marker
/// so operators reading the blob can tell it was clipped. `cap == 0`
/// means "no cap" (`max_payload_bytes` default for un-configured
/// policies — same convention `apply_to_body` uses for the body path).
/// Pure; safe to call without a tokio runtime.
pub fn cap_blob_bytes(bytes: Vec<u8>, cap: usize) -> Vec<u8> {
    if cap == 0 || bytes.len() <= cap {
        return bytes;
    }
    // Try a UTF-8-safe boundary scan first — prompts and responses
    // are typed strings, so almost always valid UTF-8.
    if let Ok(s) = std::str::from_utf8(&bytes) {
        let mut end = cap;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        let mut out = s[..end].to_string().into_bytes();
        out.extend_from_slice("…".as_bytes());
        return out;
    }
    // Non-UTF-8 (shouldn't happen in practice): hard truncate + ASCII
    // marker so the cap is still honored.
    let mut out = bytes;
    out.truncate(cap);
    out.extend_from_slice(b"...[truncated]");
    out
}

/// SHA-256 digest of the assistant text accumulation. Mirrors
/// `compute_prompt_hash`'s `sha256:<hex>` format. Callers pass `None`
/// in for empty responses (tool-use-only turns); this helper does not
/// try to detect emptiness itself so the call site retains control.
pub fn compute_response_hash(text: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(text.as_bytes())))
}

/// Retention policy carried on `ObsEmitter` so producers can gate
/// payload-bearing events (e.g. `AssistantTextDelta.delta_text`)
/// without round-tripping through `ApiContext::obs_config` per call.
/// Defaults are deny-by-default so a caller that constructs an
/// `ObsEmitter` without setting a policy never leaks raw bodies.
#[derive(Clone, Copy, Debug)]
pub struct ObsRetentionPolicy {
    pub store_prompts: bool,
    pub store_responses: bool,
    pub mode: RetentionMode,
    pub mode_is_full_debug: bool,
    pub max_payload_bytes: usize,
}

impl Default for ObsRetentionPolicy {
    fn default() -> Self {
        // Deny-by-default. Callers (engine eval handler) opt in by
        // calling `with_retention(...)` after reading the resolved
        // ObservabilityConfig off `ApiContext`.
        Self {
            store_prompts: false,
            store_responses: false,
            mode: RetentionMode::HashOnly,
            mode_is_full_debug: false,
            max_payload_bytes: 0,
        }
    }
}

impl ObsRetentionPolicy {
    pub fn from_config(cfg: &xvision_observability::ObservabilityConfig) -> Self {
        Self {
            store_prompts: cfg.retention.store_prompts,
            store_responses: cfg.retention.store_responses,
            mode: cfg.retention.mode,
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
    blob_store: Option<BlobStore>,
    /// Provider catalogs the emitter consults when computing
    /// `model_calls.cost_usd` at `emit_model_call_finished*` time.
    /// Keyed by `ProviderEntry.name` (matches `Catalog.provider`). The
    /// emit path falls back to scanning every catalog by model id when
    /// the exact provider key isn't present — provider strings carried
    /// on the `LlmRequest` (`input.slot.provider`) don't always match
    /// the registered provider name, and "unknown model" is still
    /// preferable to "wrong model".
    ///
    /// Empty map => no priced lookup; every emit publishes
    /// `cost_usd = None`. That's the legacy behaviour and exactly what
    /// callers who don't wire `with_catalogs(...)` get.
    catalogs: Arc<HashMap<String, Arc<Catalog>>>,
}

/// Track which `(provider, model)` pairs we've already warned about
/// for missing pricing. Process-wide singleton so a long-lived server
/// emits ONE debug line per unique pair regardless of how many spans
/// fire — protects the operator's log scrollback from a tight inner
/// loop on an unpriced model.
fn unpriced_seen() -> &'static Mutex<HashSet<(String, String)>> {
    static SEEN: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
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
            blob_store: None,
            catalogs: Arc::new(HashMap::new()),
        }
    }

    /// Attach provider catalogs so the emit path can compute
    /// `cost_usd` from `(input_tokens, output_tokens, pricing)` at
    /// span-close time. Without this, the emitter publishes
    /// `cost_usd = None` for every model call — matching pre-2026-05-19
    /// behaviour where `model_calls.cost_usd` was always NULL.
    ///
    /// Map shape: `ProviderEntry.name` → `Arc<Catalog>`. Multiple
    /// providers can be wired in one call; the emitter holds them in
    /// an `Arc` so cloning the emitter is cheap.
    pub fn with_catalogs(mut self, catalogs: HashMap<String, Arc<Catalog>>) -> Self {
        self.catalogs = Arc::new(catalogs);
        self
    }

    /// Compute the USD cost for a model call against the wired
    /// catalogs. Returns `None` when:
    ///   - no catalogs are wired (the default), OR
    ///   - the model id isn't present in any wired catalog, OR
    ///   - the catalog entry has no pricing populated (Anthropic /
    ///     bare OpenAI / OpenRouter free routes).
    ///
    /// On the unpriced/missing-model path, emits ONE
    /// `tracing::debug!` line per `(provider, model)` pair per
    /// process — see `unpriced_seen` for the dedupe guard.
    ///
    /// Provider-key resolution: tries the exact `provider` key first,
    /// then falls back to scanning every wired catalog. Slot-side
    /// provider strings (`input.slot.provider`) are operator-typed and
    /// don't always match `ProviderEntry.name`; falling back keeps the
    /// cost path resilient without forcing a per-call provider rename.
    fn compute_cost_usd(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<f64> {
        if self.catalogs.is_empty() {
            // No catalogs wired — legacy behaviour, no log spam.
            return None;
        }
        // Exact provider key first.
        if let Some(cat) = self.catalogs.get(provider) {
            if let Some(cost) = compute_token_cost_usd_from_catalog(input_tokens, output_tokens, model, cat) {
                return Some(cost);
            }
        }
        // Fallback: scan all catalogs for this model id. OpenRouter
        // bundles models from many vendors under one provider entry,
        // so the slot's `provider` might be "anthropic" while the
        // priced catalog lives under "openrouter".
        for cat in self.catalogs.values() {
            if let Some(cost) = compute_token_cost_usd_from_catalog(input_tokens, output_tokens, model, cat) {
                return Some(cost);
            }
        }
        // No match anywhere — log once per (provider, model) so the
        // operator can see why `model_calls.cost_usd` is NULL for this
        // pair without drowning in repeats.
        log_unpriced_once(provider, model);
        None
    }

    /// Attach a resolved retention policy. Without this call the
    /// emitter denies all payload-bearing emissions (assistant body
    /// text). The eval handler reads the policy off
    /// `ctx.obs_config` and wires it in.
    pub fn with_retention(mut self, policy: ObsRetentionPolicy) -> Self {
        self.retention = policy;
        self
    }

    /// Attach a `BlobStore` so `emit_model_call_finished_with_payloads`
    /// can persist prompt / response bodies under `full_debug` and
    /// `redacted` retention. Without this call, the emitter falls
    /// back to publishing `prompt_payload_ref: None` /
    /// `response_payload_ref: None` regardless of retention — same
    /// observable behaviour as `hash_only`.
    ///
    /// `harness-payload-blob-write`: closes PR #282's investigation
    /// gap where `BlobStore::write` had zero production callers and
    /// the trace dock rendered the "prompt body not captured for
    /// this run" placeholder on every full_debug run.
    pub fn with_blob_store(mut self, store: BlobStore) -> Self {
        self.blob_store = Some(store);
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
    ///
    /// Also emits a `state.transition` span (`None → Running`) so the
    /// trace dock's per-run state timeline records the run-start
    /// transition without having to infer it from the absence of a
    /// prior state. Span ordering: `RunStarted` is published first so
    /// the recorder has the `agent_runs` row before the
    /// `state.transition` span tries to FK into it.
    pub async fn emit_run_started(&self, objective: impl Into<String>, retention_mode: impl Into<String>) {
        self.bus
            .publish(RunEvent::RunStarted(RunStartedEvent {
                run_id: self.run_id.clone(),
                objective: objective.into(),
                strategy_id: None,
                eval_run_id: Some(self.run_id.clone()),
                source_cli_job_id: None,
                started_at: Utc::now(),
                retention_mode: retention_mode.into(),
                trajectory_mode: None,
                sidecar_version: None,
                cline_sdk_version: None,
                protocol_version: None,
                skills_json: None,
                mcp_servers_json: None,
            }))
            .await;
        self.emit_state_transition(&fresh_span_id(), None, None, RunStatus::Running)
            .await;
    }

    /// Mark the run terminal. `status` should be `Completed` /
    /// `Failed` / `Cancelled` to match the recorder's
    /// `agent_runs.status` text vocabulary.
    ///
    /// Emits a `state.transition` span (`Running → status`) before the
    /// `RunFinished` event so the trace dock sees the closing
    /// transition while the run row is still in the running state.
    pub async fn emit_run_finished(&self, status: RunStatus, error: Option<String>) {
        self.emit_state_transition(&fresh_span_id(), None, Some(RunStatus::Running), status)
            .await;
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

    /// Correct a child run's terminal status when the child run id differs
    /// from this emitter's own `run_id`. Used by the Cline executor to
    /// override the sidecar's `event.run_finished(completed)` notification
    /// when the step actually failed — the sidecar always emits
    /// `run_finished(completed)` on `end_run`, even for a failed step, so
    /// without this correction the child `agent_runs` row would be left as
    /// `completed` while its parent is `failed`.
    ///
    /// Publishes only `RunFinished` (no `state.transition` span) because the
    /// child run row was managed by the sidecar-side event stream; the
    /// emitter does not own its span history. The recorder's `UPDATE WHERE id
    /// = run_id` will overwrite whatever the sidecar last wrote.
    pub async fn emit_child_run_failed(&self, child_run_id: &str, error: String) {
        self.bus
            .publish(RunEvent::RunFinished(RunFinishedEvent {
                run_id: child_run_id.to_string(),
                finished_at: Utc::now(),
                status: RunStatus::Failed,
                final_artifact_id: None,
                error: Some(error),
            }))
            .await;
    }

    /// Emit an instantaneous `state.transition` span recording a
    /// change in run lifecycle status. Open and immediate close-ok so
    /// the trace dock renders it as a point event rather than a
    /// duration. `attributes_json` carries `{"from", "to"}` merged
    /// with the F-2 `SpanAttributes` bag — `from` is the
    /// `RunStatus.as_db_str()` of the prior state or JSON `null` for
    /// the run-start transition.
    ///
    /// Added by F-4 (`harness-span-taxonomy-extension`). The recovery
    /// state machine planned in F-5 will emit a transition span per
    /// failure-class promotion using this helper; F-4 wires only the
    /// run-lifecycle transitions.
    pub async fn emit_state_transition(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        from: Option<RunStatus>,
        to: RunStatus,
    ) {
        let typed_attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        base.insert(
            "from".to_string(),
            from.map(|s| serde_json::Value::String(s.as_db_str().to_string()))
                .unwrap_or(serde_json::Value::Null),
        );
        base.insert(
            "to".to_string(),
            serde_json::Value::String(to.as_db_str().to_string()),
        );
        let merged = typed_attrs.merge_into_object(base);
        let now = Utc::now();
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::StateTransition,
                name: format!(
                    "{} → {}",
                    from.map(|s| s.as_db_str()).unwrap_or("(start)"),
                    to.as_db_str()
                ),
                started_at: now,
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(merged),
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: now,
                status: SpanStatus::Ok,
                error_json: None,
            }))
            .await;
    }

    // ---- F43 (`trace-dock-emitters`) ----------------------------------
    //
    // The following helpers fill the migration-018 trace-dock surface
    // that previously had zero engine writers: `tool_calls`, `events`,
    // per-decision `spans`, broadened `supervisor_notes`. See
    // FOLLOWUPS.md § F43.

    /// Emit a bar-level engine lifecycle event onto the `events` table.
    /// `kind` is a free-form snake_case string. Known kinds carried by
    /// F43: `decision_started`, `decision_completed`, `fill_attempted`,
    /// `guardrail_fired`, `early_stop_triggered`, `flat_skip_fired`,
    /// `preflight_warning`, `broker_rule_violation`, `cost_cap_warning`.
    ///
    /// `payload_json` MUST already be free of secrets — the writer
    /// trusts the producer. Callers that pass user-typed strings should
    /// run them through the [`Redactor`] first; structured payloads
    /// (ints / known enum strings) need no scrubbing.
    pub async fn emit_engine_event(&self, kind: &str, span_id: Option<String>, payload_json: Option<String>) {
        self.bus
            .publish(RunEvent::EngineEvent(EngineEvent {
                run_id: self.run_id.clone(),
                span_id,
                kind: kind.to_string(),
                payload_json,
                created_at: Utc::now(),
            }))
            .await;
    }

    /// Open an `agent.decision` span. Caller pairs this with exactly
    /// one `emit_span_finished_ok` (or `_error`) using the same
    /// `span_id`. Carries `decision_index` in the typed attributes
    /// bag so the trace dock can group child spans without inferring
    /// the index from the bar timeline.
    ///
    /// QA30: the span attributes now also include the entry-state
    /// snapshot the trader is about to operate on (`bar_ts`,
    /// `mark_price`, `position_pre`), so a SpanInspector reader can
    /// see "what was on the table" when this decision opened. The
    /// post-decision payload (action / fill / position_post) lives in
    /// the `decision_completed` engine event because the action isn't
    /// known until after the model + risk + executor run.
    ///
    /// WS-10 (`trace-obs-decision-input`): `decision_input` is the
    /// structured snapshot of the market context the strategy agent
    /// actually saw this bar — the indicator panel, current-bar OHLCV,
    /// regime label, whether the briefing was FULL or a DELTA (and which
    /// indicators changed), and a BOUNDED `bar_history` summary (count +
    /// first/last ts — never the inlined window). It is merged into the
    /// span attributes under the `decision_input` key so the rich
    /// context is queryable and lands in the run export automatically
    /// (the export selects `attributes_json` for every span). Build it
    /// with [`crate::eval::executor::backtest::build_decision_input`]
    /// from the trader seed. `None` preserves the pre-WS-10 attribute
    /// shape.
    pub async fn emit_decision_span_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        decision_index: i64,
        asset: Option<&str>,
        bar_ts: Option<chrono::DateTime<chrono::Utc>>,
        mark_price: Option<f64>,
        position_pre: Option<f64>,
        decision_input: Option<serde_json::Value>,
    ) {
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            decision_index: Some(decision_index),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        if let Some(a) = asset {
            base.insert("asset".to_string(), serde_json::Value::String(a.to_string()));
        }
        if let Some(ts) = bar_ts {
            base.insert("bar_ts".to_string(), serde_json::Value::String(ts.to_rfc3339()));
        }
        if let Some(p) = mark_price {
            if let Some(n) = serde_json::Number::from_f64(p) {
                base.insert("mark_price".to_string(), serde_json::Value::Number(n));
            }
        }
        if let Some(p) = position_pre {
            if let Some(n) = serde_json::Number::from_f64(p) {
                base.insert("position_pre".to_string(), serde_json::Value::Number(n));
            }
        }
        // WS-10: the structured market-context snapshot the agent saw.
        if let Some(di) = decision_input {
            base.insert("decision_input".to_string(), di);
        }
        let merged = if base.is_empty() {
            attrs.to_attributes_json()
        } else {
            Some(attrs.merge_into_object(base))
        };
        let name = match asset {
            Some(a) => format!("decision#{decision_index} {a}"),
            None => format!("decision#{decision_index}"),
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::AgentDecision,
                name,
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: merged,
            }))
            .await;
    }

    /// Open an `agent.plan` span (`SpanKind::AgentPlan`) at pipeline
    /// entry. Caller pairs this with exactly one `emit_span_finished_ok`
    /// (or `_error`) using the same `span_id`. The span is the single
    /// per-decision-cycle record of the resolved pipeline topology — the
    /// ordered list of stages (`{ role, model?, capability? }`) that are
    /// about to run — so the trace dock can answer "which slots / roles /
    /// models drove this decision" without inferring it from the child
    /// `model.call` spans.
    ///
    /// `topology` is a caller-built JSON value (typically an object with
    /// a `topology` array). It is merged into the typed `SpanAttributes`
    /// bag under whatever keys the value carries — mirroring
    /// `emit_decision_span_started`'s `merge_into_object` structure — so
    /// the resolved plan lands in `attributes_json` and rides into the
    /// run export automatically.
    ///
    /// WS-12 (`trace-obs-ws12`): wires the previously producer-less
    /// `SpanKind::AgentPlan`. Per-stage spans, inter-stage handoff
    /// events, and parse-failure events are a separate follow-up — this
    /// helper emits ONLY the plan span.
    pub async fn emit_agent_plan_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        topology: serde_json::Value,
    ) {
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            ..SpanAttributes::default()
        };
        let attributes_json = match topology {
            serde_json::Value::Object(map) => attrs.merge_into_object(map),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("topology".to_string(), other);
                attrs.merge_into_object(map)
            }
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::AgentPlan,
                name: "plan".to_string(),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(attributes_json),
            }))
            .await;
    }

    /// Emit a `tool.call` span + matching `tool_calls` row around one
    /// tool invocation. Caller pairs with exactly one
    /// `emit_tool_call_finished` or `emit_tool_call_failed` using the
    /// same `span_id`.
    ///
    /// `args_redacted` is the producer's input payload AFTER passing
    /// through [`Redactor`] — never raw provider tokens or broker
    /// keys. The helper hashes the redacted text into `input_hash`
    /// and stores the redacted text on the (per-policy) blob ref slot
    /// is left `None` for now; F43's scope is to ensure the row
    /// exists, not to wire payload blobs to a new path.
    pub async fn emit_tool_call_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        tool_name: &str,
        args_redacted: &str,
    ) {
        // Open the parent span row first so the tool_calls row has a
        // FK target.
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            tool_name: Some(tool_name.to_string()),
            ..SpanAttributes::default()
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::ToolCall,
                name: tool_name.to_string(),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: attrs.to_attributes_json(),
            }))
            .await;
        let input_hash = format!("sha256:{}", hex::encode(Sha256::digest(args_redacted.as_bytes())));
        // F43 conservative defaults — the engine's native tools today
        // are all read-only (indicator lookups, price fetches). The
        // dashboard's risk-tier UI projects safe_read as "no warning";
        // future tools that write state should override at the call
        // site.
        self.bus
            .publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
                span_id: span_id.to_string(),
                tool_name: tool_name.to_string(),
                origin: ToolOrigin::Native,
                tool_version: None,
                tool_hash: None,
                side_effect_level: SideEffectLevel::ReadOnly,
                risk_level: RiskLevel::SafeRead,
                requires_approval: false,
                is_run_terminator: false,
                input_hash,
                input_payload_ref: None,
                input_text: None,
            }))
            .await;
    }

    /// Payload-aware companion to `emit_tool_call_started`. Gives tool
    /// input the SAME plaintext path model-call prompts have: under the
    /// run's retention (`Redacted` / `FullDebug` only — NEVER
    /// `HashOnly`) the redacted-or-verbatim input is blob-written and
    /// the resulting `BlobRef` is wired onto `input_payload_ref`, while
    /// the plaintext is carried on `input_text` so the recorder writes
    /// a `tool_call_payload` side-row.
    ///
    /// `input_raw` is the RAW tool args (e.g. the serialized
    /// `tool_use.input` JSON). The redactor is applied INSIDE this
    /// method under `Redacted` — mirroring
    /// `emit_model_call_finished_with_payloads`, which takes the raw
    /// `LlmRequest` and redacts internally. The `input_hash` is still
    /// computed over the (pre-)redacted text so the hash contract
    /// matches `emit_tool_call_started`.
    ///
    /// Blob-write failure under `Redacted`/`FullDebug` logs at `error!`
    /// (supervisor channel) and drops the ref to `None` — the row still
    /// records, the operator sees the failure surface.
    pub async fn emit_tool_call_started_with_payload(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        tool_name: &str,
        input_raw: &str,
    ) {
        // Open the parent span row first so the tool_calls row has a FK
        // target — identical to `emit_tool_call_started`.
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            tool_name: Some(tool_name.to_string()),
            ..SpanAttributes::default()
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::ToolCall,
                name: tool_name.to_string(),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: attrs.to_attributes_json(),
            }))
            .await;

        // Retention-gated plaintext: redact under Redacted, verbatim
        // under FullDebug, suppressed otherwise. The hash is taken over
        // this same gated text so the row's `input_hash` matches the
        // stored body.
        let (input_text, input_payload_ref) = self
            .persist_tool_payload(span_id, input_raw, self.retention.store_prompts)
            .await;
        let hash_input = input_text.as_deref().unwrap_or(input_raw);
        let input_hash = format!("sha256:{}", hex::encode(Sha256::digest(hash_input.as_bytes())));

        self.bus
            .publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
                span_id: span_id.to_string(),
                tool_name: tool_name.to_string(),
                origin: ToolOrigin::Native,
                tool_version: None,
                tool_hash: None,
                side_effect_level: SideEffectLevel::ReadOnly,
                risk_level: RiskLevel::SafeRead,
                requires_approval: false,
                is_run_terminator: false,
                input_hash,
                input_payload_ref,
                input_text,
            }))
            .await;
    }

    /// Retention-gated blob persistence for a single tool payload
    /// (input or output). Returns `(plaintext_for_side_row,
    /// blob_payload_ref)`:
    ///
    /// - `HashOnly` (or any non-store mode) → `(None, None)`; no blob
    ///   write, no side-row.
    /// - `FullDebug` → verbatim bytes blob-written; `(Some(text),
    ///   Some(ref))`.
    /// - `Redacted` → `Redactor`-scrubbed bytes blob-written;
    ///   `(Some(scrubbed), Some(ref))`.
    ///
    /// Mirrors the prompt/response branch in
    /// `emit_model_call_finished_with_payloads`: applies
    /// `max_payload_bytes` before the `BlobStore::write`, and on write
    /// failure logs at `error!` and drops the ref to `None` while still
    /// returning the plaintext so the side-row records.
    async fn persist_tool_payload(
        &self,
        span_id: &str,
        raw: &str,
        store_flag: bool,
    ) -> (Option<String>, Option<String>) {
        let write = store_flag
            && matches!(
                self.retention.mode,
                RetentionMode::FullDebug | RetentionMode::Redacted
            );
        if !write {
            return (None, None);
        }
        let text: String = match self.retention.mode {
            RetentionMode::FullDebug => raw.to_string(),
            RetentionMode::Redacted => Redactor::new().redact(raw).text,
            // Unreachable — gated by `write` above; explicit for future
            // modes.
            _ => raw.to_string(),
        };
        let mut payload_ref: Option<String> = None;
        if let Some(store) = self.blob_store.as_ref() {
            let bytes = cap_blob_bytes(text.clone().into_bytes(), self.retention.max_payload_bytes);
            match store.write(&bytes) {
                Ok(blob_ref) => payload_ref = Some(blob_ref.as_str().to_string()),
                Err(e) => {
                    tracing::error!(
                        run_id = %self.run_id,
                        span_id = %span_id,
                        error = %e,
                        "BlobStore::write failed for tool payload — \
                         ref will be None, hash still recorded",
                    );
                }
            }
        }
        (Some(text), payload_ref)
    }

    /// Finish a tool call as success. Closes the matching `tool.call`
    /// span with `Ok` status and stamps the `output_hash` onto the
    /// `tool_calls` row. `output_redacted` MUST already be free of
    /// secrets — same producer-trusts-redactor contract as
    /// `emit_tool_call_started`.
    pub async fn emit_tool_call_finished(&self, span_id: &str, output_redacted: &str) {
        let output_hash = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(output_redacted.as_bytes()))
        );
        self.bus
            .publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
                span_id: span_id.to_string(),
                output_hash: Some(output_hash),
                output_payload_ref: None,
                exit_code: Some(0),
                output_text: None,
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: SpanStatus::Ok,
                error_json: None,
            }))
            .await;
    }

    /// Payload-aware companion to `emit_tool_call_finished`. Gives tool
    /// output the SAME plaintext path model-call responses have: under
    /// the run's retention (`Redacted` / `FullDebug` only — NEVER
    /// `HashOnly`) the redacted-or-verbatim output is blob-written, the
    /// ref is wired onto `output_payload_ref`, and the plaintext rides
    /// on `output_text` for the recorder's `tool_call_payload`
    /// side-row.
    ///
    /// `output_raw` is the RAW tool result; the redactor is applied
    /// INSIDE this method under `Redacted` (same contract as
    /// `emit_tool_call_started_with_payload`). The `output_hash` is
    /// computed over the gated text so it matches the stored body.
    pub async fn emit_tool_call_finished_with_payload(&self, span_id: &str, output_raw: &str) {
        let (output_text, output_payload_ref) = self
            .persist_tool_payload(span_id, output_raw, self.retention.store_responses)
            .await;
        let hash_input = output_text.as_deref().unwrap_or(output_raw);
        let output_hash = format!("sha256:{}", hex::encode(Sha256::digest(hash_input.as_bytes())));
        self.bus
            .publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
                span_id: span_id.to_string(),
                output_hash: Some(output_hash),
                output_payload_ref,
                exit_code: Some(0),
                output_text,
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: SpanStatus::Ok,
                error_json: None,
            }))
            .await;
    }

    /// Finish a tool call as failure. Closes the matching span with
    /// `Error` status and writes the error JSON. Caller's
    /// `error_message` is wrapped in `{"message": "..."}` to match
    /// SpanInspector's expected shape.
    pub async fn emit_tool_call_failed(&self, span_id: &str, error_message: &str) {
        let error_json = serde_json::json!({ "message": error_message }).to_string();
        self.bus
            .publish(RunEvent::ToolCallFailed(ToolCallFailedEvent {
                span_id: span_id.to_string(),
                error_json: Some(error_json.clone()),
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status: SpanStatus::Error,
                error_json: Some(error_json),
            }))
            .await;
    }

    /// Emit a `supervisor_notes` row directly. Broadens the F-7
    /// guardrail emit site to cover preflight warnings, broker-rule
    /// violations, cost-cap warnings, and flat-degeneracy skip notes
    /// per F43 § 3. The dashboard's "operator notes" surface renders
    /// these in order.
    ///
    /// `role` is producer-defined: `system` / `guard` / `planner` /
    /// `reviewer`. `severity` is `info` / `warn` / `error`. `content`
    /// MUST be redacted before passing in.
    pub async fn emit_supervisor_note(&self, role: &str, severity: &str, content: &str) {
        self.bus
            .publish(RunEvent::SupervisorNote(SupervisorNoteEvent {
                run_id: self.run_id.clone(),
                role: role.to_string(),
                content: content.to_string(),
                severity: severity.to_string(),
                created_at: Utc::now(),
            }))
            .await;
    }

    /// Emit a `recovery.attempt` span recording the harness's decision
    /// to retry around a typed failure. Instantaneous (open + close in
    /// the same tick) so the trace dock renders it as a point event
    /// rather than a duration. `class_tag` is the
    /// [`crate::agent::recovery::FailureClass::tag`] string (e.g.
    /// `invalid_json`, `repeated_tool_failure`) — the same wire tag
    /// persisted on `eval_runs.error`. `retry_count` is the attempt
    /// number (1 = first retry).
    ///
    /// Added by F-5 (`harness-recovery-state-machine`). The
    /// `SpanKind::RecoveryAttempt` variant was reserved by F-4 — F-5
    /// adds the emit method and the call sites.
    pub async fn emit_recovery_attempt(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        class_tag: &str,
        retry_count: u32,
    ) {
        self.emit_recovery_point_span(
            SpanKind::RecoveryAttempt,
            span_id,
            parent_span_id,
            class_tag,
            retry_count,
            None,
        )
        .await;
    }

    /// Emit a `recovery.attempt` span carrying a failed-recovery
    /// outcome. Same shape as [`Self::emit_recovery_attempt`] but with
    /// `SpanStatus::Error` and a `final_error` recorded so operators
    /// can read the typed reason in the trace dock without
    /// reconstructing it from prose.
    ///
    /// Convention: a single recovery cycle emits at most one
    /// `recovery.attempt` per try; on the terminal attempt that
    /// exhausts the budget, the caller uses this method instead so
    /// the trace marks the last attempt as the failure point.
    pub async fn emit_recovery_failed(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        class_tag: &str,
        retry_count: u32,
        final_error: &str,
    ) {
        self.emit_recovery_point_span(
            SpanKind::RecoveryAttempt,
            span_id,
            parent_span_id,
            class_tag,
            retry_count,
            Some(final_error),
        )
        .await;
    }

    /// Internal shared body for the recovery span emitters. Both emit
    /// a single `recovery.attempt` SpanKind — the `final_error`
    /// presence distinguishes attempted-vs-failed at the data layer
    /// (also reflected in `SpanStatus`).
    async fn emit_recovery_point_span(
        &self,
        kind: SpanKind,
        span_id: &str,
        parent_span_id: Option<String>,
        class_tag: &str,
        retry_count: u32,
        final_error: Option<&str>,
    ) {
        debug_assert!(
            matches!(kind, SpanKind::RecoveryAttempt),
            "emit_recovery_point_span called with non-recovery kind: {kind:?}"
        );
        // `retry_count` rendered into the typed bag as i32 (the
        // SpanAttributes wire shape). Clamp at i32::MAX defensively;
        // in practice retry counts here are <10.
        let typed_attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            retry_count: Some(retry_count.min(i32::MAX as u32) as i32),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        base.insert(
            "class_tag".to_string(),
            serde_json::Value::String(class_tag.to_string()),
        );
        if let Some(err) = final_error {
            base.insert(
                "final_error".to_string(),
                serde_json::Value::String(err.to_string()),
            );
        }
        let merged: String = typed_attrs.merge_into_object(base);
        let now = Utc::now();
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind,
                name: format!("recovery:{class_tag}#{retry_count}"),
                started_at: now,
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(merged),
            }))
            .await;
        let (status, error_json) = match final_error {
            Some(err) => (
                SpanStatus::Error,
                Some(serde_json::json!({ "class_tag": class_tag, "message": err }).to_string()),
            ),
            None => (SpanStatus::Ok, None),
        };
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: now,
                status,
                error_json,
            }))
            .await;
    }

    /// Emit an instantaneous `tool.validate_input` span recording the
    /// pre-condition of a tool call. The body is a no-op today — F-6
    /// (`harness-typed-mechanical-params`) will replace the no-op with
    /// the actual schema check while the span shape stays fixed.
    /// `parent_span_id` should be the enclosing `tool.call` span when
    /// one exists; the engine eval path does not emit `tool.call`
    /// spans today, in which case `None` is correct.
    ///
    /// Added by F-4 (`harness-span-taxonomy-extension`) as the
    /// instrumentation seam for F-6.
    pub async fn emit_tool_validate_input(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        tool_name: &str,
    ) {
        self.emit_tool_validate(SpanKind::ToolValidateInput, span_id, parent_span_id, tool_name)
            .await;
    }

    /// Emit an instantaneous `tool.validate_output` span recording the
    /// post-condition of a tool call. Emitted even when the tool
    /// errored so the post-state is always recorded; F-6 will use the
    /// span body to validate the actual response shape.
    ///
    /// Added by F-4 (`harness-span-taxonomy-extension`).
    pub async fn emit_tool_validate_output(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        tool_name: &str,
    ) {
        self.emit_tool_validate(SpanKind::ToolValidateOutput, span_id, parent_span_id, tool_name)
            .await;
    }

    /// Internal shared body for the two validate-bracket emitters.
    /// Both spans have identical shape; the variant differs only by
    /// `SpanKind`.
    async fn emit_tool_validate(
        &self,
        kind: SpanKind,
        span_id: &str,
        parent_span_id: Option<String>,
        tool_name: &str,
    ) {
        debug_assert!(
            matches!(kind, SpanKind::ToolValidateInput | SpanKind::ToolValidateOutput),
            "emit_tool_validate called with non-validate kind: {kind:?}"
        );
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            tool_name: Some(tool_name.to_string()),
            ..SpanAttributes::default()
        };
        let now = Utc::now();
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind,
                name: tool_name.to_string(),
                started_at: now,
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: attrs.to_attributes_json(),
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: now,
                status: SpanStatus::Ok,
                error_json: None,
            }))
            .await;
    }

    /// Open a `DecisionModel` span (`"decision.model"`) — the model
    /// invocation that produces the trade decision. Caller pairs this with
    /// exactly one `emit_span_finished_*` call carrying the same `span_id`.
    /// The backtest executor opens this as a CHILD of the enclosing
    /// `agent.decision` span (passing `decision_span_id` as
    /// `parent_span_id`) so the decision model call nests under the
    /// decision it produced.
    ///
    /// `stage` is the `LLMSlot.role` label ("regime" / "trader" or
    /// any free-form per-strategy name) so the trace dock
    /// can group spans by their pipeline role. Populates the
    /// `SpanAttributes` bag with `run_id`, `provider`, `model`, and
    /// the stage so F-7's planned Simple/Advanced toggle has the
    /// fields to triage on — see harness audit F-2.
    ///
    /// (The emit-helper name keeps the `model_call` prefix — only the
    /// `SpanKind` it sets was renamed from `ModelCall` to `DecisionModel`
    /// — to limit churn across the cost/hash/blob test surface.)
    pub async fn emit_model_call_started(
        &self,
        span_id: &str,
        parent_span_id: Option<String>,
        provider: &str,
        model: &str,
        stage: Option<&str>,
        name_override: Option<&str>,
        extra_attrs: Option<&serde_json::Value>,
    ) {
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            stage: stage.map(|s| s.to_string()),
            model: Some(model.to_string()),
            provider: Some(provider.to_string()),
            ..SpanAttributes::default()
        };
        let attributes_json = match extra_attrs {
            Some(serde_json::Value::Object(map)) => attrs.merge_into_object(map.clone()),
            Some(other) => {
                let mut map = serde_json::Map::new();
                map.insert("context".to_string(), other.clone());
                attrs.merge_into_object(map)
            }
            None => attrs.to_attributes_json().unwrap_or_else(|| "{}".to_string()),
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::DecisionModel,
                name: name_override
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| format!("{provider}/{model}")),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(attributes_json),
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
    ///
    /// `prompt_hash` and `response_hash` are caller-computed via
    /// `compute_prompt_hash` / `compute_response_hash`. The work
    /// happens at the call site because `LlmRequest` is consumed by
    /// `dispatch.complete(req)` and the assistant text accumulation
    /// already exists there — hashing here would require either an
    /// extra clone or routing the request back through the emitter.
    /// `response_hash` is `None` for tool-use-only turns (no assistant
    /// text).
    #[allow(clippy::too_many_arguments)]
    pub async fn emit_model_call_finished(
        &self,
        span_id: &str,
        provider: &str,
        model: &str,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        cost_usd: Option<f64>,
        prompt_hash: String,
        response_hash: Option<String>,
    ) {
        // Resolve cost: caller-supplied wins (preserves provider-side
        // out-of-band cost paths for Anthropic / bare OpenAI), then
        // fall back to catalog-based math. The historical call sites
        // all pass `None`, so this is the seam where the previously
        // all-NULL `model_calls.cost_usd` column starts populating.
        let resolved_cost = cost_usd.or_else(|| match (input_tokens, output_tokens) {
            (Some(i), Some(o)) => self.compute_cost_usd(provider, model, i as u64, o as u64),
            _ => None,
        });
        self.bus
            .publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
                span_id: span_id.to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                input_token_count: input_tokens.map(i64::from),
                output_token_count: output_tokens.map(i64::from),
                cost_usd: resolved_cost,
                prompt_hash,
                response_hash,
                prompt_text: None,
                response_text: None,
                prompt_payload_ref: None,
                response_payload_ref: None,
                tool_calls_requested: None,
                capability_path: None,
            }))
            .await;
    }

    /// Payload-aware companion to `emit_model_call_finished`. Persists
    /// the prompt request body and the assistant text into the
    /// configured `BlobStore` per the active retention policy, then
    /// publishes `ModelCallFinishedEvent` with the resulting
    /// `BlobRef`s wired onto `prompt_payload_ref` /
    /// `response_payload_ref`.
    ///
    /// Retention semantics:
    /// - `FullDebug` + `store_prompts`/`store_responses` → bytes written
    ///   verbatim, refs populated.
    /// - `Redacted` → bytes pass through `Redactor::redact` before
    ///   write; refs populated with the post-redaction blob.
    /// - `HashOnly` (or any other mode) → no write, refs stay `None`.
    ///
    /// If no `BlobStore` is attached, behaves identically to the
    /// `emit_model_call_finished` shim and publishes `None` refs.
    ///
    /// `BlobStore::write` is fallible. On failure under FullDebug or
    /// Redacted retention the emitter logs at `error!` level
    /// (supervisor channel) and publishes the event with the ref
    /// dropped to `None` — the run still records, the operator sees
    /// the failure. This is intentional `feedback_alpha_root_cause`
    /// behaviour: the failure surfaces, it does not get swallowed.
    #[allow(clippy::too_many_arguments)]
    pub async fn emit_model_call_finished_with_payloads(
        &self,
        span_id: &str,
        provider: &str,
        model: &str,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        cost_usd: Option<f64>,
        prompt_hash: String,
        response_hash: Option<String>,
        prompt_request: Option<&LlmRequest>,
        response_text: Option<&str>,
    ) {
        let mut prompt_payload_ref: Option<String> = None;
        let mut response_payload_ref: Option<String> = None;

        let write_prompts = self.retention.store_prompts
            && matches!(
                self.retention.mode,
                RetentionMode::FullDebug | RetentionMode::Redacted
            );
        let write_responses = self.retention.store_responses
            && matches!(
                self.retention.mode,
                RetentionMode::FullDebug | RetentionMode::Redacted
            );

        if let Some(store) = self.blob_store.as_ref() {
            if write_prompts {
                if let Some(req) = prompt_request {
                    // Persist the full LlmRequest (model, tools,
                    // response_schema, sampling), not just the hash
                    // input — see canonical_request_bytes docstring.
                    let bytes = canonical_request_bytes(req);
                    let bytes = match self.retention.mode {
                        RetentionMode::FullDebug => bytes,
                        RetentionMode::Redacted => {
                            let as_str = String::from_utf8_lossy(&bytes);
                            Redactor::new().redact(as_str.as_ref()).text.into_bytes()
                        }
                        // Already gated by `write_prompts`; unreachable
                        // is fine but explicit fall-through is
                        // clearer for future modes.
                        _ => bytes,
                    };
                    // Apply max_payload_bytes BEFORE the BlobStore
                    // write, mirroring the body path's apply_to_body
                    // cap. Without this the janitor would still
                    // truncate eventually, but a large prompt would
                    // live full-size until cleanup.
                    let bytes = cap_blob_bytes(bytes, self.retention.max_payload_bytes);
                    match store.write(&bytes) {
                        Ok(blob_ref) => {
                            prompt_payload_ref = Some(blob_ref.as_str().to_string());
                        }
                        Err(e) => {
                            tracing::error!(
                                run_id = %self.run_id,
                                span_id = %span_id,
                                error = %e,
                                "BlobStore::write failed for prompt payload — \
                                 ref will be None, hash still recorded",
                            );
                        }
                    }
                }
            }

            if write_responses {
                if let Some(text) = response_text {
                    let text_owned: String = match self.retention.mode {
                        RetentionMode::FullDebug => text.to_string(),
                        RetentionMode::Redacted => Redactor::new().redact(text).text,
                        _ => text.to_string(),
                    };
                    let bytes = cap_blob_bytes(text_owned.into_bytes(), self.retention.max_payload_bytes);
                    match store.write(&bytes) {
                        Ok(blob_ref) => {
                            response_payload_ref = Some(blob_ref.as_str().to_string());
                        }
                        Err(e) => {
                            tracing::error!(
                                run_id = %self.run_id,
                                span_id = %span_id,
                                error = %e,
                                "BlobStore::write failed for response payload — \
                                 ref will be None, hash still recorded",
                            );
                        }
                    }
                }
            }
        }

        // Same resolution rule as `emit_model_call_finished` — see
        // that method's comment. Catalog lookup is best-effort; an
        // unpriced model leaves `cost_usd = None` and the unpriced-pair
        // dedupe ensures the operator sees one debug line per pair.
        let resolved_cost = cost_usd.or_else(|| match (input_tokens, output_tokens) {
            (Some(i), Some(o)) => self.compute_cost_usd(provider, model, i as u64, o as u64),
            _ => None,
        });
        self.bus
            .publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
                span_id: span_id.to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                input_token_count: input_tokens.map(i64::from),
                output_token_count: output_tokens.map(i64::from),
                cost_usd: resolved_cost,
                prompt_hash,
                response_hash,
                prompt_text: None,
                response_text: None,
                prompt_payload_ref,
                response_payload_ref,
                tool_calls_requested: None,
                capability_path: None,
            }))
            .await;
    }

    /// Emit a `decision.reasoning` span carrying the inline chain-of-thought
    /// captured from a model's `<think>…</think>` block (WS-17). The span
    /// is opened and immediately closed (`Ok`) as a CHILD of the enclosing
    /// `decision.model` span (`parent_span_id`) so the trace nests the
    /// reasoning under the decision-model call that produced it. When no
    /// `decision.model` span id is available the span is emitted top-level
    /// (`parent_span_id = None`); the chain-of-thought still reaches the
    /// trace.
    ///
    /// Retention (mirrors the model-call payload gate in
    /// `emit_model_call_finished_with_payloads`):
    /// - `reasoning_char_count` is ALWAYS recorded (cost legibility, no
    ///   raw-body leak).
    /// - `full_debug`/`redacted` + a `BlobStore` + `store_responses` →
    ///   the reasoning body is written to a blob (redacted first under
    ///   `redacted`), and `reasoning_blob_ref` points at it.
    /// - `full_debug` + `store_responses` additionally inlines a bounded
    ///   `reasoning_text` on the span attributes (same gate as
    ///   `emit_assistant_text_delta` via `apply_to_body`).
    /// - `hash_only` (or any non-debug mode) → NO blob write, NO inline
    ///   body: only the char count survives.
    pub async fn emit_model_reasoning(&self, parent_span_id: Option<String>, reasoning_text: &str) {
        let span_id = fresh_span_id();
        let char_count = reasoning_text.chars().count();

        // Payload gate — identical predicate to the model-call response
        // blob write: store the body only under FullDebug | Redacted with
        // store_responses set, NEVER under HashOnly.
        let write_body = self.retention.store_responses
            && matches!(
                self.retention.mode,
                RetentionMode::FullDebug | RetentionMode::Redacted
            );

        let mut reasoning_blob_ref: Option<String> = None;
        if write_body {
            if let Some(store) = self.blob_store.as_ref() {
                let body: String = match self.retention.mode {
                    RetentionMode::FullDebug => reasoning_text.to_string(),
                    RetentionMode::Redacted => Redactor::new().redact(reasoning_text).text,
                    // Already gated by `write_body`; explicit fall-through.
                    _ => reasoning_text.to_string(),
                };
                let bytes = cap_blob_bytes(body.into_bytes(), self.retention.max_payload_bytes);
                match store.write(&bytes) {
                    Ok(blob_ref) => reasoning_blob_ref = Some(blob_ref.as_str().to_string()),
                    Err(e) => {
                        tracing::error!(
                            run_id = %self.run_id,
                            span_id = %span_id,
                            error = %e,
                            "BlobStore::write failed for reasoning payload — \
                             ref will be None, char count still recorded",
                        );
                    }
                }
            }
        }

        // Inline body only under the same gate the assistant-text delta
        // uses (FullDebug + store_responses). `apply_to_body` returns ""
        // when emission is disallowed, so under hash_only/redacted no raw
        // reasoning text rides on the span attributes.
        let inline_body = self.retention.apply_to_body(reasoning_text);

        let mut attrs = serde_json::Map::new();
        attrs.insert("reasoning_char_count".to_string(), serde_json::json!(char_count));
        if let Some(blob_ref) = reasoning_blob_ref.as_ref() {
            attrs.insert("reasoning_blob_ref".to_string(), serde_json::json!(blob_ref));
        }
        if !inline_body.is_empty() {
            attrs.insert("reasoning_text".to_string(), serde_json::json!(inline_body));
        }
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            ..SpanAttributes::default()
        }
        .merge_into_object(attrs);

        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.clone(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::DecisionReasoning,
                name: "decision.reasoning".to_string(),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(attrs),
            }))
            .await;
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id,
                ended_at: Utc::now(),
                status: SpanStatus::Ok,
                error_json: None,
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
    /// memory-provenance-in-decisions-trace: publish a V2D
    /// `memory_recall` event bound to the per-decision identifier this
    /// recall fed into. The recorder persists it into the `events`
    /// table (no schema migration — the table already accepts arbitrary
    /// `(kind, payload_json)` rows) so the dashboard's per-decision
    /// recall list can answer "which memories drove decision N."
    ///
    /// `matches` mirrors `xvision_memory::types::MemoryMatch`. The full
    /// item text body is NOT carried on the event — only the first ~160
    /// chars as `text_preview` — so the bus payload stays small. The
    /// `id` lets the dashboard deep-link back to the memory store.
    pub async fn emit_memory_recall(
        &self,
        decision_id: i64,
        namespace: &str,
        matches: &[xvision_memory::types::MemoryMatch],
    ) {
        let items: Vec<MemoryRecallItem> = matches
            .iter()
            .map(|m| MemoryRecallItem {
                id: m.id.clone(),
                score: m.score,
                text_preview: preview_text(&m.text),
            })
            .collect();
        self.bus
            .publish(RunEvent::MemoryRecall(MemoryRecallEvent {
                run_id: self.run_id.clone(),
                flywheel_cycle_id: Some(format!("{}:{decision_id}", self.run_id)),
                decision_id,
                namespace: namespace.to_string(),
                items,
            }))
            .await;
    }

    pub async fn emit_memory_write(
        &self,
        decision_id: i64,
        namespace: &str,
        memory_item_id: &str,
        text: &str,
    ) {
        self.bus
            .publish(RunEvent::MemoryWrite(MemoryWriteEvent {
                run_id: self.run_id.clone(),
                flywheel_cycle_id: Some(format!("{}:{decision_id}", self.run_id)),
                decision_id,
                namespace: namespace.to_string(),
                memory_item_id: memory_item_id.to_string(),
                text_preview: preview_text(text),
            }))
            .await;
    }

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
        // Broker spans carry both the qa-trace-broker-spans
        // `broker_call` sub-object and the F-2 typed `SpanAttributes`
        // bag in the same flat object. `merge_into_object` writes the
        // typed fields at the top level and preserves `broker_call`
        // verbatim — existing dashboard projection of `broker_call`
        // continues to work.
        let typed_attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        base.insert(
            "broker_call".to_string(),
            serde_json::json!({
                "side": side,
                "symbol": symbol,
                "qty": qty,
                "intended_price": intended_price,
                "order_type": order_type,
                "venue": venue,
                "idempotency_key": idempotency_key,
            }),
        );
        let started_attrs = typed_attrs.merge_into_object(base);
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
                attributes_json: Some(started_attrs),
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

    /// Open a `risk.gate` span around one `RiskLayer::evaluate` call.
    /// Pair with exactly one `emit_risk_gate_finished` using the same
    /// `span_id`. `parent_span_id` should be the enclosing
    /// `agent.decision` span when one exists.
    pub async fn emit_risk_gate_started(&self, span_id: &str, parent_span_id: Option<String>) {
        let attrs = SpanAttributes {
            run_id: Some(self.run_id.clone()),
            ..SpanAttributes::default()
        };
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::RiskGate,
                name: "risk.gate".to_string(),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: attrs.to_attributes_json(),
            }))
            .await;
    }

    /// Close the `risk.gate` span opened by `emit_risk_gate_started`.
    /// `verdict` is "approved" / "modified" / "vetoed". `veto_reason`
    /// is the `VetoReason` debug string for vetoed decisions.
    /// `modified_qty` is the size_bps of the modified decision as f64.
    pub async fn emit_risk_gate_finished(
        &self,
        span_id: &str,
        verdict: &str,
        veto_reason: Option<&str>,
        modified_qty: Option<f64>,
    ) {
        debug_assert!(
            matches!(verdict, "approved" | "modified" | "vetoed"),
            "unexpected risk gate verdict: {verdict}"
        );
        let status = if verdict == "vetoed" {
            SpanStatus::Error
        } else {
            SpanStatus::Ok
        };
        let mut payload = serde_json::Map::new();
        payload.insert(
            "verdict".to_string(),
            serde_json::Value::String(verdict.to_string()),
        );
        if let Some(r) = veto_reason {
            payload.insert(
                "veto_reason".to_string(),
                serde_json::Value::String(r.to_string()),
            );
        }
        if let Some(qty) = modified_qty {
            if let Some(n) = serde_json::Number::from_f64(qty) {
                payload.insert("modified_qty".to_string(), serde_json::Value::Number(n));
            }
        }
        // WS-13 (`trace-obs-risk-gate`): carry the verdict payload for any
        // verdict that CHANGED the trader's action — `vetoed` (status
        // error) AND `modified` (status ok). `approved` stays clean (no
        // payload) so the persisted span row distinguishes "risk ran and
        // touched nothing" from "risk ran and rewrote the action", while
        // still matching the sibling `filter.eval` convention that a
        // clean pass carries no error payload.
        let error_json = if verdict == "approved" {
            None
        } else {
            Some(serde_json::Value::Object(payload).to_string())
        };
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                ended_at: Utc::now(),
                status,
                error_json,
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
            BrokerCallOutcome::Rejected | BrokerCallOutcome::Cancelled | BrokerCallOutcome::Failed => {
                SpanStatus::Error
            }
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

    /// Open a `filter.eval` span around one LLM filter-capability dispatch.
    /// Pair with `emit_filter_eval_finished` using the same `span_id`.
    pub async fn emit_filter_eval_started(&self, span_id: &str, parent_span_id: Option<String>, asset: &str) {
        self.bus
            .publish(RunEvent::SpanStarted(SpanStartedEvent {
                span_id: span_id.to_string(),
                run_id: self.run_id.clone(),
                parent_span_id,
                kind: SpanKind::FilterEval,
                name: format!("filter.eval {asset}"),
                started_at: Utc::now(),
                otel_trace_id: None,
                otel_span_id: None,
                attributes_json: Some(serde_json::json!({ "asset": asset }).to_string()),
            }))
            .await;
    }

    /// Close a `filter.eval` span. `verdict` is `"pass"` or `"reject"`.
    pub async fn emit_filter_eval_finished(&self, span_id: &str, verdict: &str, reason: Option<&str>) {
        let error_json = if verdict != "pass" {
            Some(serde_json::json!({ "verdict": verdict, "reason": reason }).to_string())
        } else {
            None
        };
        self.bus
            .publish(RunEvent::SpanFinished(SpanFinishedEvent {
                span_id: span_id.to_string(),
                status: if verdict == "pass" {
                    SpanStatus::Ok
                } else {
                    SpanStatus::Error
                },
                ended_at: Utc::now(),
                error_json,
            }))
            .await;
    }
}

/// Emit a `tracing::debug!` line for an unpriced `(provider, model)`
/// pair at most once per process. The dedupe lives in
/// [`unpriced_seen`]; a tight inner loop on the same model will not
/// flood the log.
fn log_unpriced_once(provider: &str, model: &str) {
    let key = (provider.to_string(), model.to_string());
    let mut guard = match unpriced_seen().lock() {
        Ok(g) => g,
        // Lock poisoning is recoverable: if a prior task panicked
        // while inserting, we'd rather log again than swallow the
        // signal. Treat as "first time" for this caller.
        Err(p) => p.into_inner(),
    };
    if guard.insert(key) {
        tracing::debug!(
            provider = %provider,
            model = %model,
            "model_calls.cost_usd: no priced catalog entry for this (provider, model) pair; \
             cost left NULL. Refresh the provider catalog (`xvn settings providers refresh`) \
             if you expect pricing to be available.",
        );
    }
}

/// Generate a fresh span id. ULID-shaped, time-prefixed so spans sort
/// chronologically without an explicit timestamp join. Lives next to
/// the emitter so callers don't grow an extra `ulid` import.
pub fn fresh_span_id() -> String {
    ulid::Ulid::new().to_string()
}

/// memory-provenance-in-decisions-trace: truncate to ~160 chars with an
/// ellipsis when trimmed. Mirrors `memory_recorder::preview` so the
/// `text_preview` carried on `memory_recall` events matches what the
/// `<prior_observations>` system-prompt block already shows the model.
/// Local copy keeps `observability.rs` free of cross-module re-exports.
fn preview_text(text: &str) -> String {
    let mut s: String = text.chars().take(160).collect();
    if text.chars().count() > 160 {
        s.push('…');
    }
    s
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
        assert!(
            out.starts_with("abcdefghij"),
            "expected first 10 bytes, got: {out:?}"
        );
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
