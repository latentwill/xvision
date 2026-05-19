//! Regression coverage for `qa-decisions-30day-count`.
//!
//! Operator reported (2026-05-18) that a 30-day backtest produced only 29
//! decisions. The root cause was in `crates/xvision-engine/src/eval/executor/backtest.rs`:
//! the per-bar loop early-`break`ed when there was no next bar to fill
//! against, silently dropping the final decision. Fix: fall back to the
//! same bar's close as the fill source for the last bar so an N-bar
//! input yields N decision rows.
//!
//! These tests are parameterized over a few representative bar counts so
//! the invariant `decisions.len() == bars.len()` is exercised at the
//! 1-bar edge case, a typical-month size, and a longer window.

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::SqlitePool;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{BacktestExecutor, Executor};
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
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.1,"justification":"qa-30day-count"}"#,
    ))
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "qa-decisions-30day-count strategy".into(),
            plain_summary: "decision-count regression coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1_440,
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

/// Daily bars starting 2026-01-01, monotonically increasing close.
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

/// Drive a backtest with `bar_count` daily bars (hold-only) and return
/// the persisted decision count, the metrics-summary decision count,
/// and the first/last decision timestamps. Single helper so every
/// parameterized assertion has consistent setup.
async fn run_backtest_with_bars(
    bar_count: usize,
) -> (u32, u32, chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let store = fresh_store().await;
    #[allow(deprecated)]
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");
    let agent_id = format!("01TESTQADECISIONS{:013}", bar_count);
    let strategy = build_strategy(&agent_id);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars(bar_count);
    let first_ts = bars.first().unwrap().timestamp;
    let last_ts = bars.last().unwrap().timestamp;

    let dispatch = hold_dispatch();
    let tools = Arc::new(ToolRegistry::empty());
    let executor = BacktestExecutor::with_bars(bars);

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    let first = decisions.first().map(|d| d.timestamp).unwrap();
    let last = decisions.last().map(|d| d.timestamp).unwrap();
    assert_eq!(first, first_ts, "first decision should be keyed to the first bar");
    assert_eq!(last, last_ts, "last decision should be keyed to the last bar");
    (decisions.len() as u32, metrics.n_decisions, first, last)
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_5_bars() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(5).await;
    assert_eq!(
        persisted, 5,
        "5 bars must yield 5 decisions in the decisions table"
    );
    assert_eq!(
        summarized, 5,
        "metrics.n_decisions must agree with persisted count"
    );
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_30_bars() {
    // The operator-reported case (2026-05-18). Prior to the
    // qa-decisions-30day-count fix this returned 29.
    let (persisted, summarized, _, _) = run_backtest_with_bars(30).await;
    assert_eq!(
        persisted, 30,
        "30 bars must yield 30 decisions (was 29 before the fix)"
    );
    assert_eq!(
        summarized, 30,
        "metrics.n_decisions must agree with persisted count"
    );
}

#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_100_bars() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(100).await;
    assert_eq!(persisted, 100, "100 bars must yield 100 decisions");
    assert_eq!(
        summarized, 100,
        "metrics.n_decisions must agree with persisted count"
    );
}

/// The 1-bar edge case must also honor "N bars → N decisions". The
/// final-bar `next_bar_open` fallback (executor falls back to
/// `bar.close`) means a single-bar window has a valid fill source.
#[tokio::test]
async fn backtest_n_bars_yields_n_decisions_for_1_bar() {
    let (persisted, summarized, _, _) = run_backtest_with_bars(1).await;
    assert_eq!(persisted, 1, "1 bar must yield 1 decision");
    assert_eq!(
        summarized, 1,
        "metrics.n_decisions must agree with persisted count"
    );
}
