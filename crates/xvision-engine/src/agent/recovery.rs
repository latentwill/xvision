//! Typed recovery dispatcher for the agent harness. F-5 of the 2026-05-18
//! harness observability audit (`team/intake/archive/2026-05-18-harness-observability-audit.md`).
//!
//! Replaces the regex-on-error-string post-hoc classifier
//! (`eval::executor::classify_run_failure`) with a typed front door.
//! The string classifier stays in place as the residual fallback inside
//! [`classify`], so consumers that match on the `&'static str` tag keep
//! working unchanged — see [`FailureClass::tag`].
//!
//! ## Scope of this module
//!
//! Phase 1 (this contract): typed enum + classifier + repeated-tool
//! block list + span emit seam for `recovery.attempt` / `recovery.failed`
//! (added on `ObsEmitter`).
//!
//! Phase 2 (future tracks): per-class recovery *policies* — repair-prompt
//! the model on `MalformedJson`, targeted patch on `SchemaMissingField`,
//! cheap-model summarize on `ContextOverflow`. Today these all map to
//! [`RecoveryFamily::Unrecoverable`] or delegate to existing seams; the
//! seam is in place so a follow-up can wire one policy at a time without
//! re-touching this surface.

use std::collections::HashMap;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, OpenAiCompatError, ResponseSchema,
};
use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::eval::executor::trader_output::{merge_and_reparse_trader_output, TraderOutput};
use crate::eval::executor::{TraderFailureKind, TraderOutputError};

/// Stable failure class. Each variant maps to exactly one of the wire
/// tags the eval surface persists on `eval_runs.error` as
/// `[<tag>] <message>`. The set is the union of every class the
/// pre-F-5 string classifier ever returned, so [`FailureClass::tag`]
/// is a drop-in replacement for the previous `&'static str` return
/// value.
///
/// The audit's seven high-level "classes" (MalformedJson, ToolTimeout,
/// SchemaMissingField, EmptyData, ContextOverflow, RepeatedToolFailure,
/// Unrecoverable) live on [`RecoveryFamily`] instead — they describe
/// *what to do*, not *what happened*. The `family()` mapping translates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureClass {
    // ─── Trader-output (model emitted text we couldn't use) ──────────────
    TraderEmpty,
    TraderToolUseOnly,
    TraderTruncated,
    TraderInvalidJson,
    TraderMissingField,
    TraderInvalidField,
    TraderMissingResponse,

    // ─── Provider transport (dispatcher reached the wire but failed) ─────
    ProviderTimeout,
    ProviderConnect,
    ProviderHttpError,
    ProviderDecode,
    ProviderRateLimited,
    ProviderMissingChoices,

    // ─── Broker transport (owned by agent-error-feedback-self-healing) ───
    BrokerAuth,
    BrokerUnsupported,
    BrokerInsufficientFunds,
    BrokerTimeout,
    BrokerRejected,

    // ─── Provider context overflow (F-5 phase-2c) ────────────────────────
    /// F-5 phase-2c introduces this. The provider returned a 400 with a
    /// body indicating the conversation history exceeded the model's
    /// context window (Anthropic: `prompt is too long` /
    /// `context_length_exceeded`; OpenAI-compat: `context_length_exceeded`).
    /// `crate::agent::execute::execute_slot` consumes this via
    /// [`crate::agent::summarize::summarize_history`] to compress the
    /// transcript and retry once. The `message` and `provider` fields
    /// are surfaced on the `recovery.attempt` span so operators can
    /// debug overflow patterns without re-parsing prose.
    ContextOverflow {
        message: String,
        provider: String,
    },

    // ─── Loop control ────────────────────────────────────────────────────
    RepeatedBrokerError,
    /// F-5 introduces this. The agent tool-use loop in
    /// [`crate::agent::execute`] tripped the [`RepeatedToolFailureTracker`]
    /// block-list — same `(tool_name, input_hash)` pair failed more than
    /// [`MAX_TOOL_RETRIES_PER_PAIR`] times in one slot execution. Surfaces
    /// the offending tool and the hash so traces are debuggable.
    RepeatedToolFailure {
        tool_name: String,
        input_hash: String,
    },

    /// QA30: Cline per-step wall / token budget was exceeded
    /// (`budget_wall_ms_exceeded`, `budget_input_tokens_exceeded`,
    /// `budget_output_tokens_exceeded`). This is a clean stop, not a
    /// failure — the agent was still alive, the harness pulled the
    /// plug. The `kind` field carries the sidecar-side reason code
    /// (the `budget_*_exceeded` string) so a future policy can branch
    /// on which dimension tripped.
    BudgetExceeded {
        kind: String,
    },

    /// The Cline run completed with `status="completed"` but the agent never
    /// called `submit_decision`. The no-decision recovery policy issues a
    /// single repair step prompt before surfacing the hard failure.
    NoDecision,

    Unclassified,
}

