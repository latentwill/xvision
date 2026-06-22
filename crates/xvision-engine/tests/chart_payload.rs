//! Integration tests for `api::chart::build_run_payload`.
//!
//! Full end-to-end testing (bars + decisions + equity fully seeded) requires
//! a live Alpaca connection and is covered by the Task 15 smoke test. The
//! tests here focus on the boundary conditions that don't need network access.

use chrono::TimeZone;
use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::{DecisionRow, Run, RunMode, RunStore};
use xvision_engine::strategies::store::strategy_store_dir;

/// Build a fresh `ApiContext` backed by an on-disk SQLite DB under a tmpdir.
/// Uses `ApiContext::open` so all migrations are applied and the canonical
/// scenario seed runs — identical to the pattern in `scenario_api.rs` and
/// `api_context.rs`.
struct TestCtx {
    ctx: ApiContext,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for TestCtx {
    type Target = ApiContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

async fn test_ctx() -> TestCtx {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "chart-test".into(),
        },
    )
    .await
    .unwrap();
    TestCtx { ctx, _dir: dir }
}

#[tokio::test]
async fn test_ctx_removes_tmpdir_on_drop() {
    let dir_path = {
        let ctx = test_ctx().await;
        let dir_path = ctx._dir.path().to_path_buf();
        assert!(
            dir_path.exists(),
            "test context directory should exist while the fixture is alive"
        );
        dir_path
    };

    assert!(
        !dir_path.exists(),
        "test context directory should be removed when the fixture is dropped"
    );
}

async fn seed_cached_bars(ctx: &ApiContext, cache_key: &str, asset: &str, count: usize) {
    let start = chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let mut blob = Vec::new();
    for i in 0..count {
        let ts = start + chrono::Duration::hours(i as i64);
        let base = 100.0 + i as f64;
        let line = serde_json::json!({
            "t": ts.to_rfc3339(),
            "o": base,
            "h": base + 2.0,
            "l": base - 1.0,
            "c": base + 1.0,
            "v": 1_000.0 + i as f64,
        });
        blob.extend(serde_json::to_vec(&line).unwrap());
        blob.push(b'\n');
    }

    sqlx::query(
        "INSERT OR REPLACE INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(cache_key)
    .bind(asset)
    .bind("1Hour")
    .bind(start.to_rfc3339())
    .bind((start + chrono::Duration::hours(count as i64)).to_rfc3339())
    .bind("alpaca-historical-v1")
    .bind("2026-05-14T00:00:00Z")
    .bind(count as i64)
    .bind(blob)
    .bind("none")
    .execute(&ctx.db)
    .await
    .unwrap();
}

fn hold_decision_for_asset(run_id: &str, asset: &str) -> DecisionRow {
    DecisionRow {
        run_id: run_id.into(),
        decision_index: 0,
        timestamp: chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        asset: asset.into(),
        action: "hold".into(),
        conviction: Some(0.6),
        justification: Some("fixture hold".into()),
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
    }
}

/// `build_run_payload` must return `ApiError::NotFound` for a run id that
/// doesn't exist in the store.
#[tokio::test]
async fn build_run_payload_unknown_run_returns_not_found() {
    let ctx = test_ctx().await;
    let err = xvision_engine::api::chart::build_run_payload(&ctx, "r_does_not_exist")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound, got: {err:?}"
    );
    // The error message should name the offending run id.
    let msg = err.to_string();
    assert!(
        msg.contains("r_does_not_exist"),
        "NotFound message should include the run id, got: {msg}"
    );
}

