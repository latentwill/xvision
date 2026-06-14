//! Regression for `risk.max_concurrent_positions` enforcement in the backtest
//! executor (QA 2026-06-03: 3 simultaneous `long_open`s opened under a cap of 2
//! in a multi-asset run).
//!
//! Three assets all signal `long_open` from flat at the first tick. With a cap
//! of 2 only the first two (BTC, ETH by `AssetSymbol` Ord) may open; SOL is
//! rewritten to `hold`. `n_trades` counts exactly two opening fills. The cap=3
//! control proves the block is the cap, not some other gate.

#![allow(deprecated)] // canonical_scenarios()

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for m in [
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
        include_str!("../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
    ] {
        sqlx::query(m).execute(&pool).await.unwrap();
    }
    RunStore::new(pool)
}

#[allow(deprecated)]
fn asset_free_scenario() -> Scenario {
    canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist")
}

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.8,"justification":"go long the trend"}"#,
    ))
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
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into(), "SOL/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(), // PerAsset
            capital_mode: Default::default(),   // Pooled
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
        risk,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// Daily bars on an aligned timeline at a given base price.
fn daily_bars(count: usize, base: f64) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = base + i as f64 * 10.0;
            Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: px,
                high: px + 25.0,
                low: px - 25.0,
                close: px + 5.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

async fn n_trades_for_cap(max_concurrent: u32) -> u32 {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = three_asset_strategy(max_concurrent);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "full_debug")
        .await
        .unwrap();

    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> = BTreeMap::from([
        (AssetSymbol::Btc, daily_bars(3, 100_000.0)),
        (AssetSymbol::Eth, daily_bars(3, 2_300.0)),
        (AssetSymbol::Sol, daily_bars(3, 200.0)),
    ]);
    let executor = Executor::new().with_asset_bars(asset_bars);

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            long_open_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("backtest run must complete cleanly");
    metrics.n_trades
}

#[tokio::test]
async fn cap_blocks_third_simultaneous_open() {
    let n_trades = n_trades_for_cap(2).await;
    assert_eq!(
        n_trades, 2,
        "max_concurrent_positions=2 must allow exactly two opening fills across three assets"
    );
}

#[tokio::test]
async fn cap_of_three_allows_all_opens() {
    let n_trades = n_trades_for_cap(3).await;
    assert_eq!(
        n_trades, 3,
        "max_concurrent_positions=3 must allow all three assets to open"
    );
}