impl FailureClass {
    /// Wire-stable tag persisted as the `[<tag>]` prefix on
    /// `eval_runs.error`. Adding a variant means adding a tag here; the
    /// adapter in `eval::executor::classify_run_failure` calls this so
    /// the wire shape is preserved across the F-5 cutover.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::TraderEmpty => "empty",
            Self::TraderToolUseOnly => "tool_use_only",
            Self::TraderTruncated => "truncated",
            Self::TraderInvalidJson => "invalid_json",
            Self::TraderMissingField => "missing_field",
            Self::TraderInvalidField => "invalid_field",
            Self::TraderMissingResponse => "missing_response",
            Self::ProviderTimeout => "provider_timeout",
            Self::ProviderConnect => "provider_connect",
            Self::ProviderHttpError => "provider_http_error",
            Self::ProviderDecode => "provider_decode",
            Self::ProviderRateLimited => "provider_rate_limited",
            Self::ProviderMissingChoices => "provider_missing_choices",
            Self::ContextOverflow { .. } => "context_overflow",
            Self::BrokerAuth => "broker_auth",
            Self::BrokerUnsupported => "broker_unsupported",
            Self::BrokerInsufficientFunds => "broker_insufficient_funds",
            Self::BrokerTimeout => "broker_timeout",
            Self::BrokerRejected => "broker_rejected",
            Self::RepeatedBrokerError => "repeated_broker_error",
            Self::RepeatedToolFailure { .. } => "repeated_tool_failure",
            Self::BudgetExceeded { .. } => "budget_exceeded",
            Self::NoDecision => "no_decision",
            Self::Unclassified => "unclassified",
        }
    }

    /// High-level recovery grouping per the audit's seven-class
    /// taxonomy. Drives policy dispatch in phase 2. Phase 1 only uses
    /// it for span attributes and tests — every variant currently maps
    /// to either [`RecoveryFamily::RepeatedToolFailure`] (handled by
    /// this module) or [`RecoveryFamily::Unrecoverable`] (handled by
    /// the existing path).
    pub fn family(&self) -> RecoveryFamily {
        match self {
            // MalformedJson family: model emitted text that isn't valid JSON,
            // OR the JSON was truncated.
            Self::TraderInvalidJson | Self::TraderTruncated => RecoveryFamily::MalformedJson,

            // SchemaMissingField family: JSON parsed but didn't conform.
            Self::TraderMissingField | Self::TraderInvalidField => RecoveryFamily::SchemaMissingField,

            // EmptyData family: the response slot was vacant.
            Self::TraderEmpty | Self::TraderToolUseOnly | Self::TraderMissingResponse => {
                RecoveryFamily::EmptyData
            }

            // ToolTimeout family: connection-level transport failure that
            // a retry might fix. The dispatcher's per-class budget
            // already attempts retries; F-5 reserves the family so a
            // future policy can promote it to a re-call with backoff.
            Self::ProviderTimeout | Self::ProviderConnect => RecoveryFamily::ToolTimeout,

            // ContextOverflow family (F-5 phase-2c): provider 400 with a
            // body indicating the conversation history exceeded the
            // model's context window. Policy: summarize prior history
            // through a cheap-model dispatch and retry once.
            Self::ContextOverflow { .. } => RecoveryFamily::ContextOverflow,

            // RepeatedToolFailure family: deterministic loop-control.
            Self::RepeatedBrokerError | Self::RepeatedToolFailure { .. } => {
                RecoveryFamily::RepeatedToolFailure
            }

            // NoDecision: Cline run ended without submit_decision call.
            Self::NoDecision => RecoveryFamily::NoDecision,

            // Everything else is unrecoverable from this module's
            // vantage point: provider HTTP/decode/rate-limit are
            // already retried inside the dispatcher; broker errors are
            // owned by `agent-error-feedback-self-healing`; unclassified
            // means we have no signal worth acting on. Budget-exceeded
            // is intentionally a clean stop, not a recoverable error —
            // bumping the budget is an operator action, not a runtime
            // retry decision.
            Self::ProviderHttpError
            | Self::ProviderDecode
            | Self::ProviderRateLimited
            | Self::ProviderMissingChoices
            | Self::BrokerAuth
            | Self::BrokerUnsupported
            | Self::BrokerInsufficientFunds
            | Self::BrokerTimeout
            | Self::BrokerRejected
            | Self::BudgetExceeded { .. }
            | Self::Unclassified => RecoveryFamily::Unrecoverable,
        }
    }
}

/// High-level recovery grouping. The seven audit classes map onto
/// these variants; each one names a *kind of recovery* rather than a
/// *kind of failure*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryFamily {
    /// Model emitted text that wouldn't parse. Phase-2 policy:
    /// repair-prompt with the parse error injected, once.
    MalformedJson,
    /// Transport-level wire failure. Phase-2 policy: re-call with
    /// exponential backoff, capped budget. Dispatcher already covers
    /// this for most provider variants — the family is reserved so a
    /// future policy can extend.
    ToolTimeout,
    /// JSON parsed but required fields are missing. Phase-2 policy:
    /// targeted patch prompt asking only for the missing fields, once.
    SchemaMissingField,
    /// Response slot was vacant. Phase-2 policy: emit
    /// `data_availability_failure` and stop the cycle.
    EmptyData,
    /// History exceeded the model's context window. Phase-2 policy:
    /// cheap-model history summarize, retry once with a hard summarize
    /// budget. No current classifier evidence triggers this variant —
    /// reserved.
    ContextOverflow,
    /// Tool-loop control: same `(tool_name, input_hash)` pair failed
    /// more than [`MAX_TOOL_RETRIES_PER_PAIR`] times in one slot. The
    /// pipeline blocks the pair for the rest of the slot. Live in
    /// phase 1.
    RepeatedToolFailure,
    /// Cline run completed without a `submit_decision` call. Policy:
    /// issue a single repair step prompt; if the model still doesn't
    /// call the tool, fail the cycle visibly.
    NoDecision,
    /// No bounded recovery applies. The caller surfaces the underlying
    /// error.
    Unrecoverable,
}

/// Walks the `anyhow::Error` source chain to classify a run-level
/// failure. Mirrors the pre-F-5 logic in
/// `eval::executor::classify_run_failure`: typed downcasts first,
/// then a string-matcher fallback. This module owns the typed
/// dispatch; the eval executor's `classify_run_failure` is now a thin
/// adapter that calls [`classify`].`tag()`.
///
/// Adding a new typed error path: add a `downcast_ref` arm before the
/// string fallback. Adding a new string pattern: extend the
/// [`classify_from_string`] helper at the bottom of this module.
pub fn classify(err: &anyhow::Error) -> FailureClass {
    // Typed: trader output failures first — they have the richest
    // diagnostic and are the most common recoverable family.
    if let Some(te) = err.downcast_ref::<TraderOutputError>() {
        return from_trader_kind(te.kind);
    }
    // Typed: provider dispatcher's OpenAI-compat surface.
    for cause in err.chain() {
        if let Some(typed) = cause.downcast_ref::<OpenAiCompatError>() {
            return match typed {
                OpenAiCompatError::RateLimited { .. } => FailureClass::ProviderRateLimited,
                OpenAiCompatError::MissingChoicesArray { .. } => FailureClass::ProviderMissingChoices,
                OpenAiCompatError::ContextOverflow { body, provider, .. } => FailureClass::ContextOverflow {
                    message: body.clone(),
                    provider: provider.clone(),
                },
                OpenAiCompatError::ResponseFormatUnsupported { .. } => FailureClass::ProviderHttpError,
            };
        }
    }
    // String fallback. The trader-output Display may have been wrapped
    // with `.context(...)` so a downcast misses; check the formatted
    // chain for the `trader_output[<tag>]` form first.
    let formatted = format!("{:#}", err).to_lowercase();
    if let Some(kind) = trader_kind_from_formatted(&formatted) {
        return from_trader_kind(kind);
    }
    classify_from_string(&formatted)
}

