//! Executor trait + concrete impls. Phase 3.B of the Eval Engine plan.
//!
//! The Executor abstracts over the two run modes:
//! - **Backtest** — replays a parquet fixture in chronological order,
//!   simulates fills with slippage + fees. No broker required.
//! - **Paper** — drives `BrokerSurface::submit_order` against a real or
//!   mocked broker, suitable for the v1 demo path against Alpaca paper.
//!
//! Callers (`engine::api::eval::run`, the eval CLI) pick an executor by
//! `RunMode` and call `run(...)` once per `xvn eval run` invocation.

pub mod backtest;
pub mod paper;
pub mod recovery;
pub mod trader_output;

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::eval::run::{MetricsSummary, Run};
use crate::eval::scenario::Scenario;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

pub use backtest::BacktestExecutor;
pub use paper::PaperExecutor;
pub use recovery::{
    classify as classify_failure_typed, FailureClass, RecoveryOutcome,
    MAX_CONTEXT_OVERFLOW_RETRIES, MAX_DECODE_REPAIR_PROMPTS, MAX_SCHEMA_PATCH_PROMPTS,
    MAX_SUMMARIZE_INPUT_TOKENS, MAX_TOOL_RETRIES, REPEATED_TOOL_FAILURE_THRESHOLD,
};
pub use trader_output::{TraderFailureKind, TraderOutputError};

/// Stable failure-class tag for a run-level error. Paper/backtest executors
/// prefix the persisted `eval_runs.error` string with `[<class>]` so review
/// and UI consumers can read the class without re-parsing the full message.
///
/// Classes:
///  - Trader output classes: `empty`, `tool_use_only`, `truncated`,
///    `invalid_json`, `missing_field`, `invalid_field`, `missing_response`.
///  - Provider transport classes: `provider_timeout`, `provider_connect`,
///    `provider_http_error`, `provider_decode`.
///  - Broker transport classes: `broker_auth`, `broker_unsupported`,
///    `broker_insufficient_funds`, `broker_timeout`, `broker_rejected`.
///  - `unclassified` for anything else.
///
/// The matcher walks the full `anyhow::Error` source chain (via the alternate
/// `Display`) so an underlying broker rejection survives a `with_context`
/// wrap from the surface caller.
pub fn classify_run_failure(err: &anyhow::Error) -> &'static str {
    // Delegate to the typed classifier and project back to the
    // legacy `&'static str` set via `FailureClass::tag()`. F-5
    // (`harness-recovery-state-machine`) introduced the typed
    // pre-recovery dispatch surface; the wire-format prefix
    // `[<class>]` that downstream consumers parse is preserved
    // unchanged.
    //
    // Four cases need a small fixup because [`FailureClass::tag`]
    // either collapses semantically-distinct legacy tags or introduces
    // a new tag that legacy `&'static str` callers (eval store,
    // dashboard, CLI grep) don't recognise yet:
    //
    // - `provider_decode` (model-dispatch decode failure) and
    //   `invalid_json` (trader-output decode failure) both map to
    //   `FailureClass::MalformedJson`. Re-discriminate here on the
    //   error-string shape so the legacy tag is unchanged.
    // - `tool_timeout`, `context_overflow`, and `empty_data` are new
    //   tags F-5 introduces. Do NOT emit them through the legacy
    //   `&'static str` surface — project back to the pre-F-5 tag the
    //   string would have produced. The typed `FailureClass` remains
    //   available to the recovery dispatcher for the playbook
    //   decision; only the persisted `[<class>]` wire prefix is held
    //   stable.
    let class = recovery::classify(err);
    if matches!(class, recovery::FailureClass::MalformedJson { .. }) {
        let s = format!("{:#}", err).to_lowercase();
        if s.contains("trader_output[invalid_json]") || err.is::<TraderOutputError>() {
            return "invalid_json";
        }
        return "provider_decode";
    }
    if matches!(class, recovery::FailureClass::ToolTimeout { .. }) {
        return "unclassified";
    }
    if matches!(class, recovery::FailureClass::ContextOverflow { .. }) {
        // Pre-F-5: `anthropic api error: context_length_exceeded`
        // matched the `anthropic api error` branch first and emitted
        // `provider_http_error`. Other shapes (`maximum context
        // length`, `context window`, `... too long ...`) fell through
        // to `unclassified`. Re-discriminate on the same needle.
        let s = format!("{:#}", err).to_lowercase();
        if s.contains("anthropic api error") || s.contains("openai-compat api error") {
            return "provider_http_error";
        }
        return "unclassified";
    }
    if matches!(class, recovery::FailureClass::EmptyData { .. }) {
        // Pre-F-5: empty-data strings fell through to `unclassified`.
        return "unclassified";
    }
    class.tag()
}

