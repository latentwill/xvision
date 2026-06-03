//! Regression for `MetricsSummary.win_rate` (QA 2026-06-03): the backtest
//! executor historically hardcoded `win_rate = 0.0`. It is now computed from
//! closed round-trips — a fill that books realized PnL is one closed trade,
//! and `win_rate = winning_trades / closed_trades`. `n_trades` stays the
//! per-fill (leg) count: an open + a close is two legs but one round-trip.
//!
//! The trader opens long on the first tick and flattens on the second; the
//! price path decides whether that single round-trip wins or loses.

#![allow(deprecated)] // canonical_scenarios() — Task 8 (M2) deprecation note.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    pool
}

fn single_asset_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTWINRATEROUNDTRIP0001".into(),
            display_name: "win-rate round-trip regression".into(),
            plain_summary: "open then flatten one round-trip".into(),
            creator: "@tester".into(),
            template: "custom".into(),
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
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-win-rate".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

/// 4 hourly bars whose opens follow `opens`. Entry fills at bar[1].open and
/// the close at bar[2].open, so the round-trip PnL is driven by those two.
fn bars_with_opens(opens: [f64; 4], scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    for &open in &opens {
        bars.push(Ohlcv {
            timestamp: ts,
            open,
            high: open * 1.01,
            low: open * 0.99,
            close: open,
            volume: 1_000.0,
        });
        ts += chrono::Duration::hours(1);
    }
    bars
}

fn resp(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: text.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

/// Run a single open→flat round-trip over the given bar opens. Returns
/// `(win_rate, n_trades)`.
async fn round_trip(opens: [f64; 4]) -> (f64, u32) {
    let long_open = r#"{"action":"long_open","conviction":0.8,"justification":"enter long"}"#;
    let flat = r#"{"action":"flat","conviction":0.8,"justification":"close out"}"#;

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let strategy = single_asset_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(bars_with_opens(opens, &scenario));
    let mut run = Run::new_queued("test-win-rate".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();
    // long_open on the first call, flat forever after (sequence repeats the
    // last entry once one remains).
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![resp(long_open), resp(flat)]));
    let tools = Arc::new(ToolRegistry::empty());

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run must complete cleanly");
    (metrics.win_rate, metrics.n_trades)
}

#[tokio::test]
async fn profitable_round_trip_yields_win_rate_one() {
    // Entry fills at bar[1].open (110), close at bar[2].open (140): a clear
    // win after round-trip costs. One closed trade, one winner.
    let (win_rate, n_trades) = round_trip([100.0, 110.0, 140.0, 150.0]).await;
    assert_eq!(n_trades, 2, "open + close are two fills (legs)");
    assert_eq!(win_rate, 1.0, "the single closed round-trip is a winner");
}

#[tokio::test]
async fn losing_round_trip_yields_win_rate_zero() {
    // Entry fills at bar[1].open (110), close at bar[2].open (90): a loss.
    // n_trades == 2 proves a close happened, so win_rate 0.0 reflects a
    // losing trade — not "no trades".
    let (win_rate, n_trades) = round_trip([100.0, 110.0, 90.0, 80.0]).await;
    assert_eq!(n_trades, 2, "open + close are two fills (legs)");
    assert_eq!(win_rate, 0.0, "the single closed round-trip is a loser");
}
