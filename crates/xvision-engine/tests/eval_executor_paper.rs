//! Phase 3.B-paper integration test for `PaperExecutor`. Drives the
//! executor with a `MockBrokerSurface` (from xvision-execution) and a
//! `MockDispatch` (from xvision-engine::agent::llm) so no network is
//! required. Verifies that:
//!  - run() flips status to Running then to Completed
//!  - actionable trader decisions hit the broker exactly once each
//!  - per-decision rows are persisted to RunStore (eval_decisions)
//!  - per-tick equity samples are persisted (eval_equity_samples)
//!  - finalize() lands a MetricsSummary on the run

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use async_trait::async_trait;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::eval::executor::{classify_run_failure, Executor, PaperExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStatus, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
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
    pool
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSTRATEGY0000000000000A".into(),
            display_name: "Test strategy".into(),
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

            min_warmup_bars: None,
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

/// 4-hour scenario at 60-min cadence → 4 ticks. Tight enough for fast tests.
fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-short".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

fn short_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    while ts < scenario.time_window.end {
        let close = 50_000.0 + i * 100.0;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 25.0,
            high: close + 50.0,
            low: close - 75.0,
            close,
            volume: 100.0 + i,
        });
        ts += chrono::Duration::hours(1);
        i += 1.0;
    }
    bars
}

/// Helper: build a paper-mode harness — pool, store, mock-broker (both
/// concrete + erased Arcs), executor, mock dispatch, tools, run, strategy,
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
    Strategy,
    Scenario,
    Arc<dyn LlmDispatch>,
    Arc<ToolRegistry>,
) {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = PaperExecutor::with_bars(broker, short_bars(&scenario));
    let run = Run::new_queued("test-strategy-hash".into(), scenario.id.clone(), RunMode::Paper);
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned_trader_json));
    let tools = Arc::new(ToolRegistry::empty());
    (mock, executor, store, run, strategy, scenario, dispatch, tools)
}

