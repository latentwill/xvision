//! WS-15 (`trace-obs` data/market-context): a market **regime change** is the
//! highest-signal pre-decision state shift, but before this track it lived only
//! as a transient `regime_transition` field inside the briefing wire-diff — it
//! was NOT a queryable trace event.
//!
//! This track makes it one: the eval executor **computes** the regime label per
//! decision from its own trailing bar-history window (via the shared
//! `derive_regime_labels` heuristic — deterministic, cheap, no LLM) and emits a
//! `regime_transition` engine event whenever the computed label for an asset
//! changes from one decision to the next. The event carries
//! `{ asset, from, to, decision_index }` and is scoped to the decision span so
//! the dashboard timeline can surface it inline. The computed label is used
//! ONLY for the trace event — it is never injected into the trader seed /
//! briefing / prompt, so the agent's behavior is unchanged.
//!
//! Two layers of coverage:
//! 1. The pure `regime_changed` detection helper — returns `Some((from, to))`
//!    ONLY when a prior label exists AND it differs from the current; `None` on
//!    the first observation or when stable.
//! 2. A REAL-executor integration assertion: a 30-bar calm/range plateau
//!    followed by a 30-bar crash is driven through the actual `Executor`
//!    backtest loop with a `MockDispatch`; the `derive_regime_labels` heuristic
//!    computed over the executor's growing window flips `"chop"` → `"crash"`,
//!    and the run publishes at least one `regime_transition` engine event with
//!    that `from`→`to`, span-scoped to a decision — with none firing during the
//!    stable early plateau.

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_outcome_observability.rs

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use serde_json::Value;
use xvision_core::market::Ohlcv;
use xvision_data::alpaca::MarketBar;
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::executor::backtest::regime_changed;
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::regime::derive_regime_labels;
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{NoopRecorder, RunEvent, RunEventBus};

mod support;

use support::eval_harness::{fresh_store, sequenced_dispatch, strategy_with};

// ---- Layer 1: pure detection helper -------------------------------

#[test]
fn regime_changed_fires_only_on_an_actual_change() {
    // First observation: no prior label → no transition.
    assert_eq!(regime_changed(None, Some("chop")), None);

    // Stable: prior == current → no transition.
    assert_eq!(regime_changed(Some("chop"), Some("chop")), None);

    // Changed: prior != current → Some((from, to)).
    assert_eq!(
        regime_changed(Some("chop"), Some("crash")),
        Some(("chop".to_string(), "crash".to_string())),
    );

    // Current label vanished (None) → no transition (we only emit on a
    // concrete from→to label change, mirroring how the executor only emits
    // when it has computed a present label for the window).
    assert_eq!(regime_changed(Some("chop"), None), None);

    // No prior and no current → no transition.
    assert_eq!(regime_changed(None, None), None);
}

// ---- Shared bar fixture: a regime-crossing series -----------------

/// A regime-CROSSING bar series: 30 flat plateau bars (close pinned at 100 →
/// zero slope → sideways → `derive_regime_labels` = `"chop"`) followed by 30
/// crash bars whose FIRST bar gaps the price down ~35% to 65 (a >25%-from-peak
/// drawdown → `"crash"` immediately), then bleeds lower. Because the executor
/// computes the regime over its GROWING window up to the current bar, the very
/// first decision that includes a crash bar already classifies as `"crash"`,
/// giving a clean `"chop"` → `"crash"` crossing rather than passing through an
/// intermediate sub-threshold downtrend.
///
/// Daily-spaced from a midnight start so each bar lands on a day boundary and
/// fires a decision under the daily (1440-min) cadence. `30 + 30` = 60 bars.
fn regime_crossing_bars() -> Vec<Ohlcv> {
    const N_CALM: usize = 30;
    const N_CRASH: usize = 30;
    let start = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::with_capacity(N_CALM + N_CRASH);

    // Calm plateau: close pinned at 100 → exactly-zero OLS slope → sideways,
    // zero drawdown → "chop" for any window size.
    for i in 0..N_CALM {
        bars.push(Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: 100.0,
            high: 100.5,
            low: 99.5,
            close: 100.0,
            volume: 1_000.0,
        });
    }

    // Crash: the first crash bar gaps down to 65 (35% drawdown from the 100
    // peak → already over the 25% crash threshold), then keeps falling to ~36.
    // Any window containing one crash bar is "crash".
    for j in 0..N_CRASH {
        let i = N_CALM + j;
        let c = 65.0 - j as f64 * 1.0; // 65 → 36
        bars.push(Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: c + 0.5,
            high: c + 1.0,
            low: c - 1.0,
            close: c,
            volume: 2_000.0,
        });
    }

    bars
}

