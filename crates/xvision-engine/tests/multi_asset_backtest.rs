//! Integration coverage for the multi-asset (B4) per-asset fan-out in the
//! backtest executor.
//!
//! The backtest executor runs the pipeline once per active asset each bar,
//! sharing ONE pooled capital book (pooled NAV), tracking per-asset positions.
//! v1 implements `execution_mode = PerAsset` + `capital_mode = Pooled`.
//!
//! Built on the single-asset `decisions_count.rs` setup, extended to a
//! two-asset universe via `with_asset_bars`.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{BacktestExecutor, Executor};
use xvision_engine::eval::run::{Run, RunMode};
#[allow(deprecated)]
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::exec_mode::ExecutionMode;
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
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

/// The asset-free canonical scenario the single-asset count regression uses.
/// Bars are injected directly, so only `capital.initial` and `warmup_bars`
/// matter here — the asset set comes from the strategy universe.
#[allow(deprecated)]
fn asset_free_scenario() -> Scenario {
    canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist")
}

/// Deterministic dispatch that always opens a long. The first `long_open`
/// per asset opens a position at bar 1; subsequent ones are no-ops at the
/// fill seam but still record a decision row, so every (bar, asset) yields a
/// decision we can collect by asset.
fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.8,"justification":"multi-asset-fanout"}"#,
    ))
}

fn build_strategy(agent_id: &str, execution_mode: ExecutionMode) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "multi-asset fan-out strategy".into(),
            plain_summary: "per-asset fan-out coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into()],
            decision_cadence_minutes: 1_440,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode,
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

/// Daily bars starting 2026-01-01 with a per-asset base price, monotonically
/// increasing close. The two assets share timestamps (aligned timeline).
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

#[tokio::test]
async fn backtest_fans_out_over_universe_with_shared_nav() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = build_strategy("01TESTMULTIASSETFANOUT", ExecutionMode::PerAsset);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Inject aligned bars for BOTH assets (same 5 timestamps, distinct prices).
    let btc = daily_bars(5, 50_000.0);
    let eth = daily_bars(5, 3_000.0);
    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> =
        BTreeMap::from([(AssetSymbol::Btc, btc), (AssetSymbol::Eth, eth)]);

    let executor = BacktestExecutor::new().with_asset_bars(asset_bars);

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
        .expect("multi-asset backtest run should complete");

    // Decisions must exist for BOTH assets.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let assets: std::collections::BTreeSet<String> =
        decisions.iter().map(|d| d.asset.clone()).collect();
    assert!(
        assets.contains("BTC/USD"),
        "expected BTC decisions, got assets: {assets:?}"
    );
    assert!(
        assets.contains("ETH/USD"),
        "expected ETH decisions, got assets: {assets:?}"
    );
    // 5 timestamps × 2 assets = 10 per-asset decisions; two assets at the
    // same bar are distinct decisions.
    assert_eq!(
        decisions.len(),
        10,
        "5 timestamps × 2 assets must yield 10 per-asset decisions"
    );
    assert_eq!(metrics.n_decisions, 10);

    // ONE pooled equity series (not two independent NAVs): exactly one
    // equity sample per distinct timestamp (5), not one per decision (10).
    let equity = store.read_equity_curve(&run.id).await.unwrap();
    assert_eq!(
        equity.len(),
        5,
        "pooled NAV records one equity sample per timestamp, got {} samples",
        equity.len()
    );
    let timestamps: std::collections::BTreeSet<_> = equity.iter().map(|(t, _)| *t).collect();
    assert_eq!(
        timestamps.len(),
        5,
        "pooled equity timestamps must be distinct (single series)"
    );
}

#[tokio::test]
async fn portfolio_mode_returns_not_implemented() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = build_strategy("01TESTPORTFOLIOMODE", ExecutionMode::Portfolio);
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> = BTreeMap::from([
        (AssetSymbol::Btc, daily_bars(3, 50_000.0)),
        (AssetSymbol::Eth, daily_bars(3, 3_000.0)),
    ]);
    let executor = BacktestExecutor::new().with_asset_bars(asset_bars);

    let err = executor
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
        .expect_err("portfolio execution_mode must error in v1");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not yet implemented"),
        "error must say not yet implemented, got: {msg}"
    );
    assert!(
        msg.contains("portfolio"),
        "error must name the portfolio mode, got: {msg}"
    );
}
