//! Phase 3.B-paper integration test for `PaperExecutor`. Drives the
//! executor with a `MockBrokerSurface` (from xvision-execution) and a
//! `MockDispatch` (from xvision-engine::agent::llm) so no network is
//! required. Verifies that:
//!  - run() flips status to Running then to Completed
//!  - actionable trader decisions hit the broker exactly once each
//!  - per-decision rows are persisted to RunStore (eval_decisions)
//!  - per-tick equity samples are persisted (eval_equity_samples)
//!  - finalize() lands a MetricsSummary on the run

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::bundle::manifest::PublicManifest;
use xvision_engine::bundle::risk::RiskPreset;
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::StrategyBundle;
use xvision_engine::eval::executor::{Executor, PaperExecutor};
use xvision_engine::eval::{
    canonical_scenarios, Run, RunMode, RunStatus, RunStore, Scenario,
};
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn minimal_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01TESTBUNDLE0000000000000A".into(),
            display_name: "Test bundle".into(),
            plain_summary: "for paper executor tests".into(),
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
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

/// 4-hour scenario at 60-min cadence → 4 ticks. Tight enough for fast tests.
fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-short".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

/// Helper: build a paper-mode harness — pool, store, mock-broker (both
/// concrete + erased Arcs), executor, mock dispatch, tools, run, bundle,
/// scenario. The two broker arcs share the same allocation so
/// `mock.submitted()` reflects the executor's calls.
async fn paper_harness(
    canned_trader_json: &str,
    initial_balance: f64,
) -> (
    Arc<MockBrokerSurface>,
    PaperExecutor,
    RunStore,
    Run,
    StrategyBundle,
    Scenario,
    Arc<dyn LlmDispatch>,
    Arc<ToolRegistry>,
) {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let executor = PaperExecutor::new(broker);
    let bundle = minimal_bundle();
    let scenario = short_scenario();
    let run = Run::new_queued(
        "test-bundle-hash".into(),
        scenario.id.clone(),
        RunMode::Paper,
    );
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned_trader_json));
    let tools = Arc::new(ToolRegistry::empty());
    (mock, executor, store, run, bundle, scenario, dispatch, tools)
}

#[tokio::test]
async fn paper_executor_runs_to_completion() {
    let canned = r#"{"action":"hold","conviction":0.0,"justification":"test mock decision"}"#;
    let (_mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;
    let id = run.id.clone();

    let metrics = executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .expect("run must succeed");

    let after = store.get(&id).await.unwrap();
    assert_eq!(after.status, RunStatus::Completed);
    assert!(after.metrics.is_some());
    assert!(after.completed_at.is_some());
    assert!(metrics.n_decisions > 0);
}

#[tokio::test]
async fn paper_executor_records_a_decision_row_per_tick() {
    let canned = r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#;
    let (_mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .unwrap();

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 4, "expected one decision row per tick");
    for (i, d) in decisions.iter().enumerate() {
        assert_eq!(d.decision_index, i as u32);
        assert_eq!(d.action, "hold");
        assert_eq!(d.asset, "BTC/USD");
    }
}

#[tokio::test]
async fn paper_executor_submits_orders_only_for_actionable_decisions() {
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"buy"}"#;
    let (mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let metrics = executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .unwrap();

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 4, "broker should see one submit per actionable tick");
    assert_eq!(metrics.n_trades, 4);
    assert_eq!(metrics.n_decisions, 4);

    let mut keys: Vec<String> = submitted.iter().map(|r| r.idempotency_key.clone()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 4, "every submission must use a unique idempotency key");
}

#[tokio::test]
async fn paper_executor_skips_broker_for_flat_decisions() {
    let canned = r#"{"action":"flat","conviction":0.0,"justification":"sit"}"#;
    let (mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let metrics = executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .unwrap();

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 0, "flat decisions must NOT hit the broker");
    assert_eq!(metrics.n_trades, 0);
    assert_eq!(metrics.n_decisions, 4);
}

#[tokio::test]
async fn paper_executor_records_equity_sample_per_tick() {
    let canned = r#"{"action":"hold","conviction":0.0,"justification":"hold"}"#;
    let (_mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 50_000.0).await;

    executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .unwrap();

    let curve = store.read_equity_curve(&run.id).await.unwrap();
    assert_eq!(curve.len(), 4);
    for (_, equity) in &curve {
        assert_eq!(*equity, 50_000.0);
    }
}

#[tokio::test]
async fn paper_executor_idempotency_key_includes_run_id_and_decision_index() {
    let canned = r#"{"action":"long_open","conviction":0.5,"justification":"buy"}"#;
    let (mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .unwrap();

    let submitted = mock.submitted();
    for (i, req) in submitted.iter().enumerate() {
        assert!(
            req.idempotency_key.contains(&run.id),
            "idempotency_key {} must contain run_id {}",
            req.idempotency_key,
            run.id
        );
        assert!(
            req.idempotency_key.contains(&i.to_string()),
            "idempotency_key {} must contain decision_index {i}",
            req.idempotency_key
        );
    }
}

#[tokio::test]
async fn paper_executor_handles_unparseable_trader_output_as_flat() {
    // Mock returns garbage that fails serde parsing → executor should fall
    // back to a flat decision instead of erroring out the run.
    let canned = "definitely not json";
    let (mock, executor, store, mut run, bundle, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let metrics = executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
        .expect("unparseable trader output must not fail the run");

    assert_eq!(metrics.n_trades, 0, "unparseable → no broker submissions");
    assert_eq!(metrics.n_decisions, 4);
    assert_eq!(mock.submitted().len(), 0);
}
