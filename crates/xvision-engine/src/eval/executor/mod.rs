//! Executor trait + concrete impl. Phase 3.B of the Eval Engine plan,
//! amended 2026-05-22 (executor-collapse-paper-mode +
//! executor-live-shell sub-tracks of the Alpaca-Live executor refactor).
//!
//! A single concrete [`Executor`] struct covers both [`RunMode::Backtest`]
//! and [`RunMode::Live`] by composing the [`BarSource`] + [`Clock`] +
//! [`FillSink`] trio at construction:
//!
//! - **Backtest** — `InjectedBars` + `InstantClock` + `SimulatedFills`.
//!   Built via [`Executor::backtest`]. Replays a parquet fixture in
//!   chronological order, simulates fills with slippage + fees. No
//!   broker required.
//! - **Live** — `MultiLiveStream` + `WallClock` + `RealBrokerFills`.
//!   Built via [`Executor::live`]. Alpaca paper live is wired end-to-end
//!   for one or more assets: each active asset is fanned out into its own
//!   `LiveStream` and merged by `MultiLiveStream` (§4 L2 multi-asset
//!   fanout). A single active asset is a 1-element `MultiLiveStream`,
//!   behaviourally identical to the L1 single `LiveStream`.
//!
//! Callers (`engine::api::eval::run`, the eval CLI) pick a constructor
//! by `RunMode` and call [`RunExecutor::run`] once per `xvn eval run`
//! invocation.

pub mod asset_set;
pub mod attest_hook;
pub mod backtest;
pub mod book;
pub mod gated_broker;
pub mod live_session;
pub mod live_source;
pub mod real_broker_fills;
pub(crate) mod sltp;
pub mod trace_types;
pub mod trader_output;
pub mod traits;
pub mod wall_clock;

pub use gated_broker::GatedBrokerSurface;

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::eval::run::{MetricsSummary, Run};
use crate::eval::scenario::Scenario;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

pub use attest_hook::{clamp_every_n, is_attest_boundary, AttestHook, AttestSummary, NoopAttestHook};
pub use backtest::Executor;
pub use live_session::LiveSessionTracker;
pub use live_source::{LiveStream, LiveStreamError, MultiLiveStream, TaggedBar};
pub use real_broker_fills::RealBrokerFills;
pub use trace_types::{
    AggressorSide, DecisionTrace, FeeSource, FillBranch, FillTrace, ToolCall, DECISIONS_SCHEMA_VERSION,
};
pub use trader_output::{TraderFailureKind, TraderOutputError};
pub use traits::{
    BarSource, Clock, FillRecord, FillRequest, FillSink, InjectedBars, InstantClock, SimulatedFills,
};
pub use wall_clock::WallClock;

use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use crate::eval::watchdog::{self, WatchdogConfig};

/// Engine-side lifecycle hook for the eval-run watchdog. Callers
/// (`xvision-dashboard::serve`, the long-running CLI daemon) invoke
/// this once during startup. It performs the one-shot boot sweep
/// synchronously (so any pre-existing stuck rows are finalized before
/// the API starts serving traffic) and then spawns the periodic task
/// for the lifetime of the process.
///
/// Returns the [`JoinHandle`] of the background task so the caller can
/// abort it on shutdown. The handle is `Send + 'static` and safe to
/// store in app state.
///
/// # Errors
///
/// Returns the underlying DB error from the boot sweep. If the boot
/// sweep fails the periodic task is *not* spawned — the caller decides
/// whether to surface this as a fatal startup error or downgrade to a
/// warning.
pub async fn start_watchdog(pool: SqlitePool, config: WatchdogConfig) -> anyhow::Result<JoinHandle<()>> {
    let store = crate::eval::store::RunStore::new(pool.clone());
    watchdog::boot_sweep(&pool, &store, &config).await?;
    Ok(watchdog::spawn(pool, config))
}