fn from_trader_kind(k: TraderFailureKind) -> FailureClass {
    match k {
        TraderFailureKind::EmptyText => FailureClass::TraderEmpty,
        TraderFailureKind::ToolUseOnly => FailureClass::TraderToolUseOnly,
        TraderFailureKind::Truncated => FailureClass::TraderTruncated,
        TraderFailureKind::InvalidJson => FailureClass::TraderInvalidJson,
        TraderFailureKind::MissingField => FailureClass::TraderMissingField,
        TraderFailureKind::InvalidField => FailureClass::TraderInvalidField,
        TraderFailureKind::MissingResponse => FailureClass::TraderMissingResponse,
    }
}

fn trader_kind_from_formatted(s: &str) -> Option<TraderFailureKind> {
    for k in [
        TraderFailureKind::EmptyText,
        TraderFailureKind::ToolUseOnly,
        TraderFailureKind::Truncated,
        TraderFailureKind::InvalidJson,
        TraderFailureKind::MissingField,
        TraderFailureKind::InvalidField,
        TraderFailureKind::MissingResponse,
    ] {
        let needle = format!("trader_output[{}]", k.tag());
        if s.contains(&needle) {
            return Some(k);
        }
    }
    None
}

/// String-matcher fallback. Preserves the residual coverage from the
/// pre-F-5 `classify_run_failure` — broker patterns, provider transport
/// patterns, repeated_broker_error. The arm order matters: more
/// specific patterns first, so a broker fill timeout doesn't get
/// re-tagged as `provider_timeout`.
fn classify_from_string(s: &str) -> FailureClass {
    // Cline no-decision: match before any broker/timeout fallbacks.
    if s.contains("run completed without calling submit_decision") {
        return FailureClass::NoDecision;
    }
    // Loop-control class — match BEFORE broker fallbacks so the abort
    // message (which embeds e.g. `broker_min_order_size`) doesn't get
    // re-classified.
    if s.contains("repeated_broker_error") {
        return FailureClass::RepeatedBrokerError;
    }
    // QA30: Cline per-step budget aborts. Surface them as a clean
    // `[budget_exceeded]` typed class instead of falling through to
    // the generic `timeout` arm (which would mask the real cause as
    // a transient network blip) or `unclassified` (which is what the
    // user saw at the start of this round). Match BEFORE the broker
    // and timeout patterns so the embedded `_ms_` / `_tokens_` keys
    // route here.
    if s.contains("budget_wall_ms_exceeded") {
        return FailureClass::BudgetExceeded {
            kind: "wall_ms".to_string(),
        };
    }
    if s.contains("budget_input_tokens_exceeded") {
        return FailureClass::BudgetExceeded {
            kind: "input_tokens".to_string(),
        };
    }
    if s.contains("budget_output_tokens_exceeded") {
        return FailureClass::BudgetExceeded {
            kind: "output_tokens".to_string(),
        };
    }
    // Context-overflow (F-5 phase-2c). Check before the broker / generic
    // provider patterns so an embedded provider-name phrase doesn't
    // shadow the more specific overflow class. The matchers mirror
    // `body_indicates_context_overflow` in `llm.rs`.
    if s.contains("context_length_exceeded")
        || s.contains("context length exceeded")
        || s.contains("context window")
        || s.contains("prompt is too long")
        || s.contains("max_tokens exceeded")
    {
        return FailureClass::ContextOverflow {
            message: s.to_string(),
            provider: "unknown".to_string(),
        };
    }
    // Broker classes — match before the generic `timeout` fallback so a
    // broker-side fill timeout doesn't get tagged `provider_timeout`.
    if s.contains("broker_unsupported")
        || s.contains("not shortable")
        || s.contains("asset is not shortable")
        || (s.contains("bracket") && s.contains("not supported"))
        || s.contains("not supported for this asset class")
    {
        return FailureClass::BrokerUnsupported;
    }
    if s.contains("insufficient buying power")
        || s.contains("insufficient balance")
        || s.contains("insufficient funds")
    {
        return FailureClass::BrokerInsufficientFunds;
    }
    if s.contains("not permitted") || s.contains("forbidden") {
        return FailureClass::BrokerAuth;
    }
    if s.contains("alpaca order") && s.contains("rejected") {
        return FailureClass::BrokerRejected;
    }
    if s.contains("did not fill within") {
        return FailureClass::BrokerTimeout;
    }
    if s.contains("timed out") || s.contains("timeout") {
        return FailureClass::ProviderTimeout;
    }
    if s.contains("tcp connect") || s.contains("dns error") || s.contains("connection refused") {
        return FailureClass::ProviderConnect;
    }
    if s.contains("anthropic api error") || s.contains("openai-compat api error") {
        return FailureClass::ProviderHttpError;
    }
    if s.contains("provider_decode")
        || s.contains("error decoding response body")
        || s.contains("invalid json response body")
        || s.contains("eof while parsing")
    {
        return FailureClass::ProviderDecode;
    }
    FailureClass::Unclassified
}

// ─── Repeated-tool-failure tracker ─────────────────────────────────────────

/// Per-slot failure budget on the exact `(tool_name, input_hash)` pair.
/// The third failure trips the block: 1 initial failure, 1 retry with a
/// `recovery.attempt` span, then 1 terminal failure with
/// `recovery.failed`. Picked from the audit's "block the pair for the
/// rest of the run" guidance plus a small head-room for transient
/// failures the agent can self-correct.
pub const MAX_TOOL_RETRIES_PER_PAIR: u8 = 3;

/// Tracks repeated failures of the same `(tool_name, input)` pair
/// inside a single slot execution. Lives in pipeline scope and resets
/// per slot — the audit's wording was per-cycle, but the natural seam
/// in [`crate::agent::execute::execute_slot`] is per-slot, which is
/// finer-grained and never wider than per-cycle.
///
/// Hashing the input collapses semantically-identical payloads even
/// when the model varies whitespace / key ordering. Uses the
/// canonical JSON byte form so `{"a":1,"b":2}` and `{"b":2,"a":1}`
/// produce different hashes — that's a deliberate trade-off: the
/// agent can re-order keys to retry, but we never assume two
/// pretty-printed forms of the same logical call are the same.
#[derive(Debug, Default)]
pub struct RepeatedToolFailureTracker {
    counts: HashMap<(String, String), u8>,
}

