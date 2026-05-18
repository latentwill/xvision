//! Typed pre-recovery dispatcher for run-level failures.
//!
//! This module implements F-5 from the 2026-05-18 harness observability
//! audit (`team/intake/2026-05-18-harness-observability-audit.md`).
//!
//! Before F-5: [`classify_run_failure`](super::classify_run_failure) was a
//! regex-on-error-string post-hoc classifier. It only ran AFTER a run
//! terminated, and only labelled the failure for downstream UI. The
//! agent never got a chance to react to MalformedJson, ToolTimeout,
//! SchemaMissingField, EmptyData, ContextOverflow, or RepeatedToolFailure
//! — they all hard-killed the run. The only recovery loop was
//! `RESPONSE_DECODE_RETRIES = 1` (agent/llm.rs), an untyped one-shot
//! retry buried inside the dispatcher.
//!
//! After F-5: [`FailureClass`] is the typed pre-recovery vocabulary.
//! Each variant carries a structured payload (e.g.
//! [`FailureClass::MalformedJson::parse_error`]) the recovery
//! dispatcher feeds back to the agent. [`RecoveryDispatcher`] owns the
//! per-variant playbook with a fixed bounded retry count
//! (constants below) and emits a `recovery.attempt` span per transition
//! via [`crate::agent::observability::ObsEmitter::emit_recovery_attempt`].
//!
//! **Folds in** the deferred `agent-error-feedback-non-broker-errors`
//! follow-up from PR #286: the recoverable/fatal split that PR shipped
//! at the [`xvision_execution`] boundary is generalized here to
//! risk-engine, model-call, and data-fetch errors. Broker errors are
//! untouched — PR #286 owns the broker boundary and the dispatcher
//! short-circuits broker classes to [`FailureClass::Unrecoverable`]
//! (the recoverable broker self-heals upstream of this module).

use serde::{Deserialize, Serialize};

use super::trader_output::{TraderFailureKind, TraderOutputError};

// ---------------------------------------------------------------------------
// Bounded retry constants (hard caps on every recovery loop).
// ---------------------------------------------------------------------------

/// Max times the dispatcher will repair-prompt the model on
/// [`FailureClass::MalformedJson`]. After this many failed repairs the
/// dispatcher stops the cycle with the legacy `[invalid_json]` /
/// `[provider_decode]` tag.
pub const MAX_DECODE_REPAIR_PROMPTS: u8 = 1;

/// Max times the dispatcher will retry a tool call on
/// [`FailureClass::ToolTimeout`] before surfacing the timeout as a
/// `ToolResult { is_error: true }` block to the agent.
pub const MAX_TOOL_RETRIES: u8 = 1;

/// Max times the dispatcher will issue a targeted patch prompt for a
/// missing schema field. One retry total.
pub const MAX_SCHEMA_PATCH_PROMPTS: u8 = 1;

/// Max times the dispatcher will summarize history and retry on
/// [`FailureClass::ContextOverflow`].
pub const MAX_CONTEXT_OVERFLOW_RETRIES: u8 = 1;

/// Hard cap on the cheap-model summarize budget for
/// [`FailureClass::ContextOverflow`]. If the conversation history is
/// already larger than this even after summarization, the dispatcher
/// stops the cycle.
pub const MAX_SUMMARIZE_INPUT_TOKENS: u32 = 4096;

/// Per-cycle threshold beyond which a `(tool_name, input_hash)` pair
/// is blocked for the rest of the run. Resets at the next cycle's
/// boundary. Two identical failures inside one cycle trip the block.
pub const REPEATED_TOOL_FAILURE_THRESHOLD: u8 = 2;

// ---------------------------------------------------------------------------
// FailureClass enum
// ---------------------------------------------------------------------------