/// Stable failure-class tag for a run-level error. Paper/backtest executors
/// prefix the persisted `eval_runs.error` string with `[<class>]` so review
/// and UI consumers can read the class without re-parsing the full message.
///
/// Classes:
///  - Trader output classes: `empty`, `tool_use_only`, `truncated`,
///    `invalid_json`, `missing_field`, `invalid_field`, `missing_response`.
///  - Provider transport classes: `provider_timeout`, `provider_connect`,
///    `provider_http_error`, `provider_decode`, `provider_rate_limited`,
///    `provider_missing_choices` (track
///    `eval-provider-error-classify-retry`, intake #344). The last two
///    are produced as typed `OpenAiCompatError` variants after the
///    dispatcher exhausts its retry budget; they're surfaced to review
///    & UI consumers via the `[<class>]` prefix on `eval_runs.error`.
///  - Broker transport classes: `broker_auth`, `broker_unsupported`,
///    `broker_insufficient_funds`, `broker_timeout`, `broker_rejected`.
///  - Loop-control classes: `repeated_broker_error` (eval circuit
///    breaker tripped — N consecutive identical recoverable broker
///    rejections in a row; added by
///    `eval-broker-error-circuit-breaker`).
///  - `unclassified` for anything else.
///
/// The matcher walks the full `anyhow::Error` source chain (via the alternate
/// `Display`) so an underlying broker rejection survives a `with_context`
/// wrap from the surface caller.
///
/// Since F-5 (`harness-recovery-state-machine`) this is a thin adapter
/// over [`crate::agent::recovery::classify`], which returns a typed
/// [`crate::agent::recovery::FailureClass`]. The wire-side `&'static str`
/// tag is `FailureClass::tag()` — preserved across the F-5 cutover so
/// `eval_runs.error` `[<tag>]` consumers do not break.
pub fn classify_run_failure(err: &anyhow::Error) -> &'static str {
    crate::agent::recovery::classify(err).tag()
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

/// Engine-side run-driver trait. A single implementor — [`Executor`] —
/// dispatches both Backtest and Live mode by composing the right
/// `BarSource` + `Clock` + `FillSink` trio at construction.
///
/// The trait is named `RunExecutor` to avoid colliding with the
/// concrete struct [`Executor`]; legacy doc comments referring to "the
/// Executor trait" mean this surface.
#[async_trait]
pub trait RunExecutor: Send + Sync {
    /// Run the strategy against the scenario end-to-end. Mutates `run`
    /// in-place to reflect status transitions (Queued → Running → Completed
    /// or Failed) and the final `MetricsSummary`. Persists every decision
    /// + equity sample + the final metrics through `store`. Returns the
    /// computed `MetricsSummary` for callers that want the value without
    /// re-reading from the store.
    #[allow(clippy::too_many_arguments)]
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

        let e3 =
            anyhow::anyhow!("alpaca create_order: order_type market is not supported for this asset class");
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
    fn classify_repeated_broker_error_routes_circuit_breaker_messages() {
        // eval-broker-error-circuit-breaker: the abort message contains
        // `repeated_broker_error:` and the inner broker class tag (e.g.
        // `broker_min_order_size`). The outer class wins.
        let e = anyhow::anyhow!(
            "repeated_broker_error: aborted after 3 consecutive broker_min_order_size rejections; \
             run_id=R decision_index=2 asset=BTC/USD last_error=cost basis must be >= minimal amount of order 10"
        );
        assert_eq!(classify_run_failure(&e), "repeated_broker_error");

        // Same message wrapped in `with_context` (matches how the
        // executor surfaces the error to the outer harness):
        let inner = anyhow::anyhow!(
            "repeated_broker_error: aborted after 3 consecutive broker_min_order_size rejections; \
             run_id=R decision_index=2 asset=BTC/USD last_error=cost basis must be >= minimal amount of order 10"
        );
        let wrapped: anyhow::Error = Err::<(), _>(inner)
            .context("paper eval run terminated")
            .unwrap_err();
        assert_eq!(classify_run_failure(&wrapped), "repeated_broker_error");
    }

    #[test]
    fn classify_unclassified_for_unrecognised_messages() {
        let e = anyhow::anyhow!("something completely unexpected went wrong");
        assert_eq!(classify_run_failure(&e), "unclassified");
    }
}
