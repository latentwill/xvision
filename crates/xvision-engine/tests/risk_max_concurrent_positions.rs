//! Regression for `risk.max_concurrent_positions` enforcement in the
//! backtest executor (QA 2026-06-03 finding: 3 simultaneous `long_open`s
//! were opened under a cap of 2 in a multi-asset run).
//!
//! Setup: a 3-asset universe (BTC/ETH/SOL) where the trader emits
//! `long_open` for every asset every bar. With `max_concurrent_positions = 2`
//! only the first two assets (BTC, ETH — `AssetSymbol` Ord) may open from
//! flat; the third (SOL) is rewritten to `hold`. `n_trades` counts exactly
//! two opening fills. The cap=3 control proves the block is the cap doing the
//! work, not some other gate.

#![allow(deprecated)] // canonical_scenarios() — Task 8 (M2) deprecation note.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
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

fn three_asset_strategy(max_concurrent: u32) -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.max_concurrent_positions = max_concurrent;
    Strategy {
        manifest: PublicManifest {
            id: "01TESTMAXCONCURRENTPOS0001".into(),
            display_name: "max-concurrent-positions regression".into(),
            plain_summary: "3 assets all signal long_open".into(),
            creator: "@tester".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into(), "SOL/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(), // PerAsset
            capital_mode: Default::default(),    // Pooled
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
        risk,
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-max-concurrent".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

/// Hourly bars over the scenario window at a given base price level. All
/// three assets share the same timestamps (so they decide at the same
/// ticks); only the price level differs.
fn bars_at(base: f64, scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    while ts < scenario.time_window.end {
        let close = base + i;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 0.25,
            high: close + 0.5,
            low: close - 0.75,
            close,
            volume: 100.0 + i,
        });
        ts += chrono::Duration::hours(1);
        i += 1.0;
    }
    bars
}

fn three_asset_bars(scenario: &Scenario) -> BTreeMap<AssetSymbol, Vec<Ohlcv>> {
    BTreeMap::from([
        (AssetSymbol::Btc, bars_at(100_000.0, scenario)),
        (AssetSymbol::Eth, bars_at(2_300.0, scenario)),
        (AssetSymbol::Sol, bars_at(200.0, scenario)),
    ])
}

async fn n_trades_for_cap(max_concurrent: u32) -> u32 {
    let canned = r#"{"action":"long_open","conviction":0.8,"justification":"go long the trend"}"#;
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let strategy = three_asset_strategy(max_concurrent);
    let scenario = short_scenario();
    let executor = Executor::new().with_asset_bars(three_asset_bars(&scenario));
    let mut run = Run::new_queued(
        "test-max-concurrent".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    // Seed the agent_runs parent row the API kickoff normally creates, so the
    // cap/guardrail `supervisor_notes` inserts (FK → agent_runs.id) resolve.
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run must complete cleanly");
    metrics.n_trades
}

#[tokio::test]
async fn cap_blocks_third_simultaneous_open() {
    // Three assets all signal long_open from flat at the first tick. Under a
    // cap of 2 only BTC and ETH may open; SOL is rewritten to hold. The only
    // fills in the whole run are those two opens.
    let n_trades = n_trades_for_cap(2).await;
    assert_eq!(
        n_trades, 2,
        "max_concurrent_positions=2 must allow exactly two opening fills across three assets"
    );
}

#[tokio::test]
async fn cap_of_three_allows_all_opens() {
    // Control: raising the cap to 3 lets all three assets open — proving the
    // block in the cap=2 case is the cap, not some other gate.
    let n_trades = n_trades_for_cap(3).await;
    assert_eq!(
        n_trades, 3,
        "max_concurrent_positions=3 must allow all three assets to open"
    );
}