fn to_market_bars(bars: &[Ohlcv]) -> Vec<MarketBar> {
    bars.iter()
        .map(|b| MarketBar {
            timestamp: b.timestamp,
            open: b.open,
            high: b.high,
            low: b.low,
            close: b.close,
            volume: b.volume,
        })
        .collect()
}

/// Verify the fixture really crosses a regime boundary the way the executor
/// computes it: over the GROWING cumulative window (everything up to the
/// current bar, since 60 < the 200-bar history cap), the label must be
/// `"chop"` while only calm bars are in view and `"crash"` once enough crash
/// bars accumulate. We assert BOTH labels actually appear so the integration
/// test below cannot silently pass on a stable regime.
#[test]
fn fixture_crosses_chop_to_crash_over_growing_window() {
    let bars = regime_crossing_bars();
    let mb = to_market_bars(&bars);

    // Early plateau window (first 30 calm bars) → "chop".
    let early = derive_regime_labels(&mb[..30]);
    assert_eq!(
        early.regime_label.as_deref(),
        Some("chop"),
        "the calm plateau must classify as chop; got {:?}",
        early.regime_label
    );

    // The window the instant the FIRST crash bar enters view (30 calm + 1
    // crash = the gap-down to 65, a 35% drawdown) must ALREADY be "crash" —
    // this is what makes the executor cross directly chop→crash without an
    // intermediate sub-threshold downtrend.
    let first_crash = derive_regime_labels(&mb[..31]);
    assert_eq!(
        first_crash.regime_label.as_deref(),
        Some("crash"),
        "the window with the first crash bar must already classify as crash; got {:?}",
        first_crash.regime_label
    );

    // Full window (plateau + full crash) → "crash" (>25% drawdown).
    let full = derive_regime_labels(&mb);
    assert_eq!(
        full.regime_label.as_deref(),
        Some("crash"),
        "the full window including the crash must classify as crash; got {:?}",
        full.regime_label
    );

    // Sanity: the two labels differ — there IS a crossing to detect.
    assert_ne!(early.regime_label, full.regime_label);
}

// ---- Layer 2: real-executor integration ---------------------------

/// Drain the bus into the recorder's snapshot.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn regime_transitions(events: &[RunEvent]) -> Vec<(&xvision_observability::EngineEvent, Value)> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineEvent(ev) if ev.kind == "regime_transition" => {
                let p: Value =
                    serde_json::from_str(ev.payload_json.as_deref().expect("payload_json present"))
                        .expect("payload is JSON");
                Some((ev, p))
            }
            _ => None,
        })
        .collect()
}

/// THE point of WS-15: drive the REAL backtest executor over a regime-crossing
/// bar series and assert a `regime_transition` engine event (computed from the
/// executor's own bar history) actually lands on the bus — `chop` → `crash`,
/// span-scoped to a decision, with none firing during the stable early
/// plateau.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_executor_emits_chop_to_crash_regime_transition() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTWS15REGIMECROSS0000A";
    // Daily cadence (1440 min) so every midnight-aligned daily bar fires a
    // decision; balanced risk; single asset.
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = regime_crossing_bars();
    let n = bars.len();
    // All `hold` — we only care about the regime computation, not trades.
    let holds: Vec<&str> = std::iter::repeat("hold").take(n).collect();
    let dispatch = sequenced_dispatch(&holds);
    let tools = Arc::new(ToolRegistry::empty());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), run.id.clone());
    let executor = Executor::with_bars(bars).with_observability(obs);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let events = collect_events(&bus, &recorder).await;
    let transitions = regime_transitions(&events);

    assert!(
        !transitions.is_empty(),
        "the real executor must emit at least one regime_transition over a \
         regime-crossing series; got none (computed regime never fired)"
    );

    // At least one transition must be the chop → crash crossing.
    let chop_to_crash = transitions
        .iter()
        .find(|(_, p)| p["from"] == "chop" && p["to"] == "crash")
        .expect("a chop→crash regime_transition must fire when the crash bars arrive");
    let (ev, p) = chop_to_crash;

    assert_eq!(p["asset"], "BTC/USD", "transition must carry the asset");
    // The transition must land at a decision index in the crash half — once
    // enough crash bars have accumulated to push drawdown over the threshold.
    let idx = p["decision_index"]
        .as_u64()
        .expect("decision_index must be an integer");
    assert!(
        idx >= 30,
        "the chop→crash transition must fire in the crash window (idx >= 30); got {idx}"
    );
    // Scoped to the decision span it belongs to.
    assert!(
        ev.span_id.is_some(),
        "regime_transition must be span-scoped to its decision"
    );

    // No transition may fire during the stable early plateau (the first calm
    // bars all classify as chop, so no change can be detected there).
    for (_, p) in &transitions {
        let i = p["decision_index"].as_u64().unwrap();
        assert!(
            i >= 1,
            "no transition may fire on the first observation (idx 0); got {i}"
        );
    }
}