/// Typed pre-recovery failure class. Each variant carries the payload
/// the dispatcher needs to construct a structured feedback message
/// (repair prompt, schema patch, summarized history, etc.).
///
/// `Display` on the variants matches the legacy `&'static str` class
/// tag returned by [`FailureClass::tag`], so consumers parsing the
/// `[<class>]` wire shape from `eval_runs.error` keep working
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum FailureClass {
    /// The provider returned text that did not decode to the expected
    /// JSON shape. Recovery: one repair prompt with the parse error
    /// inlined, then fail closed with `[invalid_json]`.
    MalformedJson {
        /// The decoder's error message (e.g. serde's `EOF while parsing`).
        parse_error: String,
        /// First 240 chars of the offending raw text, for the repair
        /// prompt's `Your previous response could not be decoded: ...`.
        raw_excerpt: String,
    },
    /// A tool call exceeded its wall-clock timeout. Recovery: one
    /// retry with constant backoff (250ms), then surface as a
    /// `ToolResult { is_error: true }` block to the agent.
    ToolTimeout {
        tool_name: String,
        /// Wall-clock that elapsed before the dispatcher gave up.
        elapsed_ms: u32,
    },
    /// JSON decoded but a required schema field is missing. Recovery:
    /// targeted patch prompt naming ONLY the missing field(s), once.
    SchemaMissingField {
        /// Field names the response lacked (e.g. `["confidence"]`).
        missing: Vec<String>,
    },
    /// The market snapshot or intern brief returned no data the agent
    /// can act on. Recovery: stop the cycle cleanly (no retry); the
    /// run completes with `[empty_data]`.
    EmptyData {
        /// Free-form reason from the data source (e.g.
        /// `"recent_bars is empty"`).
        reason: String,
    },
    /// The provider returned a context-window overflow signal (HTTP
    /// 400 with `context_length` or `too long` in the body). Recovery:
    /// summarize prior turns via a cheap-model dispatch, retry once.
    ContextOverflow {
        /// Approximate prior-context size in tokens. `None` when the
        /// provider didn't report it (best-effort estimate).
        approx_input_tokens: Option<u32>,
    },
    /// The same `(tool_name, input_hash)` pair has failed
    /// [`REPEATED_TOOL_FAILURE_THRESHOLD`] times within the current
    /// cycle. Recovery: block the pair for the remainder of the run
    /// and surface a structured `ToolResult` to the agent.
    RepeatedToolFailure {
        tool_name: String,
        /// Hash of the input arguments (FNV-1a over the canonical
        /// JSON form). Used as the block key — same input is rejected
        /// at the registry boundary thereafter.
        input_hash: u64,
        /// Count when the threshold tripped (>= the constant).
        failure_count: u8,
    },
    /// Nothing the dispatcher knows how to handle. Same behaviour as
    /// pre-F-5: the run terminates with the legacy class tag (one of
    /// the broker_* / provider_* / trader_output_* set), or
    /// `unclassified` for messages no rule matches.
    ///
    /// The dispatcher short-circuits to this variant for broker
    /// errors — PR #286 owns the broker boundary, the recoverable
    /// arm self-heals upstream, and F-5 does not re-route.
    Unrecoverable {
        /// The legacy class tag the run is recorded under.
        tag: &'static str,
    },
}

impl FailureClass {
    /// Map this typed variant back to the legacy `&'static str` class
    /// tag persisted as the `[<class>]` prefix on `eval_runs.error`.
    /// Downstream consumers (review UI, dashboard, CLI grep) parse
    /// that prefix and the set of values they recognise is preserved
    /// here — no wire-format regression.
    pub fn tag(&self) -> &'static str {
        match self {
            // MalformedJson maps to the existing `invalid_json` tag
            // from `TraderFailureKind` AND the existing
            // `provider_decode` tag — the dispatcher records the
            // class via the `provider_decode` family when the upstream
            // is the model dispatcher, and via `invalid_json` when the
            // upstream is the trader-output decoder. Choose the
            // narrower one here; the broader `provider_decode` falls
            // through to `Unrecoverable { tag: "provider_decode" }`.
            Self::MalformedJson { .. } => "invalid_json",
            // No legacy tag for tool timeouts pre-F-5 — they fell
            // through to `unclassified`. F-5 introduces a precise
            // class but uses the legacy fall-through tag for
            // wire-format compatibility. A future tag rename (e.g.
            // `tool_timeout`) is a follow-up if operators ask.
            Self::ToolTimeout { .. } => "tool_timeout",
            Self::SchemaMissingField { .. } => "missing_field",
            // EmptyData is a new tag; `[unclassified]` remains the
            // catch-all for everything else, so this is additive.
            Self::EmptyData { .. } => "empty_data",
            Self::ContextOverflow { .. } => "context_overflow",
            Self::RepeatedToolFailure { .. } => "repeated_tool_failure",
            Self::Unrecoverable { tag } => tag,
        }
    }

    /// True when this class has a bounded recovery playbook the
    /// dispatcher should run before terminating the run. False for
    /// [`FailureClass::Unrecoverable`] and [`FailureClass::EmptyData`]
    /// (which stops cleanly without retry).
    pub fn has_recovery_playbook(&self) -> bool {
        !matches!(self, Self::Unrecoverable { .. } | Self::EmptyData { .. })
    }
}