#[tokio::test]
async fn paper_executor_runs_to_completion() {
    let canned = r#"{"action":"hold","conviction":0.0,"justification":"test mock decision"}"#;
    let (_mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;
    let id = run.id.clone();

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
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
    let (_mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
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
    let (mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .unwrap();

    let submitted = mock.submitted();
    assert_eq!(
        submitted.len(),
        4,
        "broker should see one submit per actionable tick"
    );
    assert_eq!(metrics.n_trades, 4);
    assert_eq!(metrics.n_decisions, 4);

    let mut keys: Vec<String> = submitted.iter().map(|r| r.idempotency_key.clone()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(
        keys.len(),
        4,
        "every submission must use a unique idempotency key"
    );
}

#[tokio::test]
async fn paper_executor_skips_broker_for_flat_decisions() {
    let canned = r#"{"action":"flat","conviction":0.0,"justification":"sit"}"#;
    let (mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
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
    let (_mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 50_000.0).await;

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
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
    let (mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
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
async fn paper_executor_fails_on_unparseable_trader_output() {
    // Mock returns garbage that fails serde parsing → executor should fail
    // the run instead of silently converting it into a flat decision.
    let canned = "definitely not json";
    let (mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("unparseable trader output must fail the run");

    assert!(
        err.to_string().contains("invalid JSON"),
        "unexpected error: {err}"
    );
    assert_eq!(run.status, RunStatus::Failed, "run should stop as failed");
    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
    assert!(
        persisted
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("invalid JSON"),
        "unexpected persisted error: {:?}",
        persisted.error
    );
    assert_eq!(mock.submitted().len(), 0);
}

/// `LlmDispatch` that returns a caller-provided `LlmResponse` verbatim every
/// call. The default `MockDispatch::echo` always wraps text in a healthy
/// EndTurn response, which masks the empty/truncated cases this regression
/// exercises.
struct CannedResponseDispatch {
    response: LlmResponse,
}

impl CannedResponseDispatch {
    fn new(response: LlmResponse) -> Self {
        Self { response }
    }
}

#[async_trait]
impl LlmDispatch for CannedResponseDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(self.response.clone())
    }
}

async fn paper_harness_with_dispatch(
    dispatch: Arc<dyn LlmDispatch>,
) -> (
    Arc<MockBrokerSurface>,
    PaperExecutor,
    RunStore,
    Run,
    Strategy,
    Scenario,
    Arc<ToolRegistry>,
) {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let executor = PaperExecutor::with_bars(broker, short_bars(&scenario));
    let run = Run::new_queued("test-strategy-hash".into(), scenario.id.clone(), RunMode::Paper);
    store.create(&run).await.unwrap();
    let tools = Arc::new(ToolRegistry::empty());
    let _ = dispatch; // value passed into the test only for clarity
    (mock, executor, store, run, strategy, scenario, tools)
}

/// Regression for the QA10 report on run `01KRMKWZ1KJ2BGRNWGP518ZQ3Q`
/// decision 4: the provider returned `EndTurn` with no text content. The
/// executor must:
///  - fail the run with a `[empty]`-tagged reason,
///  - leave `mock.submitted()` empty,
///  - persist zero `eval_decisions` rows for the run.
#[tokio::test]
async fn paper_executor_fails_with_empty_class_on_empty_trader_output() {
    let response = LlmResponse {
        content: Vec::new(),
        stop_reason: StopReason::EndTurn,
        input_tokens: 981,
        output_tokens: 0,
    };
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(CannedResponseDispatch::new(response));
    let (mock, executor, store, mut run, strategy, scenario, tools) =
        paper_harness_with_dispatch(dispatch.clone()).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("empty trader output must fail the run");

    let err_str = err.to_string();
    assert_eq!(classify_run_failure(&err), "empty");
    assert!(
        err_str.contains("trader_output[empty]"),
        "expected trader_output[empty] tag in error: {err_str}"
    );
    assert!(
        err_str.contains("stop_reason=EndTurn"),
        "expected stop_reason diagnostic in error: {err_str}"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(persisted.status, RunStatus::Failed);
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[empty]"),
        "persisted error must lead with the [empty] class prefix: {reason:?}"
    );
    assert!(
        reason.contains("trader_output[empty]"),
        "persisted error must keep the trader_output kind tag: {reason:?}"
    );
    assert!(
        reason.contains("output_tokens=0"),
        "persisted error must include output_tokens for review: {reason:?}"
    );

    assert!(
        mock.submitted().is_empty(),
        "paper executor must NEVER submit an order for an empty trader output"
    );

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        decisions.is_empty(),
        "no decision row should be persisted when the trader output is empty"
    );
}

#[tokio::test]
async fn paper_executor_fails_with_truncated_class_on_max_tokens_no_text() {
    let response = LlmResponse {
        content: Vec::new(),
        stop_reason: StopReason::MaxTokens,
        input_tokens: 2000,
        output_tokens: 0,
    };
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(CannedResponseDispatch::new(response));
    let (mock, executor, store, mut run, strategy, scenario, tools) =
        paper_harness_with_dispatch(dispatch.clone()).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("max-tokens empty output must fail the run");

    assert_eq!(classify_run_failure(&err), "truncated");
    let err_str = err.to_string();
    assert!(
        err_str.contains("trader_output[truncated]"),
        "expected trader_output[truncated] tag: {err_str}"
    );
    assert!(err_str.contains("stop_reason=MaxTokens"));

    let persisted = store.get(&run.id).await.unwrap();
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[truncated]"),
        "persisted error must lead with [truncated] class: {reason:?}"
    );

    assert!(mock.submitted().is_empty());
    assert!(store.read_decisions(&run.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn paper_executor_fails_with_tool_use_only_class_when_no_final_text() {
    // The agent loop exits on EndTurn even if the response carries only
    // tool_use blocks (`execute.rs` treats the stop_reason as authoritative
    // over the content shape). The trader pipeline then needs a final text
    // payload and finds none. The run must fail with the `tool_use_only`
    // class — not a generic JSON parse error — and no order may be placed.
    let response = LlmResponse {
        content: vec![ContentBlock::ToolUse {
            id: "tu_1".into(),
            name: "fetch_bars".into(),
            input: serde_json::json!({}),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 500,
        output_tokens: 30,
    };
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(CannedResponseDispatch::new(response));
    let (mock, executor, store, mut run, strategy, scenario, tools) =
        paper_harness_with_dispatch(dispatch.clone()).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("tool-use-only trader response must fail the run");

    assert_eq!(classify_run_failure(&err), "tool_use_only");
    let err_str = err.to_string();
    assert!(err_str.contains("trader_output[tool_use_only]"));

    let persisted = store.get(&run.id).await.unwrap();
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(reason.starts_with("[tool_use_only]"), "{reason:?}");

    assert!(mock.submitted().is_empty());
    assert!(store.read_decisions(&run.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn paper_executor_invalid_json_failure_preserves_invalid_json_class() {
    // Sanity: the existing "definitely not json" failure (covered by the
    // legacy test) now persists with the `[invalid_json]` class prefix.
    let canned = "definitely not json";
    let (mock, executor, store, mut run, strategy, scenario, dispatch, tools) =
        paper_harness(canned, 100_000.0).await;

    let err = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect_err("garbage must fail the run");

    assert_eq!(classify_run_failure(&err), "invalid_json");
    let persisted = store.get(&run.id).await.unwrap();
    let reason = persisted.error.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("[invalid_json]"),
        "persisted error must lead with [invalid_json] class: {reason:?}"
    );
    assert!(reason.contains("invalid JSON"), "{reason:?}");
    assert!(mock.submitted().is_empty());
}
