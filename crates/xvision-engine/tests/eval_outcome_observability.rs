//! WS-14 — outcome & exit observability for the BACKTEST executor.
//!
//! Three emit-only facets, all routed through the existing
//! `engine_event` / `broker.call` channels (no schema migration, no new
//! SSE wiring), captured here via an in-memory `NoopRecorder` snapshot:
//!
//! 1. `position_exit` engine event — a deterministic SL/TP/trailing/time
//!    exit (often THE realized-PnL event) now emits a typed event with
//!    `{ asset, exit_reason, effective_sl_price, effective_tp_price,
//!       realized_pnl, exit_price }`. Before WS-14 these exits emitted
//!    nothing (DB row + chart blip only).
//! 2. `decision_completed` enrichment — the per-decision PnL/position arc
//!    (`equity_delta`, `realized_pnl`, `cumulative_realized`,
//!    `unrealized_pnl`, plus the existing `position_pre`/`position_post`)
//!    now rides the engine event, not just the chart SSE.
//! 3. backtest `broker.call` span — the simulated fill path now emits the
//!    same typed `broker.call` span as the live path, stamped with the
//!    `backtest` venue, so backtest fills are auditable on the trace dock.
//!
//! Reuses the eval-harness scaffolding in `tests/support/eval_harness.rs`
//! plus the exit-enforcement bar/dispatch patterns from
//! `tests/eval_exit_enforcement.rs`.

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_exit_enforcement.rs

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use serde_json::Value;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{NoopRecorder, RunEvent};
use xvision_observability::{RunEventBus, SpanKind};

mod support;

use support::eval_harness::{fresh_store, sequenced_dispatch, strategy_with};

/// A `long_open` carrying an explicit `take_profit_pct` bracket, followed by
/// `hold`s. Drives a deterministic winning round-trip (TP exit).
fn long_open_with_tp_then_holds(tp_pct: f64, holds: usize) -> Arc<dyn LlmDispatch> {
    let open = LlmResponse {
        content: vec![ContentBlock::Text {
            text: format!(
                r#"{{"action":"long_open","conviction":0.8,"justification":"breakout","take_profit_pct":{tp_pct}}}"#
            ),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };
    let mut resps = vec![open];
    for _ in 0..holds {
        resps.push(LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"hold","conviction":0.5,"justification":"wait"}"#.into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        });
    }
    Arc::new(MockDispatch::sequence(resps))
}

/// Drain the bus into the recorder's snapshot.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn engine_events<'a>(events: &'a [RunEvent], kind: &str) -> Vec<&'a xvision_observability::EngineEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineEvent(ev) if ev.kind == kind => Some(ev),
            _ => None,
        })
        .collect()
}

fn payload(ev: &xvision_observability::EngineEvent) -> Value {
    serde_json::from_str(ev.payload_json.as_deref().expect("payload_json present")).expect("payload is JSON")
}

/// TP-exit bar/dispatch builder shared by the position_exit + decision
/// enrichment tests. Flat ~100 history, a rally bar that trips the +6% TP,
/// then a winning fill bar. Entry fills at the bar after `long_open` (~100).
fn tp_exit_setup() -> (Vec<Ohlcv>, Arc<dyn LlmDispatch>) {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..6 {
        let (o, h, l, c) = if i == 4 {
            // Rally bar: high well above the +6% TP (106).
            (100.0, 112.0, 100.0, 110.0)
        } else if i == 5 {
            // The SLTP path fills at the next bar's open; keep it above entry.
            (110.0, 112.0, 109.0, 110.0)
        } else {
            (100.0, 101.0, 99.0, 100.0)
        };
        bars.push(Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: 1_000.0,
        });
    }
    (bars, long_open_with_tp_then_holds(6.0, 6))
}

/// 1. A deterministic take-profit exit emits a `position_exit` engine event
///    carrying the exit reason + this exit's realized PnL.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sltp_take_profit_exit_emits_position_exit_event() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTWS14POSITIONEXIT0000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let (bars, dispatch) = tp_exit_setup();
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
    let exits = engine_events(&events, "position_exit");
    assert_eq!(
        exits.len(),
        1,
        "exactly one position_exit must fire for the single TP round-trip"
    );
    let p = payload(exits[0]);
    assert_eq!(p["asset"], "BTC/USD");
    assert_eq!(
        p["exit_reason"], "take_profit",
        "exit_reason must carry the SltpTrigger reason in snake_case"
    );
    // This exit booked a positive realized PnL (entry ~100, exit ~110).
    let realized = p["realized_pnl"].as_f64().expect("realized_pnl must be a number");
    assert!(
        realized > 0.0,
        "TP exit must record a positive realized_pnl; got {realized}"
    );
    // The effective bracket prices the exit fired against are present.
    assert!(
        p.get("effective_tp_price").is_some(),
        "position_exit must carry effective_tp_price"
    );
    assert!(
        p.get("effective_sl_price").is_some(),
        "position_exit must carry effective_sl_price"
    );
    assert!(
        p["exit_price"].as_f64().is_some(),
        "position_exit must carry the exit_price"
    );
    // The event is scoped to the decision span it belongs to.
    assert!(
        exits[0].span_id.is_some(),
        "position_exit must be span-scoped to its decision"
    );
}

