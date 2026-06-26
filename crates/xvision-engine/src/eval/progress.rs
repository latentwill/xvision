//! Phase 3.D Task 13 — progress events emitted by the eval executors.
//!
//! Executors send a `ProgressEvent` after each significant action (a
//! decision, a fill, a tick, a metrics update). Subscribers — the CLI
//! progress bar today, the dashboard's SSE endpoint tomorrow — receive
//! the events through a `tokio::sync::broadcast` channel so multiple
//! consumers can observe the same run without coupling the executor to
//! any one transport.
//!
//! The bus is best-effort: if no subscribers are attached, sends are a
//! no-op (broadcast::Sender::send returns Err which `send_event` swallows).
//! If subscribers can't keep up, broadcast drops the oldest events for
//! that subscriber (`broadcast::error::RecvError::Lagged`); subscribers
//! handle that themselves.
//!
//! The dashboard SSE endpoint (Plan 2d) and the Executor wiring
//! (when Phase 3.B-backtest lands) consume the same `ProgressEvent`
//! shape. New event variants are additive — wire-compatible with older
//! subscribers because the enum is `#[serde(tag = "type")]` (unknown
//! tags fail-open with the catch-all if the consumer chooses; today
//! consumers just match on the variants they care about).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::eval::run::MetricsSummary;

/// Single event in an eval-run's lifecycle. The executor emits these via
/// a `ProgressTx`; the CLI / dashboard / autooptimizer subscribes via
/// a `ProgressBus`. New variants are additive; the enum is tagged so
/// the wire shape is JSON like `{"type": "run_started", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// Emitted once at the start of the run, before the first decision.
    RunStarted {
        run_id: String,
        /// Pre-run token estimate from the strategy's tokens module. 0 if
        /// the executor doesn't compute one (today: paper-mode-executor-deleted).
        estimated_tokens: u64,
    },
    /// One per scheduler tick. `scenario_progress_pct` is in [0.0, 100.0].
    /// `eta_secs` is an optional estimated time-to-completion in seconds;
    /// `None` for backtests (determinate from bar count) or when no bound
    /// is set. Live/forward-test runs compute this from the active
    /// `StopPolicy` constraints.
    RunTick {
        run_id: String,
        scenario_progress_pct: f64,
        current_ts: DateTime<Utc>,
        /// Estimated wall-clock seconds until the run reaches its stop
        /// condition (the minimum across all active policy limits).
        /// `None` means indeterminate (no active bound, or backtest).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        eta_secs: Option<f64>,
    },
    /// Emitted once per LLM-slot invocation. Phase 3.D-progress pares
    /// paper-mode-executor-deleted doesn't yet break out per-slot tokens, so this is
    /// reserved for the Executor + future per-slot
    /// instrumentation. paper-mode-executor-deleted does not emit this today.
    AgentFired {
        run_id: String,
        slot: String,
        tokens_used: u32,
    },
    /// Emitted after the trader output is parsed (regardless of whether
    /// the decision is actionable).
    DecisionEmitted {
        run_id: String,
        action: String,
        asset: String,
        size: f64,
        conviction: f64,
    },
    /// Emitted when an actionable decision results in a broker fill.
    FillRecorded {
        run_id: String,
        side: String,
        price: f64,
        qty: f64,
        fee: f64,
    },
    /// Emitted after each post-tick equity sample. `drawdown_pct` is the
    /// running max drawdown from peak observed so far in the run.
    ///
    /// CT5 (Epic s78 Wave 3) widens this with the per-run capital fields the
    /// `LiveDeploymentSummary` SSE `metrics` event carries — all derived in-loop
    /// from the book + the in-memory `LiveSessionTracker` (§6). The four CT5
    /// fields are `Option` and default `None`: the BACKTEST path leaves them
    /// `None` (no behavior change — backtests are NOT deployments), and an
    /// unsourceable value surfaces as `None` ("—" in the UI), NEVER a faked `0`
    /// (HONESTY MANDATE §8.1). Only the live loop populates them.
    MetricsUpdated {
        run_id: String,
        equity: f64,
        drawdown_pct: f64,
        n_trades: u32,
        /// Σ open-position notional (`PortfolioBook::open_legs()`). `None` on
        /// the backtest path / pre-first-fill.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deployed_capital_usd: Option<f64>,
        /// `book.equity(marks) - initial - book.realized()`. `None` on the
        /// backtest path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unrealized_pnl_usd: Option<f64>,
        /// `book.realized()`. `None` on the backtest path / no realized history.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        realized_pnl_usd: Option<f64>,
        /// Headroom before the enforced daily-loss kill (§6.2). `None` on the
        /// backtest path / no kill policy / no day baseline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        daily_loss_limit_remaining_usd: Option<f64>,
    },
    /// Reserved for the findings extractor (Phase 3.C). Executors do not
    /// emit this — the extractor publishes findings on the same bus so
    /// SSE consumers see them in-line with run events.
    FindingExtracted {
        run_id: String,
        kind: String,
        severity: String,
        evidence: String,
    },
    /// Terminal-success event. After this fires the executor exits.
    RunCompleted {
        run_id: String,
        metrics: MetricsSummary,
        tokens_used: u64,
    },
    /// Terminal-failure event. After this fires the executor exits.
    RunFailed { run_id: String, error: String },
    /// U5: periodic wall-clock heartbeat emitted from the top of the
    /// backtest decision loop at ~30s intervals (mirroring the
    /// `PARTIAL_PERSIST_INTERVAL` cadence-independent check). Lets a live
    /// subscriber — the CLI watch, the dashboard SSE, or the optimizer cycle
    /// re-emit bridge — observe that a long backtest is still making forward
    /// progress instead of treating a 10–20 minute silent stretch as a hang.
    /// `decisions` is the count of trader decisions emitted so far; `elapsed_s`
    /// is seconds since the decision loop started.
    EvalHeartbeat {
        run_id: String,
        decisions: u64,
        elapsed_s: u64,
    },
    /// U11: emitted when a filter wake is suppressed because a position is
    /// already open in the asset (`wake_when_in_position` gating →
    /// `SuppressedInPosition`). Lets live streams (CLI/dashboard) distinguish
    /// "filter never fired" from "filter fired but was blocked by an open
    /// position", which previously required reading the persisted
    /// `eval_filter_evaluations` ledger after the run. `reason` is a stable
    /// snake_case tag (today always `"in_position"`).
    FilterBlocked { run_id: String, reason: String },
    /// Emitted once per bar when a `FilterGated` strategy's runtime
    /// runs. Carries the activation decision and per-condition booleans
    /// so consumers (trace dock, CLI watch) can surface "plan touches"
    /// inline with the rest of the run timeline. Strategies in
    /// `EveryBar` mode never emit this variant.
    FilterEvaluated {
        run_id: String,
        bar_index: u64,
        ts: DateTime<Utc>,
        /// Stable decision tag from `xvision_filters::ActivationDecision::tag()`.
        decision_tag: String,
        /// Boolean per condition leaf (index aligns with the filter's
        /// flat condition list). Empty during warmup.
        conditions_passed: Vec<bool>,
        /// True iff the leaf-rollup evaluated to true on this bar
        /// (regardless of cooldown / cap / suppression).
        tree_true: bool,
        /// True only for the `false → true` transition of the rollup.
        /// Hold bars (sustained true) report `false`.
        trip: bool,
    },
}