impl RepeatedToolFailureTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a failed invocation of `(tool_name, input)`. Returns the
    /// new failure count for the pair. Callers compare against
    /// [`MAX_TOOL_RETRIES_PER_PAIR`] to decide whether this failure
    /// trips the block.
    pub fn record_failure(&mut self, tool_name: &str, input: &serde_json::Value) -> u8 {
        let key = (tool_name.to_string(), hash_input(input));
        let entry = self.counts.entry(key).or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    /// `true` when an invocation of `(tool_name, input)` should be
    /// blocked because the pair has already failed
    /// [`MAX_TOOL_RETRIES_PER_PAIR`] times.
    pub fn is_blocked(&self, tool_name: &str, input: &serde_json::Value) -> bool {
        let key = (tool_name.to_string(), hash_input(input));
        self.counts.get(&key).copied().unwrap_or(0) >= MAX_TOOL_RETRIES_PER_PAIR
    }

    /// `input_hash` for the current `(tool_name, input)` pair, for
    /// span attributes / [`FailureClass::RepeatedToolFailure`].
    pub fn input_hash(input: &serde_json::Value) -> String {
        hash_input(input)
    }
}

fn hash_input(input: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(input).unwrap_or_else(|_| b"tool-input-serialize-error".to_vec());
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

// ─── MalformedJson repair-prompt builder ───────────────────────────────────
//
// F-5 phase 2a (`harness-recovery-malformed-json`): when the trader's text
// fails to parse as the canonical `TraderOutput` JSON shape, the eval
// executor invokes a single-shot repair attempt before propagating the
// original error. The conversation log appended on that retry carries
// the parse diagnostic + the response schema descriptor + a no-prose
// instruction so the model has every piece of information it needs to
// emit a clean JSON object on the second try.
//
// The body construction lives here so the call site in
// `eval::executor::recovery` stays minimal — paper.rs and backtest.rs
// dispatch through the same helper, which keeps the wire-shape of the
// repair turn identical across executors.

/// Build the user-message body for a malformed-json repair attempt. The
/// returned string carries:
///
///   1. The verbatim parse error from `TraderOutputError.detail` so the
///      model sees exactly which key, type, or token tripped the
///      deserializer.
///   2. The response-schema descriptor (name + serialized schema) so the
///      model is reminded what it should have emitted.
///   3. A one-line instruction forbidding prose, code fences, or further
///      tool calls — the second attempt must emit a single JSON object.
///
/// The text is deterministic for a given `(parse_error, schema)` pair so
/// the engine's prompt-hashing seam (A/B cache pairing across re-runs
/// with the same `cycle_id`) produces a stable digest. Operators
/// inspecting the trace dock see the same repair message every time a
/// strategy's trader emits the same unparseable response.
pub fn build_malformed_json_repair_message(parse_error: &str, schema: &ResponseSchema) -> String {
    // Render the schema body deterministically — `serde_json::to_string`
    // is field-order-stable for a `serde_json::Value` built from a
    // literal `json!` macro, but `to_string_pretty` is what callers
    // typically see in the trace dock, so we use that for human
    // readability. The schema descriptor is the same object the
    // dispatcher would have stamped on the original outbound request,
    // so quoting it here only restates known context.
    let schema_body = serde_json::to_string_pretty(&schema.schema)
        .unwrap_or_else(|_| "<schema-serialize-error>".to_string());
    format!(
        "Your previous response failed to parse: {parse_error}\n\
         \n\
         Emit a single JSON object matching the `{schema_name}` schema below. \
         Do not include prose, code fences, or tool calls. Return ONLY the JSON object.\n\
         \n\
         Schema:\n{schema_body}",
        schema_name = schema.name,
    )
}

// ─── MalformedJson repair-prompt dispatch ──────────────────────────────────
//
// The dispatch side of the repair path lives here (rather than in
// `eval::executor`) so both `paper.rs` and `backtest.rs` converge on the
// same helper and the wire shape of the repair turn is byte-stable
// across executors. The call sites in those modules only own the
// classification check + a thin projection into [`TraderRepairContext`];
// every step that touches the LlmDispatch / parses the second response /
// emits the `recovery.*` spans is centralised here.

/// Slot fields required to re-dispatch the trader for a repair attempt.
/// Both the legacy `Strategy.trader_slot` path and the agent-slot path
/// project into this shape before calling [`try_repair_malformed_json`]
/// so the helper stays oblivious to which path produced the original
/// failure.
///
/// Field-by-field semantics match the equivalent shape constructed inside
/// `execute_slot`:
/// - `system_prompt`: the slot's free-form prompt body (no preamble
///   added; the dispatcher's response-schema preamble is re-applied via
///   `response_schema` below).
/// - `model`: the effective model id — `LLMSlot::effective_model()` for
///   the legacy path, `ResolvedAgentSlot::slot.effective_model()` for
///   the agent path.
/// - `max_tokens`: the operator's per-request budget; `None` lets the
///   provider decide. Mirrors the value the original trader call used.
/// - `temperature`: same — pass-through verbatim.
pub struct TraderRepairContext<'a> {
    pub system_prompt: &'a str,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

/// Convenience predicate: returns `true` when the `TraderOutputError`
/// falls into the MalformedJson family and is therefore eligible for the
/// repair path. Callers in paper.rs / backtest.rs check this before
/// invoking [`try_repair_malformed_json`] so the helper isn't invoked for
/// `MissingField` / `InvalidField` / `EmptyText` failures (those are
/// owned by sibling phase-2 contracts).
pub fn is_malformed_json_recoverable(err: &TraderOutputError) -> bool {
    matches!(
        err.kind,
        TraderFailureKind::InvalidJson | TraderFailureKind::Truncated
    )
}

/// Single-shot repair attempt for `MalformedJson` family failures
/// (`InvalidJson` / `Truncated`). Returns `Ok(parsed)` on success and
/// emits a `recovery.attempt` span. Returns the ORIGINAL
/// [`TraderOutputError`] on second-attempt failure and emits
/// `recovery.failed` carrying the second-attempt error as `final_error`.
/// Callers propagate the returned error verbatim — the wire-stable
/// `[<tag>]` prefix on `eval_runs.error` stays exactly as today's path
/// produces it.
///
/// The dispatched repair LlmRequest carries:
///
///   1. The same `system_prompt` + `model` + `max_tokens` + `temperature`
///      the original trader call used, so the model has identical
///      context.
///   2. The same `response_schema` so OpenAI-compat providers re-emit
///      the strict json_schema response_format and Anthropic re-injects
///      the schema preamble.
///   3. A three-turn conversation log: the original user prompt (derived
///      from `seed_inputs` in the same shape `execute_slot` would have
///      produced), an assistant turn carrying the verbatim raw text the
///      model just emitted, and a user turn with the repair message
///      built by [`build_malformed_json_repair_message`].
///
/// The repair dispatch does NOT pass any tools — the model must emit a
/// single JSON object, not a tool_use. This is intentional: the contract
/// says "do not include prose, code fences, or tool calls" and removing
/// the tool definitions removes the temptation to emit one.
///
/// ## A/B cache pairing
///
/// The repair message body is deterministic for a given
/// `(parse_error, schema)` pair (see
/// [`build_malformed_json_repair_message`]). The seed-derived user prompt
/// is also deterministic because the eval executor's seed is
/// reconstructed from the scenario + bar history every cycle. Together
/// these mean the repair dispatch's prompt hash is reproducible across
/// re-runs of the same strategy/cycle, so a strategy that hits the
/// repair path once will hit the same repair path on every replay —
/// matching the existing A/B-compare deterministic-recovery expectation.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn try_repair_malformed_json(
    failed_response: &LlmResponse,
    original_error: TraderOutputError,
    repair_ctx: TraderRepairContext<'_>,
    seed_inputs: &serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    obs: Option<&ObsEmitter>,
    run_id: &str,
    decision_index: u32,
) -> Result<TraderOutput, TraderOutputError> {
    // Only the MalformedJson family is eligible for the repair path.
    // The contract reserves Truncated + InvalidJson; SchemaMissingField /
    // EmptyData / Tool* are owned by sibling contracts (or already
    // surfaced as today). The check is defensive — paper.rs and
    // backtest.rs only call this helper after they've matched on the
    // kind via `is_malformed_json_recoverable`.
    let class_tag = match original_error.kind {
        TraderFailureKind::InvalidJson => "invalid_json",
        TraderFailureKind::Truncated => "truncated",
        _ => return Err(original_error),
    };

    let schema = ResponseSchema::trader_output();

    // Reconstruct the original user prompt body in the same shape
    // `execute_slot` would have produced. The wording is identical so
    // the model sees byte-stable context across the original + repair
    // call. We deliberately drop the `agent_error_feedback` hoist that
    // `execute_slot` applies (it isn't relevant on the repair path —
    // the broker self-healing seam belongs to the first attempt).
    let initial_user_body = format!(
        "Inputs:\n{inputs}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data for the current decision asset only; emit your final decision as JSON.",
        inputs = serde_json::to_string_pretty(seed_inputs)
            .unwrap_or_else(|_| "<seed-serialize-error>".to_string()),
    );

    // The verbatim raw text the model just emitted. Anthropic / OpenAI
    // both accept an assistant turn with a single Text block, so we
    // re-build from `LlmResponse.text()` to keep the shape minimal.
    // Including only the text (no tool_use blocks) is the right call
    // because the malformed-json failure is text-side; any tool_use
    // blocks the model emitted before the parse failure are not part
    // of the response under repair.
    let assistant_raw = failed_response.text();

    let repair_user_body = build_malformed_json_repair_message(&original_error.detail, &schema);

    let messages = vec![
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: initial_user_body,
            }],
        },
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text { text: assistant_raw }],
        },
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: repair_user_body,
            }],
        },
    ];

    let req = LlmRequest {
        model: repair_ctx.model,
        system_prompt: repair_ctx.system_prompt.to_string(),
        messages,
        max_tokens: repair_ctx.max_tokens,
        // No tools on the repair turn — the model must emit a single
        // JSON object. Stripping the tool definitions removes the
        // temptation to emit one (and matches the repair-message
        // "do not include tool calls" instruction).
        tools: Vec::new(),
        temperature: repair_ctx.temperature,
        response_schema: Some(schema),
        cache_control: None,
        force_json: false,
    };

    let repair_resp = match dispatch.complete(req).await {
        Ok(r) => r,
        Err(e) => {
            // Dispatcher-level transport failure during the repair
            // attempt — emit `recovery.failed` and surface the original
            // parse error as the contract requires.
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("repair dispatch failed: {e:#}"),
                    )
                    .await;
            }
            return Err(original_error);
        }
    };

    match TraderOutput::parse_response(&repair_resp, run_id, decision_index) {
        Ok(parsed) => {
            // Repair landed — emit a `recovery.attempt` span with
            // retry_count=1 because exactly one repair attempt was made.
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_attempt(&fresh_span_id(), None, class_tag, 1)
                    .await;
            }
            tracing::info!(
                event = "trader_output_repair_recovered",
                run_id = %run_id,
                decision_index,
                class_tag,
                original_detail = %original_error.detail,
                "F-5 MalformedJson repair succeeded on retry 1",
            );
            Ok(parsed)
        }
        Err(second_err) => {
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("second attempt also failed to parse: {second_err}"),
                    )
                    .await;
            }
            tracing::warn!(
                event = "trader_output_repair_failed",
                run_id = %run_id,
                decision_index,
                class_tag,
                original_detail = %original_error.detail,
                second_detail = %second_err.detail,
                "F-5 MalformedJson repair exhausted (1 retry); surfacing original error",
            );
            // Contract: propagate the ORIGINAL error (not the second
            // attempt's) so `eval_runs.error` carries `[invalid_json]` /
            // `[truncated]` exactly as it did pre-F-5.
            Err(original_error)
        }
    }
}

