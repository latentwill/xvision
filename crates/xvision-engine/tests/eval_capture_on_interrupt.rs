//! F36 (capture-on-interrupt): a cancelled / failed / interrupted eval run must
//! persist the metrics + tokens it accumulated up to the interrupt, instead of
//! leaving `metrics_json = NULL`. Before this fix, metrics were written only by
//! `RunStore::finalize`, which the cancel/fail paths never reach — so all
//! cancelled and failed runs in the live DB had NULL metrics.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{LlmDispatch, LlmRequest, LlmResponse, MockDispatch};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::MetricsSummary;
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
#[allow(deprecated)]
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{ActivationMode, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    RunStore::new(pool)
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"test"}"#,
    ))
}

/// A dispatch that cancels its own eval run after the Nth completion, then keeps
/// echoing `hold`. Lets a test reach a genuine *mid-flight* cancel: the executor
/// makes a few real decisions (so the accumulators are non-trivial), the run
/// flips to cancelled, and the executor's next in-loop terminal check captures
/// the partial metrics and bails.
struct CancelAfterDispatch {
    inner: Arc<dyn LlmDispatch>,
    store: RunStore,
    run_id: String,
    cancel_after: u32,
    calls: AtomicU32,
}

#[async_trait]
impl LlmDispatch for CancelAfterDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.cancel_after {
            let _ = self
                .store
                .cancel_active(&self.run_id, "cancelled mid-flight by test")
                .await;
        }
        self.inner.complete(req).await
    }
}

fn test_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::hours(i as i64),
                open: px,
                high: px + 50.0,
                low: px - 50.0,
                close: px + 5.0,
                volume: 100.0,
            }
        })
        .collect()
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTF36CAPTURE".into(),
            display_name: "f36-capture-test".into(),
            plain_summary: "capture-on-interrupt test".into(),
            creator: "@test".into(),
            template: "trend_follower".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "mock".into(),
            allowed_tools: vec![],
            provider: None,
            model: Some("mock".into()),
        }),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// Direct unit test of the new `RunStore::persist_partial`: it writes
/// `metrics_json` + tokens on a running row without flipping status.
#[tokio::test]
async fn persist_partial_writes_metrics_and_tokens_without_status_change() {
    let store = fresh_store().await;
    let mut run = Run::new_queued("strat".into(), "scen".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store.begin_running(&run.id).await.unwrap();

    let metrics = MetricsSummary {
        total_return_pct: 1.5,
        sharpe: 0.3,
        n_decisions: 7,
        ..Default::default()
    };
    let wrote = store.persist_partial(&run.id, &metrics, 1234, 56).await.unwrap();
    assert!(wrote, "persist_partial must update the running row");

    run = store.get(&run.id).await.unwrap();
    assert_eq!(run.status, RunStatus::Running, "status must stay running");
    let m = run.metrics.expect("metrics must be persisted, not NULL");
    assert_eq!(m.n_decisions, 7);
    assert_eq!(run.actual_input_tokens, Some(1234));
    assert_eq!(run.actual_output_tokens, Some(56));
}

/// The capture-on-interrupt path: a run cancelled MID-FLIGHT (while running, after
/// some decisions) must come out of the executor with NON-NULL metrics (the F36
/// fix), not the old `NULL`. The executor detects the cancellation at its in-loop
/// terminal check, persists the accumulated metrics+tokens, then bails.
#[tokio::test]
async fn cancelled_run_persists_partial_metrics_not_null() {
    let store = fresh_store().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let strategy = minimal_strategy();
    let bars = test_bars(40);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Cancel the run from inside the dispatch after the 3rd decision, so the
    // executor is genuinely mid-flight (running, with accumulators populated)
    // when it next checks for termination.
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(CancelAfterDispatch {
        inner: hold_dispatch(),
        store: RunStore::new(store.pool().clone()),
        run_id: run.id.clone(),
        cancel_after: 3,
        calls: AtomicU32::new(0),
    });

    let result = Executor::with_bars(bars)
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch,
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await;
    // The executor bails with "eval run stopped" once it sees the cancellation.
    assert!(
        result.is_err(),
        "a mid-flight cancelled run must abort the executor"
    );

    let persisted = store.get(&run.id).await.unwrap();
    assert_eq!(
        persisted.status,
        RunStatus::Cancelled,
        "status stays cancelled (persist_partial must not revive it)"
    );
    let metrics = persisted
        .metrics
        .expect("F36: a cancelled run must persist partial metrics, not leave metrics_json NULL");
    assert!(
        metrics.n_decisions >= 1,
        "partial metrics must reflect the decisions made before cancel, got n_decisions={}",
        metrics.n_decisions
    );
}