/// Format the persisted/displayed failure string for a run error. The
/// `[<class>] ` prefix is the stable wire shape downstream consumers parse.
///
/// Uses anyhow's alternate `Display` (`{:#}`) so the underlying broker
/// rejection / provider error / etc. is preserved alongside any outer
/// `with_context` wrapper instead of being collapsed to the outermost
/// message.
pub(crate) fn format_failure_reason(err: &anyhow::Error) -> String {
    let class = classify_run_failure(err);
    let raw = format!("{:#}", err);
    if raw.starts_with(&format!("[{class}]")) {
        raw
    } else {
        format!("[{class}] {raw}")
    }
}

#[async_trait]
pub trait Executor: Send + Sync {
    /// Run the strategy against the scenario end-to-end. Mutates `run`
    /// in-place to reflect status transitions (Queued → Running → Completed
    /// or Failed) and the final `MetricsSummary`. Persists every decision
    /// + equity sample + the final metrics through `store`. Returns the
    /// computed `MetricsSummary` for callers that want the value without
    /// re-reading from the store.
    async fn run(
        &self,
        run: &mut Run,
        strategy: &Strategy,
        scenario: &Scenario,
        agent_slots: &[ResolvedAgentSlot],
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> anyhow::Result<MetricsSummary>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context as _;

    #[test]
    fn classify_broker_unsupported_routes_short_open_and_bracket_phrases() {
        let e1 = anyhow::anyhow!(
            "alpaca crypto broker_unsupported: short_open is not supported for BTC/USD (asset is not shortable on Alpaca crypto)"
        );
        assert_eq!(classify_run_failure(&e1), "broker_unsupported");

        let e2 = anyhow::anyhow!("alpaca create_order: bracket orders not supported for this asset class");
        assert_eq!(classify_run_failure(&e2), "broker_unsupported");

        let e3 = anyhow::anyhow!(
            "alpaca create_order: order_type market is not supported for this asset class"
        );
        assert_eq!(classify_run_failure(&e3), "broker_unsupported");
    }

    #[test]
    fn classify_broker_auth_routes_forbidden_and_not_permitted() {
        let e1 = anyhow::anyhow!("alpaca create_order: not permitted");
        assert_eq!(classify_run_failure(&e1), "broker_auth");

        let e2 = anyhow::anyhow!("alpaca get_account: forbidden");
        assert_eq!(classify_run_failure(&e2), "broker_auth");
    }

    #[test]
    fn classify_broker_insufficient_funds_routes_buying_power_phrases() {
        let e1 = anyhow::anyhow!("alpaca create_order: insufficient buying power for this order");
        assert_eq!(classify_run_failure(&e1), "broker_insufficient_funds");

        let e2 = anyhow::anyhow!("orderly: insufficient balance");
        assert_eq!(classify_run_failure(&e2), "broker_insufficient_funds");
    }

    #[test]
    fn classify_broker_rejected_routes_alpaca_order_rejected() {
        let e = anyhow::anyhow!("alpaca order 01H... rejected");
        assert_eq!(classify_run_failure(&e), "broker_rejected");
    }

    #[test]
    fn classify_broker_timeout_routes_fill_poll_exhaustion() {
        let e = anyhow::anyhow!("alpaca order 01H... did not fill within 5 polls");
        assert_eq!(classify_run_failure(&e), "broker_timeout");
    }

    #[test]
    fn classify_preserves_existing_provider_classes() {
        // Provider classes still route correctly after the broker_*
        // additions (no regression).
        let e_provider_to = anyhow::anyhow!("openrouter request timed out after 60s");
        assert_eq!(classify_run_failure(&e_provider_to), "provider_timeout");

        let e_provider_conn = anyhow::anyhow!("tcp connect: connection refused");
        assert_eq!(classify_run_failure(&e_provider_conn), "provider_connect");

        let e_provider_http = anyhow::anyhow!("anthropic api error: 500 internal server error");
        assert_eq!(classify_run_failure(&e_provider_http), "provider_http_error");

        let e_provider_decode =
            anyhow::anyhow!("provider_decode: anthropic returned invalid JSON response body: EOF while parsing a value at line 1707 column 0");
        assert_eq!(classify_run_failure(&e_provider_decode), "provider_decode");
    }

    #[test]
    fn classify_walks_anyhow_context_chain() {
        // The eval paper executor wraps broker errors with `with_context`
        // (`paper eval submit_order failed: …`). The outermost message has
        // no class hint, but the inner cause does — the classifier must
        // walk the chain to find it.
        let inner = anyhow::anyhow!("alpaca create_order: bracket orders not supported for this asset class");
        let wrapped: anyhow::Error = Err::<(), _>(inner)
            .context("paper eval submit_order failed: run_id=01H decision_index=0")
            .unwrap_err();
        assert_eq!(classify_run_failure(&wrapped), "broker_unsupported");
    }

    #[test]
    fn format_failure_reason_preserves_full_chain() {
        // `err.to_string()` only shows the outermost context; this test
        // pins the alternate-Display behaviour so the underlying Alpaca
        // rejection text reaches the operator.
        let inner = anyhow::anyhow!("alpaca create_order: not permitted");
        let wrapped: anyhow::Error = Err::<(), _>(inner)
            .context("paper eval submit_order failed: run_id=R decision_index=0")
            .unwrap_err();
        let formatted = format_failure_reason(&wrapped);
        assert!(
            formatted.starts_with("[broker_auth] "),
            "must carry the broker_auth class tag, got: {formatted}"
        );
        assert!(
            formatted.contains("paper eval submit_order failed"),
            "must keep the with_context wrapper, got: {formatted}"
        );
        assert!(
            formatted.contains("alpaca create_order: not permitted"),
            "must surface the inner broker error, got: {formatted}"
        );
    }

    #[test]
    fn format_failure_reason_does_not_double_prefix() {
        // If the underlying error already starts with `[class] `, the
        // prefix is not stacked.
        let pre_tagged = anyhow::anyhow!(
            "[broker_unsupported] alpaca crypto broker_unsupported: short_open is not supported for BTC/USD"
        );
        let formatted = format_failure_reason(&pre_tagged);
        assert!(
            formatted.starts_with("[broker_unsupported] "),
            "prefix appears exactly once, got: {formatted}"
        );
        assert!(
            !formatted.starts_with("[broker_unsupported] [broker_unsupported]"),
            "must not double-prefix, got: {formatted}"
        );
    }

    #[test]
    fn classify_unclassified_for_unrecognised_messages() {
        let e = anyhow::anyhow!("something completely unexpected went wrong");
        assert_eq!(classify_run_failure(&e), "unclassified");
    }

    #[test]
    fn classify_context_overflow_preserves_legacy_wire_format() {
        // Pre-F-5 wire-format pin (regression coverage for the typed
        // `FailureClass::ContextOverflow` projection added in F-5). The
        // typed classifier learns `context_overflow` as a new tag; the
        // legacy `&'static str` surface must keep emitting the tag the
        // pre-F-5 regex chain produced so the eval store / dashboard /
        // CLI grep keep parsing the persisted `[<class>]` prefix.
        let e_anthropic = anyhow::anyhow!(
            "anthropic api error: context_length_exceeded: max 200000 tokens"
        );
        assert_eq!(classify_run_failure(&e_anthropic), "provider_http_error");

        let e_generic = anyhow::anyhow!("model returned: maximum context length exceeded");
        assert_eq!(classify_run_failure(&e_generic), "unclassified");
    }

    #[test]
    fn classify_empty_data_preserves_legacy_unclassified() {
        // Pre-F-5: empty-data strings fell through to `unclassified`.
        // F-5 introduces a typed `EmptyData` class but the legacy
        // `&'static str` surface must hold the wire format stable.
        let e = anyhow::anyhow!("snapshot recent_bars is empty for run R, cycle 4");
        assert_eq!(classify_run_failure(&e), "unclassified");
    }
}