// ─── SchemaMissingField repair (F-5 phase 2b) ──────────────────────────────
//
// `harness-recovery-schema-missing-field`. Targets the
// [`RecoveryFamily::SchemaMissingField`] family: trader response parsed as
// JSON but a required field was missing or invalid.
//
// Unlike the MalformedJson repair (which re-asks for the *whole* JSON
// object), the schema-patch repair asks the model to emit ONLY the
// offending field(s). The original response is then merged with the patch
// — second-attempt keys override first-attempt keys — and re-parsed via
// [`merge_and_reparse_trader_output`]. Cheaper than the full-response retry
// because the model is constrained to a single small JSON object.
//
// Fall-through behaviour: ONE patch attempt only. If the merged value
// still fails to parse, the ORIGINAL `TraderOutputError` is propagated
// (same fail-closed policy as MalformedJson). The dispatcher does NOT
// fall through into the MalformedJson repair — schema and malformed are
// disjoint families per [`FailureClass::family`], and double-repair is
// out of scope (it would silently use 2x the budget without operator
// consent).

/// Convenience predicate: returns `true` when the `TraderOutputError`
/// falls into the SchemaMissingField family and is therefore eligible for
/// the targeted-patch repair path. Callers in paper.rs / backtest.rs
/// check this before invoking [`try_repair_schema_missing_field`].
pub fn is_schema_missing_field_recoverable(err: &TraderOutputError) -> bool {
    matches!(
        err.kind,
        TraderFailureKind::MissingField | TraderFailureKind::InvalidField
    )
}

