//! BacktestExecutor → ProgressBus integration tests. Mirrors the
//! PaperExecutor coverage in `eval_progress.rs` (PR #35) but drives a
//! real fixture replay end-to-end and asserts every event type the
//! BacktestExecutor is responsible for emitting fires at least once.
//!
//! As with the paper-side test, subscribers MUST subscribe BEFORE the
//! executor runs — broadcast doesn't replay, so a late subscribe loses
//! the RunStarted event.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::SqlitePool;
use xvision_core::market::Ohlcv;
use xvision_data::fixtures::ensure_test_fixture;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{BacktestExecutor, Executor};
use xvision_engine::eval::progress::{ProgressBus, ProgressEvent};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.6,"justification":"backtest progress test"}"#,
    ))
}

fn invalid_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo("definitely not json"))
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "backtest-progress-test strategy".into(),
            plain_summary: "for eval::progress backtest tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

fn daily_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 100.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 250.0,
                low: px - 250.0,
                close: px + 50.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

#[tokio::test]
async fn backtest_executor_runs_30_day_fixture_without_200_bar_warmup() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let mut strategy = build_strategy("01TESTSTRATEGYWARMUP000000001");
    strategy.manifest.decision_cadence_minutes = 1_440;

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(30);
    let first_bar_ts = bars[0].timestamp;
    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_bars(bars);

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("30 daily bars should not require 200 warmup bars");

    assert_eq!(
        metrics.n_decisions, 29,
        "30 bars should produce one decision for each bar with a next-open fill"
    );
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len() as u32, metrics.n_decisions);
    assert_eq!(
        decisions.first().map(|d| d.timestamp),
        Some(first_bar_ts),
        "replay should start on the first bar instead of skipping a 200-bar warmup"
    );
}

#[tokio::test]
async fn backtest_executor_emits_all_progress_event_types() {
    // Idempotent — generates the synthetic fixture if missing.
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGBT00000001");

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Subscribe BEFORE running so RunStarted isn't lost. A backtest at
    // 60-min cadence over a flash-crash window emits ~hundreds of ticks
    // — size the buffer generously so the receiver never lags.
    let bus = ProgressBus::new(16384);
    let mut rx = bus.subscribe();
    let tx = bus.sender();

    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_progress(tx);

    let result = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await;
    assert!(result.is_ok(), "backtest run should succeed: {:?}", result.err());

    // Drain the bus.
    use tokio::sync::broadcast::error::TryRecvError;
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => events.push(ev),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(n)) => panic!("bus lagged by {n} — bump capacity"),
        }
    }
    assert!(!events.is_empty(), "bus produced no events");

    let mut saw_started = false;
    let mut saw_tick = false;
    let mut saw_decision = false;
    let mut saw_fill = false;
    let mut saw_metrics = false;
    let mut saw_completed = false;
    for ev in &events {
        match ev {
            ProgressEvent::RunStarted { run_id, .. } => {
                assert_eq!(run_id, &run.id);
                saw_started = true;
            }
            ProgressEvent::RunTick {
                run_id,
                scenario_progress_pct,
                ..
            } => {
                assert_eq!(run_id, &run.id);
                assert!(
                    *scenario_progress_pct >= 0.0 && *scenario_progress_pct <= 100.0,
                    "out-of-range progress {scenario_progress_pct}",
                );
                saw_tick = true;
            }
            ProgressEvent::DecisionEmitted { run_id, action, .. } => {
                assert_eq!(run_id, &run.id);
                // The mock dispatch returns `long_open` so every cycle
                // should produce that action.
                assert_eq!(action, "long_open");
                saw_decision = true;
            }
            ProgressEvent::FillRecorded { run_id, .. } => {
                assert_eq!(run_id, &run.id);
                saw_fill = true;
            }
            ProgressEvent::MetricsUpdated {
                run_id, drawdown_pct, ..
            } => {
                assert_eq!(run_id, &run.id);
                assert!(
                    *drawdown_pct >= 0.0,
                    "drawdown_pct should be non-negative, got {drawdown_pct}",
                );
                saw_metrics = true;
            }
            ProgressEvent::RunCompleted { run_id, metrics, .. } => {
                assert_eq!(run_id, &run.id);
                assert!(metrics.n_decisions > 0);
                saw_completed = true;
            }
            _ => {}
        }
    }
    assert!(saw_started, "no RunStarted in {} events", events.len());
    assert!(saw_tick, "no RunTick in {} events", events.len());
    assert!(saw_decision, "no DecisionEmitted in {} events", events.len());
    assert!(saw_fill, "no FillRecorded in {} events", events.len());
    assert!(saw_metrics, "no MetricsUpdated in {} events", events.len());
    assert!(saw_completed, "no RunCompleted in {} events", events.len());
}

#[tokio::test]
async fn backtest_executor_emits_run_failed_on_unparseable_trader_output() {
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTBUNDLEPROGBT00000004");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bus = ProgressBus::new(1024);
    let mut rx = bus.subscribe();
    let tx = bus.sender();

    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_progress(tx);
    let err = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            invalid_dispatch(),
            tools,
            &store,
        )
        .await
        .expect_err("invalid trader JSON must fail the backtest run");
    assert!(
        err.to_string().contains("invalid JSON"),
        "unexpected error: {err}"
    );

    use tokio::sync::broadcast::error::TryRecvError;
    let mut saw_failed = false;
    let mut saw_completed = false;
    loop {
        match rx.try_recv() {
            Ok(ProgressEvent::RunFailed { run_id, error }) => {
                assert_eq!(run_id, run.id);
                assert!(error.contains("invalid JSON"), "unexpected error: {error}");
                saw_failed = true;
            }
            Ok(ProgressEvent::RunCompleted { .. }) => saw_completed = true,
            Ok(_) => {}
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(n)) => panic!("bus lagged by {n}"),
        }
    }
    assert!(saw_failed, "expected RunFailed event");
    assert!(
        !saw_completed,
        "RunCompleted must not be emitted on parse failure"
    );
}

#[tokio::test]
async fn backtest_executor_runs_clean_with_no_progress_subscriber() {
    // Sanity: passing `with_progress` but having NO active subscriber
    // must not crash the run. broadcast::Sender::send returns Err when
    // there are no receivers; the executor swallows it silently.
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGBT00000002");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bus = ProgressBus::new(8);
    let tx = bus.sender();
    drop(bus); // drop the bus's anchor receiver; tx still lives

    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_progress(tx);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run should still succeed without a subscriber");
}

#[tokio::test]
async fn backtest_executor_default_constructor_is_silent() {
    // Pre-PR callers used `BacktestExecutor` (unit struct). Post-PR,
    // `BacktestExecutor::new()` is the equivalent — confirm it still
    // runs to completion with no progress wiring.
    ensure_test_fixture("scenario-flash-crash-2024-08").unwrap();

    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGBT00000003");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::new();

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("BacktestExecutor::new() should run to completion");
}
