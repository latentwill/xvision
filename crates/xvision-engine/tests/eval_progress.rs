//! Phase 3.D Task 13 — `eval::progress` integration tests.
//!
//! Drives `paper-mode-executor-deleted` end-to-end with a `MockBrokerSurface` +
//! `MockDispatch` that always echoes a `long_open` decision, subscribes
//! to the `ProgressBus` ahead of time, and asserts every event type the
//! executor is responsible for emitting fires at least once.
//!
//! The bus uses a `tokio::sync::broadcast` channel under the hood, so
//! subscribers must subscribe BEFORE the executor runs to avoid losing
//! the `RunStarted` event (broadcast doesn't replay).

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use chrono::Duration;
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::progress::{ProgressBus, ProgressEvent};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::eval::Scenario;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
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
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
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
        r#"{"action":"long_open","conviction":0.8,"justification":"mock"}"#,
    ))
}

fn invalid_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo("definitely not json"))
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "progress-test strategy".into(),
            plain_summary: "for eval::progress tests".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,

            min_warmup_bars: None,

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn scenario_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    while ts < scenario.time_window.end {
        let close = 60_000.0 + i * 25.0;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 10.0,
            high: close + 20.0,
            low: close - 30.0,
            close,
            volume: 250.0 + i,
        });
        ts += Duration::hours(1);
        i += 1.0;
    }
    bars
}

#[tokio::test]
async fn paper_executor_emits_run_failed_on_unparseable_trader_output() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGRESS00000C");

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bus = ProgressBus::new(1024);
    let mut rx = bus.subscribe();
    let tx = bus.sender();

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock_broker;
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars_and_progress(scenario_bars(&scenario), tx);

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
        .expect_err("invalid trader JSON must fail the paper run");
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
async fn paper_executor_emits_all_progress_event_types() {
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGRESS00000A");

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Subscribe BEFORE running so RunStarted isn't lost.
    let bus = ProgressBus::new(8192);
    let mut rx = bus.subscribe();
    let tx = bus.sender();

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock_broker;
    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars_and_progress(scenario_bars(&scenario), tx);

    let result = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await;
    assert!(result.is_ok(), "paper run should succeed: {:?}", result.err());

    // Drain the bus. broadcast::Receiver::try_recv returns Empty / Closed
    // / Lagged — we only care about Empty (we're done), and we treat
    // Lagged as "got too many events" which is fine for this test (we
    // sized the channel large to avoid this anyway).
    let mut events = Vec::new();
    use tokio::sync::broadcast::error::TryRecvError;
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
            ProgressEvent::RunTick { run_id, .. } => {
                assert_eq!(run_id, &run.id);
                saw_tick = true;
            }
            ProgressEvent::DecisionEmitted { run_id, action, .. } => {
                assert_eq!(run_id, &run.id);
                assert_eq!(action, "long_open");
                saw_decision = true;
            }
            ProgressEvent::FillRecorded { run_id, side, .. } => {
                assert_eq!(run_id, &run.id);
                assert_eq!(side, "buy");
                saw_fill = true;
            }
            ProgressEvent::MetricsUpdated { run_id, .. } => {
                assert_eq!(run_id, &run.id);
                saw_metrics = true;
            }
            ProgressEvent::RunCompleted { run_id, .. } => {
                assert_eq!(run_id, &run.id);
                saw_completed = true;
            }
            _ => {}
        }
    }
    assert!(saw_started, "no RunStarted event in the bus");
    assert!(saw_tick, "no RunTick event in the bus");
    assert!(saw_decision, "no DecisionEmitted event in the bus");
    assert!(saw_fill, "no FillRecorded event in the bus");
    assert!(saw_metrics, "no MetricsUpdated event in the bus");
    assert!(saw_completed, "no RunCompleted event in the bus");
}

#[tokio::test]
async fn paper_executor_runs_clean_with_no_progress_subscriber() {
    // Sanity: passing `with_progress` but having NO active subscriber must
    // not crash the run. broadcast::Sender::send returns Err when there
    // are no receivers; the executor swallows it silently.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = build_strategy("01TESTSTRATEGYPROGRESS00000B");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bus = ProgressBus::new(8);
    let tx = bus.sender();
    drop(bus); // drop the bus so no subscribers remain; the tx still lives

    let mock_broker = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock_broker;
    let dispatch = long_open_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars_and_progress(scenario_bars(&scenario), tx);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run should still succeed without a subscriber");
}

#[tokio::test]
async fn progress_bus_supports_multiple_subscribers() {
    let bus = ProgressBus::new(8);
    let mut a = bus.subscribe();
    let mut b = bus.subscribe();
    let _ = bus.sender().send(ProgressEvent::RunStarted {
        run_id: "r1".into(),
        estimated_tokens: 0,
    });
    let ea = a.recv().await.unwrap();
    let eb = b.recv().await.unwrap();
    assert!(matches!(ea, ProgressEvent::RunStarted { run_id, .. } if run_id == "r1"));
    assert!(matches!(eb, ProgressEvent::RunStarted { run_id, .. } if run_id == "r1"));
}