/// Build the user-message body for a schema-patch repair attempt. The
/// returned string carries:
///
///   1. The list of missing/invalid field names (comma-separated) so the
///      model sees exactly what needs to change.
///   2. A one-line instruction: emit JUST the offending fields as a
///      single JSON object — the engine merges the patch over the
///      original response, so other fields the model already produced
///      are accepted as-is.
///   3. The verbatim parse-error detail (clipped to the repaired-field
///      diagnostic) so the model has the specific complaint at hand.
///
/// The text is deterministic for a given `(field_list, parse_error)`
/// pair so the prompt-hashing seam produces a stable digest across
/// re-runs with the same `cycle_id` — see the A/B-cache pairing test in
/// the integration suite.
pub fn build_schema_missing_field_repair_message(problem_fields: &[String], parse_error: &str) -> String {
    let fields = if problem_fields.is_empty() {
        // Defensive: the dispatcher gates with `is_schema_missing_field_recoverable`
        // and the contract says we should always have at least one field,
        // but emit a usable prompt either way so a bad detail string
        // doesn't break the repair.
        "(unknown field — see error detail)".to_string()
    } else {
        problem_fields.join(", ")
    };
    format!(
        "Your previous response was missing or invalid for the following fields: [{fields}].\n\
         \n\
         Re-emit ONLY a single JSON object containing those fields, filled in correctly. \
         The other fields you produced are accepted as-is — do not repeat them. \
         Do not include prose, code fences, or tool calls.\n\
         \n\
         Validator detail: {parse_error}",
    )
}