#[tokio::test]
async fn build_run_payload_uses_persisted_decision_asset_before_strategy_file() {
    let ctx = test_ctx().await;
    let scenario = xvision_engine::api::scenario::get(&ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    let strategy_id = "corrupt-chart-strategy";
    let strategy_dir = strategy_store_dir(&ctx.xvn_home);
    tokio::fs::create_dir_all(&strategy_dir).await.unwrap();
    tokio::fs::write(
        strategy_dir.join(format!("{strategy_id}.json")),
        b"{not valid json",
    )
    .await
    .unwrap();

    let cache_key = xvision_engine::eval::bars::compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    seed_cached_bars(&ctx, &cache_key, "ETH/USD", 3).await;

    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(strategy_id.into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store
        .record_decision(&hold_decision_for_asset(&run.id, "ETH/USD"))
        .await
        .unwrap();

    let payload = xvision_engine::api::chart::build_run_payload(&ctx, &run.id)
        .await
        .unwrap();

    assert_eq!(payload.asset, "ETH");
    assert_eq!(payload.bars.len(), 3);
    assert_eq!(
        payload.markers.holds.len(),
        1,
        "decision asset should let chart render even when the strategy artifact is unreadable",
    );
}

// ── Task 4 — build_compare_payload tests ────────────────────────────────────

/// Requesting more than 10 runs must return `ApiError::Validation` whose
/// message contains "narrow your filter".
#[tokio::test]
async fn build_compare_payload_caps_at_10_runs() {
    let ctx = test_ctx().await;
    let ids: Vec<String> = (0..11).map(|i| format!("r_{i}")).collect();
    let err = xvision_engine::api::chart::build_compare_payload(&ctx, &ids)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation error for >10 runs, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("narrow your filter"),
        "error message must contain 'narrow your filter', got: {msg}"
    );
}

// ── Task 1 — build_scenario_payload tests ───────────────────────────────────

/// Canonical scenarios have no cached bars in a fresh xvn_home — the cache
/// is only populated when `xvn bars fetch` runs. So `build_scenario_payload`
/// must return `CacheStatus::NotCached`.
#[tokio::test]
async fn build_scenario_payload_returns_not_cached_for_seeded_scenario() {
    use xvision_engine::api::chart::{build_scenario_payload, CacheStatus};
    let ctx = test_ctx().await;
    let payload = build_scenario_payload(&ctx, "crypto-bull-q1-2025").await.unwrap();
    assert_eq!(payload.scenario.id, "crypto-bull-q1-2025");
    assert!(
        matches!(payload.cache_status, CacheStatus::NotCached { .. }),
        "expected NotCached on fresh DB, got: {:?}",
        payload.cache_status
    );
    assert!(payload.bars.is_empty(), "bars should be empty for NotCached");
}

#[tokio::test]
async fn build_scenario_payload_loads_cached_bars_and_indicators() {
    use xvision_engine::api::chart::{build_scenario_payload, CacheStatus};
    use xvision_engine::api::scenario as api_scenario;

    let ctx = test_ctx().await;
    let scenario = api_scenario::get(&ctx, "crypto-bull-q1-2025").await.unwrap();
    // Scenarios are asset-free; `build_scenario_payload` previews against the
    // canonical BTC/USD backdrop, so seed bars under that asset-specific key.
    let preview_key = xvision_engine::eval::bars::compute_cache_key(
        "BTC/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    seed_cached_bars(&ctx, &preview_key, "BTC/USD", 64).await;

    let payload = build_scenario_payload(&ctx, &scenario.id).await.unwrap();

    assert_eq!(payload.scenario.id, scenario.id);
    assert!(matches!(
        payload.cache_status,
        CacheStatus::PartiallyCached {
            fetched_count: 64,
            ..
        }
    ));
    assert_eq!(payload.bars.len(), 64);
    assert_eq!(payload.indicators.sma_20.len(), 45);
    assert_eq!(payload.indicators.ema_20.len(), 45);
    assert_eq!(payload.indicators.bollinger.middle.len(), 45);
    assert_eq!(payload.indicators.rsi_14.len(), 50);
    assert_eq!(payload.indicators.atr_14.len(), 50);
    assert_eq!(payload.indicators.macd.line.len(), 39);
    assert_eq!(payload.indicators.macd.signal.len(), 31);
    assert_eq!(payload.indicators.macd.histogram.len(), 31);
}

#[tokio::test]
async fn build_scenario_payload_uses_requested_granularity_cache_key() {
    use xvision_engine::api::chart::{build_scenario_payload_with_granularity, CacheStatus};
    use xvision_engine::api::scenario as api_scenario;
    use xvision_engine::eval::scenario::BarGranularity;

    let ctx = test_ctx().await;
    let scenario = api_scenario::get(&ctx, "crypto-bull-q1-2025").await.unwrap();
    let override_key = xvision_engine::eval::bars::compute_cache_key(
        "BTC/USD",
        BarGranularity::Hour4,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );

    let payload = build_scenario_payload_with_granularity(&ctx, &scenario.id, Some("4h"), None)
        .await
        .unwrap();

    assert_eq!(payload.scenario.bar_cache_policy.cache_key, override_key);
    assert!(
        matches!(payload.cache_status, CacheStatus::NotCached { .. }),
        "expected alternate timeframe to check its own cache row"
    );
}

// ── Phase 1 — preview-asset selector tests ──────────────────────────────────

/// Absent `asset` param keeps the BTC/USD preview default (backward compat).
#[tokio::test]
async fn build_scenario_payload_defaults_to_btc_preview_asset() {
    use xvision_engine::api::chart::build_scenario_payload;
    let ctx = test_ctx().await;
    let payload = build_scenario_payload(&ctx, "crypto-bull-q1-2025").await.unwrap();
    assert_eq!(
        payload.preview_asset, "BTC",
        "default preview asset must be BTC when no asset is requested"
    );
}

/// Requesting `asset=ETH/USD` computes an ETH-specific cache key (distinct
/// from the BTC default) and reports ETH as the resolved preview asset.
#[tokio::test]
async fn build_scenario_payload_uses_requested_asset_cache_key() {
    use xvision_engine::api::chart::build_scenario_payload_with_granularity;
    use xvision_engine::api::scenario as api_scenario;

    let ctx = test_ctx().await;
    let scenario = api_scenario::get(&ctx, "crypto-bull-q1-2025").await.unwrap();

    let btc_key = xvision_engine::eval::bars::compute_cache_key(
        "BTC/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    let eth_key = xvision_engine::eval::bars::compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    assert_ne!(btc_key, eth_key, "sanity: ETH and BTC cache keys differ");

    let payload = build_scenario_payload_with_granularity(&ctx, &scenario.id, None, Some("ETH/USD"))
        .await
        .unwrap();

    assert_eq!(payload.preview_asset, "ETH");
    assert_eq!(
        payload.scenario.bar_cache_policy.cache_key, eth_key,
        "ETH request must use the ETH-specific cache key"
    );
    assert_ne!(
        payload.scenario.bar_cache_policy.cache_key, btc_key,
        "ETH request must not reuse the BTC cache key"
    );
}

/// An unrecognised asset is rejected with a validation error, not silently
/// coerced to the BTC default.
#[tokio::test]
async fn build_scenario_payload_rejects_unknown_asset() {
    use xvision_engine::api::chart::build_scenario_payload_with_granularity;
    let ctx = test_ctx().await;
    let err = build_scenario_payload_with_granularity(&ctx, "crypto-bull-q1-2025", None, Some("NOTACOIN"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, xvision_engine::api::ApiError::Validation(_)),
        "expected Validation for unknown asset, got: {err:?}"
    );
}

/// `build_scenario_payload` must return `ApiError::NotFound` for an id that
/// does not exist in the scenarios table.
#[tokio::test]
async fn build_scenario_payload_returns_not_found_for_unknown() {
    use xvision_engine::api::chart::build_scenario_payload;
    let ctx = test_ctx().await;
    let err = build_scenario_payload(&ctx, "no-such-scenario")
        .await
        .unwrap_err();
    assert!(
        matches!(err, xvision_engine::api::ApiError::NotFound(_)),
        "expected NotFound, got: {err:?}"
    );
}

// ── Task 2 — build_strategy_payload tests ───────────────────────────────────

/// A strategy id with no runs must return an empty `run_series`.
#[tokio::test]
async fn build_strategy_payload_empty_for_unused_strategy() {
    use xvision_engine::api::chart::build_strategy_payload;
    let ctx = test_ctx().await;
    let payload = build_strategy_payload(&ctx, "unused-strategy").await.unwrap();
    assert!(
        payload.run_series.is_empty(),
        "expected no runs for unused strategy"
    );
}

/// With 0 runs in the DB, the first missing id should return NotFound.
#[tokio::test]
async fn build_compare_payload_returns_not_found_for_missing_run() {
    let ctx = test_ctx().await;
    // Single missing id — should get NotFound (not panic or internal error).
    let ids = vec!["r_missing_1".to_string()];
    let err = xvision_engine::api::chart::build_compare_payload(&ctx, &ids)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound for missing run id, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("r_missing_1"),
        "error message should name the missing run id, got: {msg}"
    );
}

// ── Task 3 — build_scenario_preview tests ───────────────────────────────────

#[tokio::test]
async fn build_scenario_preview_validates_dates_and_assets() {
    let ctx = test_ctx().await;

    // Invalid date format.
    let err = xvision_engine::api::chart::build_scenario_preview(
        &ctx,
        xvision_engine::api::chart::PreviewQuery {
            asset: "ETH".into(),
            from: "not-a-date".into(),
            to: "2024-02-10".into(),
            granularity: "1h".into(),
            baseline: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, xvision_engine::api::ApiError::Validation(_)));

    // from >= to.
    let err = xvision_engine::api::chart::build_scenario_preview(
        &ctx,
        xvision_engine::api::chart::PreviewQuery {
            asset: "ETH".into(),
            from: "2024-02-10".into(),
            to: "2024-02-03".into(),
            granularity: "1h".into(),
            baseline: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, xvision_engine::api::ApiError::Validation(_)));

    // Unknown granularity.
    let err = xvision_engine::api::chart::build_scenario_preview(
        &ctx,
        xvision_engine::api::chart::PreviewQuery {
            asset: "ETH".into(),
            from: "2024-02-03".into(),
            to: "2024-02-10".into(),
            granularity: "banana".into(),
            baseline: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, xvision_engine::api::ApiError::Validation(_)));

    // Asset not on whitelist.
    let err = xvision_engine::api::chart::build_scenario_preview(
        &ctx,
        xvision_engine::api::chart::PreviewQuery {
            asset: "FAKE".into(),
            from: "2024-02-03".into(),
            to: "2024-02-10".into(),
            granularity: "1h".into(),
            baseline: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, xvision_engine::api::ApiError::Validation(_)));
}

#[tokio::test]
async fn build_scenario_preview_uncached_returns_not_cached_status() {
    let ctx = test_ctx().await;

    // Valid request, but cache is empty — should return NotCached
    // status with an empty bars vec (no Alpaca fetch attempted because
    // we short-circuit on NotCached).
    let payload = xvision_engine::api::chart::build_scenario_preview(
        &ctx,
        xvision_engine::api::chart::PreviewQuery {
            asset: "ETH".into(),
            from: "2024-02-03".into(),
            to: "2024-02-10".into(),
            granularity: "1h".into(),
            baseline: Some(true),
        },
    )
    .await
    .unwrap();

    assert_eq!(payload.asset, "ETH");
    assert_eq!(payload.granularity, "1h");
    assert!(payload.bars.is_empty(), "no cache → no bars");
    assert!(
        payload.baseline_equity.is_none(),
        "baseline depends on bars; empty bars → no baseline"
    );
    assert!(
        matches!(
            payload.cache_status,
            xvision_engine::api::chart::CacheStatus::NotCached { .. }
        ),
        "expected NotCached, got: {:?}",
        payload.cache_status
    );
    // Cache key should be deterministic — re-running with the same inputs
    // must produce the same key.
    assert!(!payload.cache_key.is_empty());
}

// ── include-set payload variants (pulse chart views) ────────────────────────

use chrono::Utc;
use xvision_engine::api::chart::IncludeSet;

/// Seed a completed backtest run with decisions + an equity curve against
/// the canonical scenario, with `bar_count` bars cached for ETH/USD.
async fn seed_backtest_run_with_equity(ctx: &ApiContext, bar_count: usize) -> String {
    let scenario = xvision_engine::api::scenario::get(ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    let cache_key = xvision_engine::eval::bars::compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    seed_cached_bars(ctx, &cache_key, "ETH/USD", bar_count).await;

    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "pulse-test-strategy".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    store
        .record_decision(&hold_decision_for_asset(&run.id, "ETH/USD"))
        .await
        .unwrap();

    // Equity samples aligned to the first three seeded bar hours
    // (seed_cached_bars starts at 2025-01-01T00:00Z, hourly).
    let t0 = chrono::TimeZone::with_ymd_and_hms(&Utc, 2025, 1, 1, 0, 0, 0).unwrap();
    let samples: Vec<(chrono::DateTime<Utc>, f64)> = (0..3)
        .map(|i| (t0 + chrono::Duration::hours(i), 100_000.0 + i as f64 * 500.0))
        .collect();
    store.record_equity_batch(&run.id, &samples).await.unwrap();
    run.id
}

#[tokio::test]
async fn include_equity_only_skips_bars_indicators_markers() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx, 8).await;

    let payload =
        xvision_engine::api::chart::build_run_payload_with(&ctx, &run_id, IncludeSet::parse("equity"))
            .await
            .unwrap();

    assert_eq!(payload.equity.len(), 3, "equity always ships");
    assert_eq!(payload.drawdown.len(), 3, "drawdown derives from equity");
    assert!(payload.bars.is_empty(), "equity-only must not ship bars");
    assert!(payload.indicators.sma_20.is_empty(), "indicators skipped");
    assert!(payload.indicators.macd.line.is_empty(), "indicators skipped");
    assert!(payload.markers.holds.is_empty(), "markers skipped");
    assert!(payload.position.is_empty(), "position skipped");
    assert!(payload.baseline_equity.is_none(), "baseline not requested");
}

#[tokio::test]
async fn include_bars_markers_skips_indicators_but_ships_candles() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx, 8).await;

    let payload =
        xvision_engine::api::chart::build_run_payload_with(&ctx, &run_id, IncludeSet::parse("bars,markers"))
            .await
            .unwrap();

    assert_eq!(payload.bars.len(), 8, "bars ship");
    assert_eq!(payload.markers.holds.len(), 1, "markers ship");
    assert!(payload.indicators.sma_20.is_empty(), "indicators skipped");
    assert!(payload.indicators.rsi_14.is_empty(), "indicators skipped");
    assert!(!payload.equity.is_empty(), "equity always ships");
}