/// Sender half of the progress channel. Cheap to clone (it's an `Arc`
/// internally). Pass to executors that want to emit events.
pub type ProgressTx = broadcast::Sender<ProgressEvent>;

/// Receiver half. Each call to `ProgressBus::subscribe` returns a fresh
/// receiver; broadcast events are fanned out to every active receiver.
pub type ProgressRx = broadcast::Receiver<ProgressEvent>;

/// Owned wrapper around a `tokio::sync::broadcast` channel. Holds a
/// dummy receiver internally so the channel stays open even when no
/// external subscribers are attached — without this, `Sender::send`
/// would fail before the first `subscribe()` call.
pub struct ProgressBus {
    tx: broadcast::Sender<ProgressEvent>,
    /// Internal "anchor" receiver so the channel doesn't close when all
    /// external subscribers drop. Held by the bus itself; new subscribers
    /// get fresh receivers from `subscribe()`.
    _anchor: broadcast::Receiver<ProgressEvent>,
}

impl ProgressBus {
    /// `capacity` is the broadcast channel's per-receiver buffer. Pick a
    /// value larger than the expected event-burst between subscriber
    /// polls; events past capacity are dropped for slow receivers and
    /// surface as `RecvError::Lagged`.
    pub fn new(capacity: usize) -> Self {
        let (tx, _anchor) = broadcast::channel(capacity);
        Self { tx, _anchor }
    }

    /// Returns the sender half. Cheap to clone for handing to executors.
    pub fn sender(&self) -> ProgressTx {
        self.tx.clone()
    }

    /// Returns a fresh receiver. Subscribers should subscribe BEFORE
    /// the executor runs to avoid losing the `RunStarted` event.
    pub fn subscribe(&self) -> ProgressRx {
        self.tx.subscribe()
    }
}

impl Default for ProgressBus {
    fn default() -> Self {
        // 1024 is plenty for a run that emits a few hundred ticks.
        Self::new(1024)
    }
}

/// Send an event, swallowing the "no receivers" error. Use this in
/// executors so a missing subscriber never aborts a run.
pub fn send_event(tx: &ProgressTx, event: ProgressEvent) {
    let _ = tx.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U5: EvalHeartbeat serializes as snake_case "eval_heartbeat".
    #[test]
    fn test_eval_heartbeat_wire_name() {
        let event = ProgressEvent::EvalHeartbeat {
            run_id: "r1".into(),
            decisions: 7,
            elapsed_s: 30,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "eval_heartbeat");
        assert_eq!(v["decisions"], 7);
        assert_eq!(v["elapsed_s"], 30);
        // round-trips
        let s = serde_json::to_string(&event).unwrap();
        let back: ProgressEvent = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, ProgressEvent::EvalHeartbeat { decisions: 7, .. }));
    }

    /// U11: FilterBlocked serializes as snake_case "filter_blocked" and carries
    /// the reason tag.
    #[test]
    fn test_filter_blocked_wire_name() {
        let event = ProgressEvent::FilterBlocked {
            run_id: "r1".into(),
            reason: "in_position".into(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "filter_blocked");
        assert_eq!(v["reason"], "in_position");
    }

    /// A subscriber attached before send receives the event; send with no
    /// subscribers is a no-op (does not panic).
    #[test]
    fn test_bus_delivers_and_tolerates_no_subscribers() {
        let bus = ProgressBus::new(8);
        let tx = bus.sender();
        // No external subscriber yet (anchor only) — must not panic.
        send_event(
            &tx,
            ProgressEvent::FilterBlocked {
                run_id: "r".into(),
                reason: "in_position".into(),
            },
        );
        let mut rx = bus.subscribe();
        send_event(
            &tx,
            ProgressEvent::EvalHeartbeat {
                run_id: "r".into(),
                decisions: 1,
                elapsed_s: 30,
            },
        );
        let got = rx
            .try_recv()
            .expect("subscriber receives event sent after subscribe");
        assert!(matches!(got, ProgressEvent::EvalHeartbeat { .. }));
    }
}