/// Single-shot targeted-patch repair for the SchemaMissingField family
/// (`MissingField` / `InvalidField`). Returns `Ok(parsed)` on success
/// and emits a `recovery.attempt` span. Returns the ORIGINAL
/// [`TraderOutputError`] on second-attempt failure and emits
/// `recovery.failed` carrying the merged-reparse error as
/// `final_error`. Callers propagate the returned error verbatim — the
/// wire-stable `[missing_field]` / `[invalid_field]` prefix on
/// `eval_runs.error` stays exactly as today's path produces it.
///
/// The dispatched repair LlmRequest mirrors the MalformedJson helper:
/// same `system_prompt`/`model`/`max_tokens`/`temperature`, same
/// `response_schema`, no tools. The user turn carries
/// [`build_schema_missing_field_repair_message`] with the failing field
/// list. The model is expected to emit a small JSON object with just
/// those fields; the helper merges that patch over the original (partial)
/// response via [`merge_and_reparse_trader_output`].
///
/// ## A/B cache pairing
///
/// The repair body is deterministic for a given `(problem_fields,
/// parse_error)` pair. The seed-derived user prompt is reproducible
/// across re-runs because the eval executor's seed is reconstructed from
/// the scenario + bar history every cycle. Together these mean the
/// repair dispatch's prompt hash is reproducible across re-runs — see
/// the `schema_missing_field_repair_is_deterministic_for_ab_cache_pairing`
/// integration test.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn try_repair_schema_missing_field(
    failed_response: &LlmResponse,
    original_error: TraderOutputError,
    repair_ctx: TraderRepairContext<'_>,
    seed_inputs: &serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    obs: Option<&ObsEmitter>,
    run_id: &str,
    decision_index: u32,
) -> Result<TraderOutput, TraderOutputError> {
    let class_tag = match original_error.kind {
        TraderFailureKind::MissingField => "missing_field",
        TraderFailureKind::InvalidField => "invalid_field",
        _ => return Err(original_error),
    };

    let schema = ResponseSchema::trader_output();
    let problem_fields = original_error.problem_fields();

    // Reconstruct the original user prompt body in the same shape
    // `execute_slot` would have produced — byte-identical to the
    // MalformedJson repair so the model's context is consistent
    // across both repair families.
    let initial_user_body = format!(
        "Inputs:\n{inputs}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data for the current decision asset only; emit your final decision as JSON.",
        inputs = serde_json::to_string_pretty(seed_inputs)
            .unwrap_or_else(|_| "<seed-serialize-error>".to_string()),
    );

    let assistant_raw = failed_response.text();
    let repair_user_body = build_schema_missing_field_repair_message(&problem_fields, &original_error.detail);

    let messages = vec![
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: initial_user_body,
            }],
        },
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: assistant_raw.clone(),
            }],
        },
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: repair_user_body,
            }],
        },
    ];

    let req = LlmRequest {
        model: repair_ctx.model,
        system_prompt: repair_ctx.system_prompt.to_string(),
        messages,
        max_tokens: repair_ctx.max_tokens,
        // No tools — see MalformedJson helper for the rationale.
        tools: Vec::new(),
        temperature: repair_ctx.temperature,
        response_schema: Some(schema),
        cache_control: None,
        force_json: false,
    };

    let repair_resp = match dispatch.complete(req).await {
        Ok(r) => r,
        Err(e) => {
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("repair dispatch failed: {e:#}"),
                    )
                    .await;
            }
            return Err(original_error);
        }
    };

    let patch_text = repair_resp.text();
    match merge_and_reparse_trader_output(&assistant_raw, &patch_text, run_id, decision_index) {
        Ok(parsed) => {
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_attempt(&fresh_span_id(), None, class_tag, 1)
                    .await;
            }
            tracing::info!(
                event = "trader_output_schema_patch_recovered",
                run_id = %run_id,
                decision_index,
                class_tag,
                fields = ?problem_fields,
                "F-5 SchemaMissingField patch repair succeeded on retry 1",
            );
            Ok(parsed)
        }
        Err(second_err) => {
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("merge-and-reparse failed: {second_err}"),
                    )
                    .await;
            }
            tracing::warn!(
                event = "trader_output_schema_patch_failed",
                run_id = %run_id,
                decision_index,
                class_tag,
                original_detail = %original_error.detail,
                second_detail = %second_err.detail,
                "F-5 SchemaMissingField patch repair exhausted (1 retry); surfacing original error",
            );
            // Contract: propagate the ORIGINAL error so `eval_runs.error`
            // carries `[missing_field]` / `[invalid_field]` exactly as
            // it did pre-F-5.
            Err(original_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anyhow(s: &str) -> anyhow::Error {
        anyhow::anyhow!(s.to_string())
    }

    #[test]
    fn classify_trader_invalid_json_via_string_fallback() {
        let e = anyhow("run abc decision 5: trader_output[invalid_json]: not json (raw_excerpt=foo)");
        assert_eq!(classify(&e), FailureClass::TraderInvalidJson);
        assert_eq!(classify(&e).tag(), "invalid_json");
        assert_eq!(classify(&e).family(), RecoveryFamily::MalformedJson);
    }

    #[test]
    fn classify_trader_missing_field_via_string_fallback() {
        let e = anyhow("trader_output[missing_field]: conviction missing");
        assert_eq!(classify(&e), FailureClass::TraderMissingField);
        assert_eq!(classify(&e).family(), RecoveryFamily::SchemaMissingField);
    }

    #[test]
    fn classify_broker_unsupported_short_phrases() {
        let cases = [
            "alpaca crypto broker_unsupported: short_open is not supported for BTC/USD",
            "alpaca create_order: bracket orders not supported for this asset class",
            "asset is not shortable on Alpaca crypto",
        ];
        for s in cases {
            assert_eq!(classify(&anyhow(s)), FailureClass::BrokerUnsupported);
            assert_eq!(classify(&anyhow(s)).tag(), "broker_unsupported");
            assert_eq!(classify(&anyhow(s)).family(), RecoveryFamily::Unrecoverable);
        }
    }

    #[test]
    fn classify_broker_insufficient_funds_phrases() {
        for s in [
            "alpaca create_order: insufficient buying power for this order",
            "orderly: insufficient balance",
            "insufficient funds for trade",
        ] {
            assert_eq!(classify(&anyhow(s)), FailureClass::BrokerInsufficientFunds);
            assert_eq!(classify(&anyhow(s)).tag(), "broker_insufficient_funds");
        }
    }

    #[test]
    fn classify_provider_timeout_after_broker_timeout() {
        // Broker fill timeout must NOT get re-classified as a provider timeout.
        let broker = anyhow("alpaca order 01H... did not fill within 5 polls");
        assert_eq!(classify(&broker), FailureClass::BrokerTimeout);
        let provider = anyhow("openrouter request timed out after 60s");
        assert_eq!(classify(&provider), FailureClass::ProviderTimeout);
    }

    #[test]
    fn classify_provider_decode() {
        let e = anyhow(
            "provider_decode: anthropic returned invalid JSON response body: EOF while parsing a value at line 1707 column 0",
        );
        assert_eq!(classify(&e), FailureClass::ProviderDecode);
    }

    #[test]
    fn classify_walks_context_chain() {
        // Outer wrapper has no class hint; inner cause does.
        let inner = anyhow("alpaca create_order: bracket orders not supported for this asset class");
        let wrapped: anyhow::Error = anyhow::Error::msg("paper eval submit_order failed").context(inner);
        // anyhow Context inverts the wrap direction — re-build the
        // chain explicitly to mirror the executor's `with_context`
        // pattern.
        let actual: anyhow::Result<()> = Err(anyhow::anyhow!(
            "alpaca create_order: bracket orders not supported for this asset class"
        ))
        .map_err(|e| e.context("paper eval submit_order failed"));
        let err = actual.unwrap_err();
        // Confirm the formatted chain walks the cause; the classifier
        // sees the inner via `format!("{:#}", err)`.
        assert_eq!(classify(&err), FailureClass::BrokerUnsupported);
        let _ = wrapped; // silence unused
    }

    #[test]
    fn classify_repeated_broker_error_before_inner_class() {
        // The circuit-breaker tag must match before the embedded
        // broker_min_order_size class.
        let e = anyhow("[repeated_broker_error] N=3 consecutive broker_min_order_size rejections");
        assert_eq!(classify(&e), FailureClass::RepeatedBrokerError);
        assert_eq!(classify(&e).family(), RecoveryFamily::RepeatedToolFailure);
    }

    #[test]
    fn classify_unclassified_falls_through() {
        let e = anyhow("some completely unrecognized error message");
        assert_eq!(classify(&e), FailureClass::Unclassified);
        assert_eq!(classify(&e).tag(), "unclassified");
        assert_eq!(classify(&e).family(), RecoveryFamily::Unrecoverable);
    }

    #[test]
    fn repeated_tool_failure_tracker_blocks_after_threshold() {
        let mut t = RepeatedToolFailureTracker::new();
        let input = serde_json::json!({"symbol": "BTC/USD", "side": "buy"});
        assert!(!t.is_blocked("submit_order", &input));
        for i in 1..MAX_TOOL_RETRIES_PER_PAIR {
            let count = t.record_failure("submit_order", &input);
            assert_eq!(count, i);
            assert!(!t.is_blocked("submit_order", &input));
        }
        // Threshold-th failure trips the block.
        let count = t.record_failure("submit_order", &input);
        assert_eq!(count, MAX_TOOL_RETRIES_PER_PAIR);
        assert!(t.is_blocked("submit_order", &input));
    }

    #[test]
    fn repeated_tool_failure_distinguishes_pairs() {
        let mut t = RepeatedToolFailureTracker::new();
        let a = serde_json::json!({"symbol": "BTC/USD"});
        let b = serde_json::json!({"symbol": "ETH/USD"});
        for _ in 0..MAX_TOOL_RETRIES_PER_PAIR {
            t.record_failure("submit_order", &a);
        }
        assert!(t.is_blocked("submit_order", &a));
        assert!(!t.is_blocked("submit_order", &b));
        assert!(!t.is_blocked("get_quote", &a));
    }

    #[test]
    fn repeated_tool_failure_input_hash_is_stable() {
        // Same value → same hash. Different value → different hash.
        let a = serde_json::json!({"x": 1});
        let b = serde_json::json!({"x": 1});
        let c = serde_json::json!({"x": 2});
        assert_eq!(
            RepeatedToolFailureTracker::input_hash(&a),
            RepeatedToolFailureTracker::input_hash(&b)
        );
        assert_ne!(
            RepeatedToolFailureTracker::input_hash(&a),
            RepeatedToolFailureTracker::input_hash(&c)
        );
    }

    #[test]
    fn all_failure_class_variants_have_stable_tags() {
        // Coverage check: any newly-added FailureClass variant must
        // explicitly extend tag(). The match-exhaustiveness in tag()
        // itself enforces this at compile time; this test pins the
        // wire-side tags so a rename is caught immediately.
        let expected: &[(FailureClass, &'static str)] = &[
            (FailureClass::TraderEmpty, "empty"),
            (FailureClass::TraderToolUseOnly, "tool_use_only"),
            (FailureClass::TraderTruncated, "truncated"),
            (FailureClass::TraderInvalidJson, "invalid_json"),
            (FailureClass::TraderMissingField, "missing_field"),
            (FailureClass::TraderInvalidField, "invalid_field"),
            (FailureClass::TraderMissingResponse, "missing_response"),
            (FailureClass::ProviderTimeout, "provider_timeout"),
            (FailureClass::ProviderConnect, "provider_connect"),
            (FailureClass::ProviderHttpError, "provider_http_error"),
            (FailureClass::ProviderDecode, "provider_decode"),
            (FailureClass::ProviderRateLimited, "provider_rate_limited"),
            (FailureClass::ProviderMissingChoices, "provider_missing_choices"),
            (
                FailureClass::ContextOverflow {
                    message: "ctx".into(),
                    provider: "anthropic".into(),
                },
                "context_overflow",
            ),
            (FailureClass::BrokerAuth, "broker_auth"),
            (FailureClass::BrokerUnsupported, "broker_unsupported"),
            (FailureClass::BrokerInsufficientFunds, "broker_insufficient_funds"),
            (FailureClass::BrokerTimeout, "broker_timeout"),
            (FailureClass::BrokerRejected, "broker_rejected"),
            (FailureClass::RepeatedBrokerError, "repeated_broker_error"),
            (
                FailureClass::RepeatedToolFailure {
                    tool_name: "x".into(),
                    input_hash: "y".into(),
                },
                "repeated_tool_failure",
            ),
            (
                FailureClass::BudgetExceeded {
                    kind: "wall_ms".into(),
                },
                "budget_exceeded",
            ),
            (FailureClass::NoDecision, "no_decision"),
            (FailureClass::Unclassified, "unclassified"),
        ];
        for (variant, tag) in expected {
            assert_eq!(variant.tag(), *tag, "tag drift for variant: {variant:?}");
        }
    }

    #[test]
    fn build_malformed_json_repair_message_carries_parse_error_schema_and_instruction() {
        // F-5 phase 2a contract acceptance: the repair body must contain
        // (1) the verbatim parse-error detail, (2) the schema name hint,
        // and (3) the no-prose-no-fences instruction. The trader-output
        // canonical schema name is `trader_output`.
        let schema = ResponseSchema::trader_output();
        let parse_error = "expected value at line 1 column 1";
        let body = build_malformed_json_repair_message(parse_error, &schema);

        assert!(
            body.contains(parse_error),
            "repair message must include the verbatim parse error, got: {body}"
        );
        assert!(
            body.contains("trader_output"),
            "repair message must reference the schema name, got: {body}"
        );
        assert!(
            body.contains("Do not include prose, code fences, or tool calls"),
            "repair message must carry the no-prose instruction, got: {body}"
        );
        assert!(
            body.contains("Return ONLY the JSON object"),
            "repair message must instruct returning JSON only, got: {body}"
        );
    }

    #[test]
    fn build_schema_missing_field_repair_message_carries_field_list_and_instruction() {
        let fields = vec!["conviction".to_string()];
        let body = build_schema_missing_field_repair_message(&fields, "missing field `conviction`");
        assert!(
            body.contains("[conviction]"),
            "repair body must reference the field list: {body}"
        );
        assert!(
            body.contains("Re-emit ONLY a single JSON object"),
            "repair body must instruct emitting ONLY the bad fields: {body}"
        );
        assert!(
            body.contains("missing field `conviction`"),
            "repair body must include the verbatim parse-error detail: {body}"
        );
        assert!(
            body.contains("Do not include prose, code fences, or tool calls"),
            "repair body must carry the no-prose-no-fences instruction: {body}"
        );
    }

    #[test]
    fn build_schema_missing_field_repair_message_lists_multiple_fields() {
        let fields = vec!["action".to_string(), "conviction".to_string()];
        let body = build_schema_missing_field_repair_message(&fields, "two fields failed");
        assert!(body.contains("[action, conviction]"), "multi-field list: {body}");
    }

    #[test]
    fn build_schema_missing_field_repair_message_is_deterministic_for_ab_cache_pairing() {
        // A/B cache pairing acceptance: byte-stable for the same
        // `(field_list, parse_error)` pair so the prompt-hash digest is
        // reproducible across re-runs of the same strategy/cycle.
        let fields = vec!["action".to_string()];
        let detail = "invalid value for field `action`";
        let a = build_schema_missing_field_repair_message(&fields, detail);
        let b = build_schema_missing_field_repair_message(&fields, detail);
        assert_eq!(a, b);
    }

    #[test]
    fn build_malformed_json_repair_message_is_deterministic_for_ab_cache_pairing() {
        // A/B cache pairing acceptance: the repair body must be
        // byte-stable for the same `(parse_error, schema)` pair so the
        // prompt-hash digest is reproducible across re-runs of the same
        // strategy/cycle. Two calls with identical inputs must produce
        // identical strings.
        let schema = ResponseSchema::trader_output();
        let parse_error = "missing field `action` at line 2 column 8";
        let a = build_malformed_json_repair_message(parse_error, &schema);
        let b = build_malformed_json_repair_message(parse_error, &schema);
        assert_eq!(a, b, "repair message must be deterministic for cache pairing");
    }
}
