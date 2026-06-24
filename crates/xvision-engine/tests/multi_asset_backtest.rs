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
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
#[allow(deprecated)]
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::exec_mode::{CapitalMode, ExecutionMode};
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
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
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
    sqlx::query(include_str!("../migrations/071_decisions_delayed.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/073_eval_run_bars.sql"))
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
            timeframe_requirements: Default::default(),
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
    store
        .ensure_agent_run_baseline(&run.id, "full_debug")
        .await
        .unwrap();

    // Inject aligned bars for BOTH assets (same 5 timestamps, distinct prices).
    let btc = daily_bars(5, 50_000.0);
    let eth = daily_bars(5, 3_000.0);
    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> =
        BTreeMap::from([(AssetSymbol::Btc, btc), (AssetSymbol::Eth, eth)]);

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
        .expect("multi-asset backtest run should complete");

    // Decisions must exist for BOTH assets.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let assets: std::collections::BTreeSet<String> = decisions.iter().map(|d| d.asset.clone()).collect();
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

/// Misaligned timeline: asset B (ETH) is missing a bar at one INTERIOR
/// timestamp. ETH carries an open long across that gap. The pooled equity at
/// the gap timestamp must reflect ETH's LAST seen mark, not snap back to
/// ETH's entry-based / zero unrealized (the `PortfolioBook` last-mark fix).
///
/// Prices rise monotonically for both assets and both legs are long, so the
/// pooled NAV must be non-decreasing across the whole timeline. Under the
/// pre-fix behavior the gap timestamp drops ETH's accrued unrealized gain
/// (absent-from-marks → zero unrealized), producing a spurious dip — caught
/// here by the monotonicity assertion at the gap.
#[tokio::test]
async fn backtest_misaligned_timeline_carries_last_mark() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = build_strategy("01TESTMISALIGNEDLASTMARK", ExecutionMode::PerAsset);
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

    // BTC has all 5 daily bars. ETH is missing the interior bar at index 2
    // (the 3rd timestamp) — it has bars at indices 0,1,3,4 only. ETH opens a
    // long at its first bar (fills at the next bar's open) and stays long.
    let btc = daily_bars(5, 50_000.0);
    let eth_full = daily_bars(5, 3_000.0);
    let gap_idx = 2usize;
    let gap_ts = eth_full[gap_idx].timestamp;
    let eth: Vec<Ohlcv> = eth_full
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != gap_idx)
        .map(|(_, b)| b)
        .collect();
    assert_eq!(eth.len(), 4, "ETH must be missing exactly one interior bar");

    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> =
        BTreeMap::from([(AssetSymbol::Btc, btc), (AssetSymbol::Eth, eth)]);

    let executor = Executor::new().with_asset_bars(asset_bars);
    executor
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
        .expect("misaligned multi-asset backtest run should complete");

    // 5 distinct timestamps (BTC drives all 5; ETH absent at the gap).
    let equity = store.read_equity_curve(&run.id).await.unwrap();
    assert_eq!(
        equity.len(),
        5,
        "pooled NAV records one equity sample per distinct timestamp, got {}",
        equity.len()
    );

    // Rising prices + two long legs ⇒ once both legs are open the pooled
    // NAV must be non-decreasing across every subsequent step (no new fees
    // accrue — repeated `long_open` on an already-long leg is a no-op fill).
    // The pre-fix bug would drop ETH's unrealized at the gap timestamp,
    // producing a spurious dip that this monotonicity check catches.
    //
    // We anchor the monotonic check at the timestamp BEFORE the gap (by then
    // both legs are open and fees are paid). The gap-timestamp equity is the
    // crux: under the fix ETH is carried at its last mark (its index-1 mark),
    // BTC's mark rose, so the gap equity must be >= the pre-gap equity.
    let mut pre_gap_equity: Option<f64> = None;
    let mut gap_equity: Option<f64> = None;
    let mut last_before_gap: Option<f64> = None;
    for (t, eq) in &equity {
        if *t < gap_ts {
            pre_gap_equity = last_before_gap;
            last_before_gap = Some(*eq);
        } else if *t == gap_ts {
            // The immediately-preceding sample is the last `< gap_ts` value.
            pre_gap_equity = last_before_gap;
            gap_equity = Some(*eq);
        }
    }

    let gap_equity = gap_equity.expect("the gap timestamp must appear in the equity series");
    let pre_gap_equity = pre_gap_equity.expect("there must be at least one timestamp before the gap");

    // The core assertion: ETH's accrued unrealized is CARRIED across the gap
    // (not reset to entry/zero). With ETH carried flat at its last mark and
    // BTC's mark rising into the gap, the pooled equity must not dip at the
    // gap timestamp. The pre-fix behavior would subtract ETH's entire
    // unrealized gain here, dropping below `pre_gap_equity`.
    assert!(
        gap_equity >= pre_gap_equity - 1e-6,
        "pooled equity dipped at the gap timestamp ({gap_equity} < pre-gap {pre_gap_equity}) \
         — ETH's last mark was not carried across the gap (snapped to entry/zero unrealized)"
    );
}