#[tokio::test]
async fn include_baseline_ships_aligned_buy_and_hold() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx, 8).await;

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run_id,
        IncludeSet::parse("equity,baseline"),
    )
    .await
    .unwrap();

    let baseline = payload.baseline_equity.expect("baseline requested");
    assert_eq!(baseline.len(), payload.equity.len(), "aligned to equity");
    for (b, e) in baseline.iter().zip(payload.equity.iter()) {
        assert_eq!(b.time, e.time, "baseline sampled at equity timestamps");
    }
    // seed_cached_bars closes are base+1 with base=100+i → first close 101,
    // second 102: baseline[1] = 100k * 102/101.
    assert!((baseline[0].equity_usd - 100_000.0).abs() < 1e-6);
    assert!((baseline[1].equity_usd - 100_000.0 * (102.0 / 101.0)).abs() < 1e-6);
    assert!(payload.bars.is_empty(), "baseline mode does not SHIP bars");
}

#[tokio::test]
async fn baseline_degrades_to_none_when_equity_empty() {
    // A run with bars cached but NO equity seeded: compute_baseline_equity
    // returns None when the equity slice is empty. This verifies that
    // requesting baseline does not error — it degrades to None gracefully.
    let ctx = test_ctx().await;
    let scenario = xvision_engine::api::scenario::get(&ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    let cache_key = xvision_engine::eval::bars::compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    seed_cached_bars(&ctx, &cache_key, "ETH/USD", 4).await;

    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("live-strategy".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store
        .record_decision(&hold_decision_for_asset(&run.id, "ETH/USD"))
        .await
        .unwrap();
    // Intentionally NO equity seeded — baseline must degrade to None.

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run.id,
        IncludeSet::parse("equity,baseline"),
    )
    .await
    .unwrap();
    assert!(
        payload.baseline_equity.is_none(),
        "no equity → baseline is None, not an error"
    );
    assert!(payload.equity.is_empty(), "no equity seeded");
}

#[tokio::test]
async fn empty_scenario_early_return_baseline_is_null_not_error() {
    // A Backtest run with an empty scenario_id exercises the chart builder's
    // early-return branch at `run.scenario_id.is_empty()`, returning a
    // metric-only payload without bars, indicators, or baseline.
    //
    // `store::create` for Backtest binds scenario_id as Some(""), which the
    // runs_scenario_id_fk_insert trigger rejects (empty string IS NOT NULL, so
    // the trigger fires and can't find "" in scenarios). We bypass it by
    // dropping the trigger, inserting directly via raw SQL, then restoring the
    // trigger — the same technique used in `src/api/mod.rs::migrate_eval_runs_live_config`.
    let ctx = test_ctx().await;
    let run = Run::new_queued("metadata-only-strategy".into(), String::new(), RunMode::Backtest);
    sqlx::query("DROP TRIGGER IF EXISTS runs_scenario_id_fk_insert")
        .execute(&ctx.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, mode, status, started_at) \
         VALUES (?, ?, '', 'backtest', 'queued', ?)",
    )
    .bind(&run.id)
    .bind(&run.agent_id)
    .bind(run.started_at.to_rfc3339())
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS runs_scenario_id_fk_insert \
         BEFORE INSERT ON eval_runs \
         WHEN NEW.scenario_id IS NOT NULL \
         BEGIN \
           SELECT RAISE(ABORT, 'foreign-key violation: eval_runs.scenario_id does not exist in scenarios') \
           WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE id = NEW.scenario_id); \
         END",
    )
    .execute(&ctx.db)
    .await
    .unwrap();

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run.id,
        IncludeSet::parse("equity,baseline"),
    )
    .await
    .unwrap();

    assert!(
        payload.baseline_equity.is_none(),
        "early return ships no baseline"
    );
    assert!(payload.bars.is_empty(), "early return ships no bars");
    assert!(payload.equity.is_empty(), "no equity recorded for this run");
    assert!(
        payload.indicators.sma_20.is_empty(),
        "early return ships no indicators"
    );
}

#[tokio::test]
async fn full_payload_unchanged_and_baseline_absent() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx, 64).await;

    let payload = xvision_engine::api::chart::build_run_payload(&ctx, &run_id)
        .await
        .unwrap();
    assert_eq!(payload.bars.len(), 64);
    assert!(
        !payload.indicators.ema_20.is_empty(),
        "full payload computes indicators"
    );
    assert_eq!(payload.markers.holds.len(), 1);
    assert!(
        payload.baseline_equity.is_none(),
        "full mode never computes baseline"
    );
}