/// 2. The `decision_completed` engine event carries the per-decision
///    PnL/position arc (equity_delta / cumulative_realized / unrealized_pnl)
///    in addition to the pre/post position already present.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn decision_completed_carries_pnl_and_position_arc() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTWS14DECISIONARC00000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let (bars, dispatch) = tp_exit_setup();
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
    let completed = engine_events(&events, "decision_completed");
    assert!(
        !completed.is_empty(),
        "at least one decision_completed event must fire"
    );

    // Every decision_completed payload must carry the enrichment keys.
    for ev in &completed {
        let p = payload(ev);
        for key in [
            "position_pre",
            "position_post",
            "equity_delta",
            "realized_pnl",
            "cumulative_realized",
            "unrealized_pnl",
        ] {
            assert!(
                p.get(key).is_some(),
                "decision_completed payload must carry `{key}`; payload = {p}"
            );
        }
    }

    // The opening fill decision must show a position_post of a non-flat long
    // (entered the position) and a defined equity_delta + cumulative_realized.
    let opened = completed
        .iter()
        .map(|ev| payload(ev))
        .find(|p| p["position_post"].as_f64().unwrap_or(0.0).abs() > f64::EPSILON)
        .expect("one decision must end with a non-flat position (the open)");
    assert!(
        opened["equity_delta"].as_f64().is_some(),
        "equity_delta must be a number on the open decision"
    );
    assert!(
        opened["cumulative_realized"].as_f64().is_some(),
        "cumulative_realized must be a number"
    );
}

/// 4. WS-17 span taxonomy — the executor opens a `decision.model` span
///    (kind `decision.model`) as a CHILD of the per-bar `agent.decision`
///    span, around the model invocation that produces the trade decision.
///    This is the parent the captured `decision.reasoning` chain-of-thought
///    nests under (the reasoning-capture path itself only fires on the
///    Cline runtime; the nesting contract `agent.decision → decision.model
///    → decision.reasoning` is locked in
///    `decision_model_parents_a_captured_reasoning_span` below using the
///    emitter directly with the same parent-id threading the executor does).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn executor_emits_decision_model_span_child_of_agent_decision() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTWS17DECISIONMODEL000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let (bars, dispatch) = tp_exit_setup();
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

    // Index every SpanStarted by id so we can walk the parent chain.
    let started: std::collections::HashMap<String, &xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) => Some((s.span_id.clone(), s)),
            _ => None,
        })
        .collect();

    // At least one decision.model span must exist.
    let decision_models: Vec<&xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::DecisionModel) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        !decision_models.is_empty(),
        "executor must open a decision.model span around the trade-decision model call"
    );

    // The executor-owned decision.model span (the one this track adds) is a
    // CHILD of the per-bar agent.decision span. NB: the MockDispatch test
    // runtime also drives the retired LlmDispatch `execute_slot` path, which
    // emits its own (parentless) decision.model span — production's Cline
    // path does not. We assert the executor-owned, agent.decision-parented
    // span exists; that is the parent the reasoning span nests under.
    let executor_owned: Vec<&&xvision_observability::SpanStartedEvent> = decision_models
        .iter()
        .filter(|dm| {
            dm.parent_span_id
                .as_deref()
                .and_then(|pid| started.get(pid))
                .is_some_and(|parent| matches!(parent.kind, SpanKind::AgentDecision))
        })
        .collect();
    assert!(
        !executor_owned.is_empty(),
        "the executor's decision.model span must nest under an agent.decision span; \
         found decision.model parents = {:?}",
        decision_models
            .iter()
            .map(|dm| dm
                .parent_span_id
                .as_deref()
                .and_then(|pid| started.get(pid))
                .map(|p| format!("{:?}", p.kind)))
            .collect::<Vec<_>>()
    );

    // The executor-owned decision.model span is closed so the trace dock can
    // compute its duration (mirrors the broker.call / agent.decision lifecycle).
    for dm in &executor_owned {
        let finished = events
            .iter()
            .any(|e| matches!(e, RunEvent::SpanFinished(f) if f.span_id == dm.span_id));
        assert!(
            finished,
            "decision.model span {} must be closed (SpanFinished)",
            dm.span_id
        );
    }
}