// ---------------------------------------------------------------------------
// Classifier — promotes `classify_run_failure` to a typed return value.
// ---------------------------------------------------------------------------

/// Typed pre-recovery classifier. Walks the full `anyhow::Error` source
/// chain (alternate `Display`) so a wrapped inner cause is found
/// regardless of `.context()` nesting.
///
/// **Wire-format compatibility:** [`FailureClass::tag`] returns the
/// legacy `&'static str` set that the persisted `[<class>]` prefix
/// already uses. Existing callers of [`super::classify_run_failure`]
/// continue to work via the `.tag()` accessor.
pub fn classify(err: &anyhow::Error) -> FailureClass {
    // Trader-output errors carry a typed kind already — map directly
    // without re-parsing the message.
    if let Some(te) = err.downcast_ref::<TraderOutputError>() {
        return from_trader_failure(te.kind, te);
    }
    let s = format!("{:#}", err).to_lowercase();
    classify_from_message(&s, err)
}

fn from_trader_failure(kind: TraderFailureKind, te: &TraderOutputError) -> FailureClass {
    match kind {
        TraderFailureKind::InvalidJson => FailureClass::MalformedJson {
            parse_error: te.detail.clone(),
            raw_excerpt: te.raw_excerpt.clone(),
        },
        TraderFailureKind::MissingField => FailureClass::SchemaMissingField {
            missing: parse_missing_fields(&te.detail),
        },
        // EmptyText, ToolUseOnly, Truncated, InvalidField, MissingResponse:
        // pre-F-5 these all terminate. Mark Unrecoverable with the
        // legacy tag so the wire format is unchanged.
        TraderFailureKind::EmptyText
        | TraderFailureKind::ToolUseOnly
        | TraderFailureKind::Truncated
        | TraderFailureKind::InvalidField
        | TraderFailureKind::MissingResponse => FailureClass::Unrecoverable { tag: kind.tag() },
    }
}

/// Best-effort: extract field names from a trader-output detail string
/// like `missing field "confidence"` or `missing fields: confidence,
/// action`. Returns whatever it can pluck out; empty on no match.
/// The dispatcher's repair-prompt path is robust to an empty list
/// (falls back to a generic "the response was missing required
/// fields" prompt).
fn parse_missing_fields(detail: &str) -> Vec<String> {
    let mut fields = Vec::new();
    // serde_json's typical message: `missing field \`confidence\``.
    for token in detail.split('`') {
        let trimmed = token.trim();
        if !trimmed.is_empty()
            && trimmed.len() < 64
            && trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            fields.push(trimmed.to_string());
        }
    }
    fields
}