/// Task C3 — assets_subset wiring.
///
/// A strategy with universe `[BTC/USD, ETH/USD]` is run with
/// `BacktestExecutor::with_asset_subset(vec![Eth])`. Bars are injected for
/// BOTH assets; only ETH decisions must appear. This is a hermetic executor-
/// level test — no DB-resolved bars, no network.
#[tokio::test]
async fn backtest_asset_subset_excludes_other_assets() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = build_strategy("01TESTASSETSUBSET", ExecutionMode::PerAsset);
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

    // Inject bars for BOTH assets in the universe.
    let btc = daily_bars(4, 50_000.0);
    let eth = daily_bars(4, 3_000.0);
    let asset_bars: BTreeMap<AssetSymbol, Vec<Ohlcv>> =
        BTreeMap::from([(AssetSymbol::Btc, btc), (AssetSymbol::Eth, eth)]);

    // Apply subset: only ETH should trade.
    let executor = Executor::new()
        .with_asset_bars(asset_bars)
        .with_asset_subset(vec![AssetSymbol::Eth]);

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
        .expect("subset backtest must complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    let assets: std::collections::BTreeSet<String> = decisions.iter().map(|d| d.asset.clone()).collect();

    // BTC must be absent — the subset excludes it.
    assert!(
        !assets.contains("BTC/USD"),
        "BTC/USD must be excluded by assets_subset, got assets: {assets:?}"
    );
    // ETH must be present.
    assert!(
        assets.contains("ETH/USD"),
        "ETH/USD must be included by assets_subset, got assets: {assets:?}"
    );
    // 4 bars × 1 active asset = 4 decisions.
    assert_eq!(
        decisions.len(),
        4,
        "4 bars × 1 active asset (ETH) must yield exactly 4 decisions, got {}",
        decisions.len()
    );
    assert_eq!(metrics.n_decisions, 4);
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
    let executor = Executor::new().with_asset_bars(asset_bars);

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

/// Not-implemented guard: `ExecutionMode::Custom(_)` is stored as experimental
/// but rejected at run time in v1. Flips to a behavior test when a concrete
/// custom mode is implemented. (Pins existing behavior — see Phase 3 spec.)
#[tokio::test]
async fn custom_execution_mode_returns_not_implemented() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let strategy = build_strategy(
        "01TESTCUSTOMEXECMODE000000",
        ExecutionMode::Custom("rotate".into()),
    );
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
    let executor = Executor::new().with_asset_bars(asset_bars);

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
        .expect_err("custom execution_mode must error in v1");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not yet implemented"),
        "error must say not yet implemented, got: {msg}"
    );
    assert!(
        msg.contains("custom:rotate"),
        "error must name the custom mode, got: {msg}"
    );
}

/// Not-implemented guard: `CapitalMode::PerAsset` (segregated per-asset
/// sleeves) is stored as experimental but rejected at run time in v1. Flips to
/// a behavior test when sleeve accounting is implemented. (Pins existing
/// behavior — see Phase 3 spec.)
#[tokio::test]
async fn capital_mode_per_asset_returns_not_implemented() {
    let store = fresh_store().await;
    let scenario = asset_free_scenario();
    let mut strategy = build_strategy("01TESTCAPITALPERASSET00000", ExecutionMode::PerAsset);
    strategy.manifest.capital_mode = CapitalMode::PerAsset;
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
    let executor = Executor::new().with_asset_bars(asset_bars);

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
        .expect_err("capital_mode per_asset must error in v1");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not yet implemented"),
        "error must say not yet implemented, got: {msg}"
    );
    assert!(
        msg.contains("per_asset"),
        "error must name capital_mode per_asset, got: {msg}"
    );
}