/// WS-17 nesting contract: a captured `decision.reasoning` span nests
/// under `decision.model`, which nests under `agent.decision`. The
/// executor threads `decision_model_span_id` into the Cline dispatch as
/// the reasoning span's parent; here we reproduce that threading with
/// the emitter directly (the MockDispatch executor path never emits a
/// `<think>` block) so the full chain is locked regardless of runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn decision_model_parents_a_captured_reasoning_span() {
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let obs = ObsEmitter::new(bus.clone(), "run-ws17-chain".to_string());

    // agent.decision (top) → decision.model (child) → decision.reasoning.
    let decision_span = xvision_engine::agent::observability::fresh_span_id();
    let decision_model_span = xvision_engine::agent::observability::fresh_span_id();

    obs.emit_decision_span_started(&decision_span, None, 0, Some("BTC/USD"), None, None, None, None)
        .await;
    obs.emit_model_call_started(
        &decision_model_span,
        Some(decision_span.clone()),
        "anthropic",
        "claude-sonnet-4-6",
        Some("trader"),
        None,
        None,
    )
    .await;
    // The Cline strip site passes the threaded decision.model span id as
    // the reasoning span's parent.
    obs.emit_model_reasoning(
        Some(decision_model_span.clone()),
        "1h trend up, RSI crossed 30 — long looks favorable.",
    )
    .await;
    obs.emit_span_finished_ok(&decision_model_span).await;
    obs.emit_span_finished_ok(&decision_span).await;

    let events = collect_events(&bus, &recorder).await;
    let started: std::collections::HashMap<String, &xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) => Some((s.span_id.clone(), s)),
            _ => None,
        })
        .collect();

    // Find the decision.reasoning span and walk up two levels.
    let reasoning = events
        .iter()
        .find_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::DecisionReasoning) => Some(s),
            _ => None,
        })
        .expect("a decision.reasoning span must be emitted");

    let dm_id = reasoning
        .parent_span_id
        .as_deref()
        .expect("decision.reasoning must have a parent");
    let dm = started.get(dm_id).expect("decision.reasoning parent must exist");
    assert!(
        matches!(dm.kind, SpanKind::DecisionModel),
        "decision.reasoning must nest under decision.model; got {:?}",
        dm.kind
    );

    let ad_id = dm
        .parent_span_id
        .as_deref()
        .expect("decision.model must have a parent");
    let ad = started.get(ad_id).expect("decision.model parent must exist");
    assert!(
        matches!(ad.kind, SpanKind::AgentDecision),
        "decision.model must nest under agent.decision; got {:?}",
        ad.kind
    );
}

/// 3. A simulated (backtest) fill produces a `broker.call` span stamped with
///    the `backtest` venue, mirroring the live path's broker.call coverage.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backtest_fill_emits_broker_call_span_with_backtest_venue() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTWS14BROKERSPAN00000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Open a long then hold — the open fill is the broker.call we assert on.
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let bars: Vec<Ohlcv> = (0..6)
        .map(|i| Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        })
        .collect();
    let dispatch = sequenced_dispatch(&["long_open", "hold", "hold", "hold", "hold", "hold"]);
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

    // A broker.call span (SpanStarted with BrokerCall kind) must exist.
    let broker_spans: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::BrokerCall) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        !broker_spans.is_empty(),
        "a backtest fill must open a broker.call span"
    );

    // The typed BrokerCallStarted event must carry the `backtest` venue.
    let started: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::BrokerCallStarted(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        !started.is_empty(),
        "a backtest fill must emit a BrokerCallStarted event"
    );
    assert!(
        started.iter().any(|s| s.venue == "backtest"),
        "backtest broker.call venue must be `backtest`; got {:?}",
        started.iter().map(|s| s.venue.clone()).collect::<Vec<_>>()
    );

    // The span is closed (BrokerCallFinished) so the trace dock can compute
    // its duration — mirrors the live path.
    let finished: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::BrokerCallFinished(f) => Some(f),
            _ => None,
        })
        .collect();
    assert!(
        !finished.is_empty(),
        "a backtest fill must close the broker.call span with BrokerCallFinished"
    );
}