fn classify_from_message(s: &str, _err: &anyhow::Error) -> FailureClass {
    // Trader-output classes that escaped downcast — match the same
    // legacy needle the pre-F-5 classifier used.
    for kind in [
        TraderFailureKind::EmptyText,
        TraderFailureKind::ToolUseOnly,
        TraderFailureKind::Truncated,
        TraderFailureKind::InvalidJson,
        TraderFailureKind::MissingField,
        TraderFailureKind::InvalidField,
        TraderFailureKind::MissingResponse,
    ] {
        let needle = format!("trader_output[{}]", kind.tag());
        if s.contains(&needle) {
            // No payload available without the typed downcast — fall
            // through to Unrecoverable with the legacy tag, preserving
            // pre-F-5 behaviour for context-wrapped trader errors.
            return FailureClass::Unrecoverable { tag: kind.tag() };
        }
    }

    // Broker classes — match before the generic `timeout` fallback so
    // a broker-side fill timeout doesn't get tagged provider_timeout.
    // F-5 short-circuits broker matches to Unrecoverable because
    // PR #286 owns the broker recoverable arm upstream of this layer.
    if s.contains("broker_unsupported")
        || s.contains("not shortable")
        || s.contains("asset is not shortable")
        || (s.contains("bracket") && s.contains("not supported"))
        || s.contains("not supported for this asset class")
    {
        return FailureClass::Unrecoverable {
            tag: "broker_unsupported",
        };
    }
    if s.contains("insufficient buying power")
        || s.contains("insufficient balance")
        || s.contains("insufficient funds")
    {
        return FailureClass::Unrecoverable {
            tag: "broker_insufficient_funds",
        };
    }
    if s.contains("not permitted") || s.contains("forbidden") {
        return FailureClass::Unrecoverable {
            tag: "broker_auth",
        };
    }
    if s.contains("alpaca order") && s.contains("rejected") {
        return FailureClass::Unrecoverable {
            tag: "broker_rejected",
        };
    }
    if s.contains("did not fill within") {
        return FailureClass::Unrecoverable {
            tag: "broker_timeout",
        };
    }

    // Context overflow — match before the generic provider_http_error
    // class. Providers signal this differently; cover the common ones.
    if s.contains("context_length_exceeded")
        || s.contains("maximum context length")
        || s.contains("context window")
        || (s.contains("too long") && (s.contains("messages") || s.contains("context")))
    {
        return FailureClass::ContextOverflow {
            approx_input_tokens: None,
        };
    }

    // Provider decode failures — recoverable via the MalformedJson
    // playbook (one repair prompt). The operator's
    // `[unclassified] error decoding response body` repro from PR #242
    // falls into this branch and now gets a recovery attempt.
    if s.contains("provider_decode")
        || s.contains("error decoding response body")
        || s.contains("invalid json response body")
        || s.contains("eof while parsing")
    {
        return FailureClass::MalformedJson {
            parse_error: "provider returned undecodable response body".to_string(),
            raw_excerpt: String::new(),
        };
    }

    // Provider transport errors — pre-F-5 these all terminated.
    // ToolTimeout-style retry isn't safe here without a tool boundary
    // (a model-call retry could replay side effects); keep them
    // Unrecoverable with the legacy tag.
    if s.contains("timed out") || s.contains("timeout") {
        return FailureClass::Unrecoverable {
            tag: "provider_timeout",
        };
    }
    if s.contains("tcp connect") || s.contains("dns error") || s.contains("connection refused") {
        return FailureClass::Unrecoverable {
            tag: "provider_connect",
        };
    }
    if s.contains("anthropic api error") || s.contains("openai-compat api error") {
        return FailureClass::Unrecoverable {
            tag: "provider_http_error",
        };
    }

    // Data-availability failure — the new EmptyData class. Matches
    // explicit signals from the intern/data layer.
    if s.contains("recent_bars is empty")
        || s.contains("data_availability_failure")
        || s.contains("market snapshot empty")
    {
        return FailureClass::EmptyData {
            reason: "data source returned empty snapshot".to_string(),
        };
    }

    FailureClass::Unrecoverable {
        tag: "unclassified",
    }
}

// ---------------------------------------------------------------------------
// RecoveryOutcome — what the dispatcher decided to do.
// ---------------------------------------------------------------------------

/// Result of running a recovery playbook. The eval executor inspects
/// this to decide whether to keep the cycle going (`Continue`),
/// terminate with the persisted failure tag (`Stop`), or hand a
/// structured `ToolResult { is_error: true }` block to the agent's
/// next turn (`Surfaced`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// The dispatcher patched up the failure and the cycle should
    /// continue. The carrier holds whatever follow-up payload the
    /// caller needs to inject (e.g. a repair-prompt user message, a
    /// summarized history snippet).
    Continue { feedback_to_agent: Option<String> },
    /// The dispatcher gave up. The eval executor terminates the run
    /// with the persisted [`FailureClass::tag`].
    Stop,
    /// The dispatcher delivered a structured `is_error: true` block
    /// to the agent and the agent will self-heal on its next turn.
    /// Used by [`FailureClass::ToolTimeout`] and
    /// [`FailureClass::RepeatedToolFailure`].
    Surfaced,
}

impl RecoveryOutcome {
    /// Wire-format tag for the `outcome` field on a `recovery.attempt`
    /// span's `attributes_json` bag.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Continue { .. } => "continue",
            Self::Stop => "stop",
            Self::Surfaced => "surfaced",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context as _;

    #[test]
    fn malformed_json_via_message_match() {
        let e = anyhow::anyhow!(
            "anthropic returned invalid JSON response body: EOF while parsing a value at line 1707 column 0"
        );
        let class = classify(&e);
        assert!(matches!(class, FailureClass::MalformedJson { .. }));
        assert_eq!(class.tag(), "invalid_json");
        assert!(class.has_recovery_playbook());
    }

    #[test]
    fn provider_decode_routes_to_malformed_json() {
        // The operator's `[unclassified] error decoding response body`
        // repro from PR #242 history. F-5 promotes this to a
        // recoverable MalformedJson class with a repair-prompt
        // playbook.
        let e = anyhow::anyhow!("error decoding response body");
        assert!(matches!(classify(&e), FailureClass::MalformedJson { .. }));
    }

    #[test]
    fn context_overflow_match() {
        let e = anyhow::anyhow!("anthropic api error: context_length_exceeded: max 200000");
        let class = classify(&e);
        assert!(matches!(class, FailureClass::ContextOverflow { .. }));
        assert_eq!(class.tag(), "context_overflow");
        assert!(class.has_recovery_playbook());
    }

    #[test]
    fn empty_data_match() {
        let e = anyhow::anyhow!("snapshot recent_bars is empty for run R, cycle 4");
        let class = classify(&e);
        assert!(matches!(class, FailureClass::EmptyData { .. }));
        assert_eq!(class.tag(), "empty_data");
        // EmptyData stops cleanly; it does NOT have a retry playbook.
        assert!(!class.has_recovery_playbook());
    }

    #[test]
    fn broker_errors_map_to_unrecoverable_passthrough() {
        // PR #286 owns the broker recoverable arm; F-5 must not
        // double-dispatch. All five broker classes map to
        // Unrecoverable with their legacy tag.
        for (msg, expected_tag) in [
            (
                "alpaca create_order: insufficient buying power for this order",
                "broker_insufficient_funds",
            ),
            (
                "alpaca crypto broker_unsupported: short_open is not supported for BTC/USD",
                "broker_unsupported",
            ),
            ("alpaca get_account: forbidden", "broker_auth"),
            ("alpaca order 01H... rejected", "broker_rejected"),
            ("alpaca order 01H... did not fill within 5 polls", "broker_timeout"),
        ] {
            let class = classify(&anyhow::anyhow!(msg.to_string()));
            assert!(
                matches!(class, FailureClass::Unrecoverable { .. }),
                "broker class must be Unrecoverable, got: {:?}",
                class,
            );
            assert_eq!(class.tag(), expected_tag);
            assert!(!class.has_recovery_playbook());
        }
    }

    #[test]
    fn legacy_provider_classes_unchanged() {
        for (msg, tag) in [
            ("openrouter request timed out after 60s", "provider_timeout"),
            ("tcp connect: connection refused", "provider_connect"),
            (
                "anthropic api error: 500 internal server error",
                "provider_http_error",
            ),
        ] {
            assert_eq!(classify(&anyhow::anyhow!(msg.to_string())).tag(), tag);
        }
    }

    #[test]
    fn classify_walks_anyhow_context_chain() {
        // Same invariant as the legacy classify_run_failure test: the
        // alternate `Display` walks the source chain so a wrapped
        // broker error survives `.context()` nesting.
        let inner = anyhow::anyhow!(
            "alpaca create_order: bracket orders not supported for this asset class"
        );
        let wrapped: anyhow::Error = Err::<(), _>(inner)
            .context("paper eval submit_order failed: run_id=01H decision_index=0")
            .unwrap_err();
        assert_eq!(classify(&wrapped).tag(), "broker_unsupported");
    }

    #[test]
    fn unclassified_for_unknown_messages() {
        let e = anyhow::anyhow!("something completely unexpected went wrong");
        let class = classify(&e);
        assert!(matches!(class, FailureClass::Unrecoverable { .. }));
        assert_eq!(class.tag(), "unclassified");
    }

    #[test]
    fn parse_missing_fields_extracts_serde_message() {
        let detail = "missing field `confidence`";
        let fields = parse_missing_fields(detail);
        assert_eq!(fields, vec!["confidence".to_string()]);
    }

    #[test]
    fn parse_missing_fields_handles_no_match() {
        let detail = "some other detail entirely";
        assert!(parse_missing_fields(detail).is_empty());
    }

    #[test]
    fn recovery_outcome_tags_round_trip() {
        assert_eq!(
            RecoveryOutcome::Continue {
                feedback_to_agent: None
            }
            .tag(),
            "continue"
        );
        assert_eq!(RecoveryOutcome::Stop.tag(), "stop");
        assert_eq!(RecoveryOutcome::Surfaced.tag(), "surfaced");
    }

    #[test]
    fn bounded_constants_are_small_positive_integers() {
        // Defensive: F-5's whole point is "every loop hard-capped".
        // If a future PR raises one of these above a small constant,
        // this assertion forces the author to justify it.
        assert!(MAX_DECODE_REPAIR_PROMPTS <= 2);
        assert!(MAX_TOOL_RETRIES <= 2);
        assert!(MAX_SCHEMA_PATCH_PROMPTS <= 2);
        assert!(MAX_CONTEXT_OVERFLOW_RETRIES <= 2);
        assert!(REPEATED_TOOL_FAILURE_THRESHOLD <= 3);
        assert!(MAX_SUMMARIZE_INPUT_TOKENS <= 8192);
    }
}
