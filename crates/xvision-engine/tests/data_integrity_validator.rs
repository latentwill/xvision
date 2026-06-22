//! Integration tests for the candle integrity validator (V2E, migration 027).
//!
//! Covers:
//! - One positive case per DataDefect variant
//! - ManifestMismatch refusal from compare_runs
//! - bars_content_hash byte-stable assertion
//! - --allow-defective-data (allow_manifest_mismatch) bypass
//! - DataManifest round-trip through Scenario::data_manifest

use chrono::{Duration, TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_data::manifest::{bars_content_hash, AdjustmentKind, DataManifest, FeedKind, SessionFilter};
use xvision_data::validate::{validate_ohlcv, CalendarHint, DataDefect, DefectSeverity, OhlcViolationKind};
use xvision_engine::eval::compare::{compare_runs, CompareOptions};
use xvision_engine::eval::findings::Finding;
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::store::RunStore;

use xvision_core::market::Ohlcv;

// ── DB helpers ─────────────────────────────────────────────────────────────────

async fn pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
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
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/017_eval_findings_review_columns.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Migration 026 — V2E trace-surface foundation (determinism_receipts,
    // findings.evidence_cycle_ids_json + findings.produced_by_check).
    sqlx::query(include_str!("../migrations/026_trace_surface_foundation.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Migration 027 — adds bars_content_hash, manifest_canonical, bars_manifest.
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
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
    pool
}

// ── Bar factory helpers ────────────────────────────────────────────────────────

fn bar(ts_hour: i64, o: f64, h: f64, l: f64, c: f64, v: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.with_ymd_and_hms(2024, 6, 3, 0, 0, 0).unwrap() + Duration::hours(ts_hour),
        open: o,
        high: h,
        low: l,
        close: c,
        volume: v,
    }
}

fn clean_bar(h: i64) -> Ohlcv {
    bar(h, 100.0, 101.0, 99.0, 100.5, 50.0)
}

// ── Validator: one positive case per defect kind ──────────────────────────────

#[test]
fn defect_non_monotonic_timestamp() {
    let bars = vec![clean_bar(2), clean_bar(1)];
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, DataDefect::NonMonotonicTimestamp { .. })),
        "expected NonMonotonicTimestamp; got: {defects:?}"
    );
    let d = defects
        .iter()
        .find(|d| matches!(d, DataDefect::NonMonotonicTimestamp { .. }))
        .unwrap();
    assert_eq!(d.severity(), DefectSeverity::Error);
}

#[test]
fn defect_duplicate_timestamp() {
    let bars = vec![clean_bar(1), clean_bar(1)];
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, DataDefect::DuplicateTimestamp { .. })),
        "expected DuplicateTimestamp; got: {defects:?}"
    );
    let d = defects
        .iter()
        .find(|d| matches!(d, DataDefect::DuplicateTimestamp { .. }))
        .unwrap();
    assert_eq!(d.severity(), DefectSeverity::Error);
}

#[test]
fn defect_missing_bar() {
    // hour 1 → hour 3: hour 2 is missing.
    let bars = vec![clean_bar(1), clean_bar(3)];
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    let missing: Vec<_> = defects
        .iter()
        .filter(|d| matches!(d, DataDefect::MissingBar { .. }))
        .collect();
    assert!(!missing.is_empty(), "expected MissingBar; got: {defects:?}");
    assert_eq!(missing[0].severity(), DefectSeverity::Warning);
    if let DataDefect::MissingBar { gap_bars, .. } = missing[0] {
        assert_eq!(*gap_bars, 1);
    }
}

#[test]
fn defect_ohlc_violation_low_above_open() {
    let bars = vec![bar(1, 100.0, 102.0, 101.0, 100.5, 50.0)]; // low=101 > open=100
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects.iter().any(|d| matches!(
            d,
            DataDefect::OhlcViolation {
                kind: OhlcViolationKind::LowAboveOpen,
                ..
            }
        )),
        "expected LowAboveOpen; got: {defects:?}"
    );
}

#[test]
fn defect_ohlc_violation_low_above_close() {
    let bars = vec![bar(1, 100.0, 102.0, 101.0, 100.0, 50.0)]; // low=101 > close=100
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects.iter().any(|d| matches!(
            d,
            DataDefect::OhlcViolation {
                kind: OhlcViolationKind::LowAboveClose,
                ..
            }
        )),
        "expected LowAboveClose; got: {defects:?}"
    );
}

#[test]
fn defect_ohlc_violation_high_below_open() {
    let bars = vec![bar(1, 100.0, 99.0, 97.0, 98.0, 50.0)]; // high=99 < open=100
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects.iter().any(|d| matches!(
            d,
            DataDefect::OhlcViolation {
                kind: OhlcViolationKind::HighBelowOpen,
                ..
            }
        )),
        "expected HighBelowOpen; got: {defects:?}"
    );
}

#[test]
fn defect_ohlc_violation_high_below_close() {
    let bars = vec![bar(1, 98.0, 99.0, 97.0, 100.0, 50.0)]; // high=99 < close=100
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects.iter().any(|d| matches!(
            d,
            DataDefect::OhlcViolation {
                kind: OhlcViolationKind::HighBelowClose,
                ..
            }
        )),
        "expected HighBelowClose; got: {defects:?}"
    );
}

#[test]
fn defect_ohlc_violation_high_below_low() {
    let bars = vec![bar(1, 99.0, 98.0, 100.0, 99.0, 50.0)]; // high=98 < low=100
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects.iter().any(|d| matches!(
            d,
            DataDefect::OhlcViolation {
                kind: OhlcViolationKind::HighBelowLow,
                ..
            }
        )),
        "expected HighBelowLow; got: {defects:?}"
    );
}

#[test]
fn defect_negative_or_nan_field() {
    let bars = vec![bar(1, -10.0, 1.0, 0.0, 0.5, 50.0)];
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, DataDefect::NegativeOrNanField { .. })),
        "expected NegativeOrNanField; got: {defects:?}"
    );
    let d = defects
        .iter()
        .find(|d| matches!(d, DataDefect::NegativeOrNanField { .. }))
        .unwrap();
    assert_eq!(d.severity(), DefectSeverity::Error);
}

#[test]
fn defect_zero_volume_bar() {
    let bars = vec![bar(1, 100.0, 101.0, 99.0, 100.5, 0.0)];
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, DataDefect::ZeroVolumeBar { .. })),
        "expected ZeroVolumeBar; got: {defects:?}"
    );
    let d = defects
        .iter()
        .find(|d| matches!(d, DataDefect::ZeroVolumeBar { .. }))
        .unwrap();
    assert_eq!(d.severity(), DefectSeverity::Info);
}

#[test]
fn defect_wick_shock_outlier() {
    let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = (0..200u32)
        .map(|i| Ohlcv {
            timestamp: base + Duration::hours(i as i64),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 50.0,
        })
        .collect();
    // Outlier: range = 200 vs median ~2 → sigma ≈ 100 >> 8.
    bars.push(Ohlcv {
        timestamp: base + Duration::hours(200),
        open: 100.0,
        high: 200.0,
        low: 0.0,
        close: 100.0,
        volume: 50.0,
    });
    let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, DataDefect::WickShockOutlier { .. })),
        "expected WickShockOutlier; got: {defects:?}"
    );
    let d = defects
        .iter()
        .find(|d| matches!(d, DataDefect::WickShockOutlier { .. }))
        .unwrap();
    assert_eq!(d.severity(), DefectSeverity::Warning);
}

// ── Finding construction from data defects ────────────────────────────────────

#[test]
fn finding_from_data_defect_has_correct_kind() {
    let defect = DataDefect::ZeroVolumeBar {
        at: 0,
        ts: Utc::now(),
    };
    let finding = Finding::from_data_defect("run-abc", &defect);
    assert_eq!(finding.kind, "data_defect");
    assert_eq!(finding.run_id, "run-abc");
    let evidence = &finding.evidence;
    assert_eq!(
        evidence.get("produced_by_check").and_then(|v| v.as_str()),
        Some("validator:ohlcv")
    );
    let cycles: &Vec<serde_json::Value> = evidence
        .get("evidence_cycle_ids")
        .and_then(|v| v.as_array())
        .unwrap();
    assert!(
        cycles.is_empty(),
        "evidence_cycle_ids must be empty for data defects"
    );
}

// ── bars_content_hash byte-stable assertion ────────────────────────────────────

#[test]
fn bars_content_hash_is_byte_stable() {
    let bytes = b"stable parquet payload for test";
    let h1 = bars_content_hash(bytes);
    let h2 = bars_content_hash(bytes);
    assert_eq!(h1, h2, "hash must be deterministic");
    assert_eq!(h1.len(), 64, "sha256 hex = 64 chars");
    assert_ne!(
        h1,
        bars_content_hash(b"different bytes"),
        "different bytes must produce different hashes"
    );
}

// ── DataManifest round-trip ───────────────────────────────────────────────────

#[test]
fn data_manifest_canonical_hash_is_stable() {
    let m = DataManifest {
        feed: FeedKind::Crypto,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let h1 = m.canonical_hash();
    let h2 = m.canonical_hash();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);
}

#[test]
fn data_manifest_different_feeds_produce_different_hashes() {
    let base = DataManifest {
        feed: FeedKind::Iex,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let other = DataManifest {
        feed: FeedKind::Sip,
        ..base.clone()
    };
    assert_ne!(base.canonical_hash(), other.canonical_hash());
}

// ── ManifestMismatch refusal ──────────────────────────────────────────────────

#[tokio::test]
async fn manifest_mismatch_returns_error() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);

    let manifest_a = DataManifest {
        feed: FeedKind::Iex,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let manifest_b = DataManifest {
        feed: FeedKind::Sip,
        ..manifest_a.clone()
    };

    let mut run_a = Run::new_queued("agent-a".into(), "sc-1".into(), RunMode::Backtest);
    run_a.manifest_canonical = Some(manifest_a.canonical_hash());
    run_a.bars_manifest = Some(serde_json::to_value(&manifest_a).unwrap());
    store.create(&run_a).await.unwrap();

    let mut run_b = Run::new_queued("agent-b".into(), "sc-1".into(), RunMode::Backtest);
    run_b.manifest_canonical = Some(manifest_b.canonical_hash());
    run_b.bars_manifest = Some(serde_json::to_value(&manifest_b).unwrap());
    store.create(&run_b).await.unwrap();

    // Without override → should get ManifestMismatch error.
    let result = compare_runs(
        &[run_a.id.clone(), run_b.id.clone()],
        &store,
        &CompareOptions {
            allow_manifest_mismatch: false,
        },
    )
    .await;
    assert!(result.is_err(), "expected ManifestMismatch error");
    let err = result.unwrap_err();
    let mismatch = err.downcast_ref::<xvision_engine::eval::compare::ManifestMismatch>();
    assert!(
        mismatch.is_some(),
        "error must downcast to ManifestMismatch; got: {err}"
    );
    let mm = mismatch.unwrap();
    // feed field should appear in diff_fields.
    assert!(
        mm.diff_fields.contains(&"feed".to_string()),
        "diff_fields must include 'feed': {:?}",
        mm.diff_fields
    );
}

#[tokio::test]
async fn allow_manifest_mismatch_bypasses_refusal() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);

    let manifest_a = DataManifest {
        feed: FeedKind::Iex,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let manifest_b = DataManifest {
        feed: FeedKind::Sip,
        ..manifest_a.clone()
    };

    let mut run_a = Run::new_queued("agent-a".into(), "sc-1".into(), RunMode::Backtest);
    run_a.manifest_canonical = Some(manifest_a.canonical_hash());
    run_a.bars_manifest = Some(serde_json::to_value(&manifest_a).unwrap());
    store.create(&run_a).await.unwrap();

    let mut run_b = Run::new_queued("agent-b".into(), "sc-1".into(), RunMode::Backtest);
    run_b.manifest_canonical = Some(manifest_b.canonical_hash());
    run_b.bars_manifest = Some(serde_json::to_value(&manifest_b).unwrap());
    store.create(&run_b).await.unwrap();

    // With override → should succeed.
    let result = compare_runs(
        &[run_a.id.clone(), run_b.id.clone()],
        &store,
        &CompareOptions {
            allow_manifest_mismatch: true,
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "allow_manifest_mismatch=true must bypass refusal; got: {:?}",
        result.err()
    );
}

// ── Store round-trip for manifest columns ─────────────────────────────────────

#[tokio::test]
async fn store_persists_and_reads_manifest_columns() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);

    let manifest = DataManifest {
        feed: FeedKind::Crypto,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let hash = "a".repeat(64);
    let canonical = manifest.canonical_hash();

    let mut run = Run::new_queued("agent-x".into(), "sc-2".into(), RunMode::Backtest);
    run.bars_content_hash = Some(hash.clone());
    run.manifest_canonical = Some(canonical.clone());
    run.bars_manifest = Some(serde_json::to_value(&manifest).unwrap());

    store.create(&run).await.unwrap();
    let loaded = store.get(&run.id).await.unwrap();

    assert_eq!(loaded.bars_content_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(loaded.manifest_canonical.as_deref(), Some(canonical.as_str()));
    assert!(loaded.bars_manifest.is_some());
}

// ── set_bars_manifest update path ─────────────────────────────────────────────

#[tokio::test]
async fn set_bars_manifest_updates_existing_run() {
    let pool = pool_with_migrations().await;
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-y".into(), "sc-3".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    let manifest = DataManifest {
        feed: FeedKind::Crypto,
        adjustment: AdjustmentKind::Raw,
        timeframe: "1Hour".to_string(),
        session_filter: SessionFilter::All,
        calendar: "Continuous24x7".to_string(),
        timezone: "UTC".to_string(),
    };
    let hash = "b".repeat(64);
    let canonical = manifest.canonical_hash();
    let manifest_value = serde_json::to_value(&manifest).unwrap();

    store
        .set_bars_manifest(&run.id, &hash, &canonical, &manifest_value)
        .await
        .unwrap();

    let loaded = store.get(&run.id).await.unwrap();
    assert_eq!(loaded.bars_content_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(loaded.manifest_canonical.as_deref(), Some(canonical.as_str()));
    assert!(loaded.bars_manifest.is_some());
}

// ── Scenario::data_manifest / calendar_hint ────────────────────────────────────

#[test]
fn scenario_data_manifest_derives_from_data_source() {
    use xvision_core::Capital;
    use xvision_data::alpaca::BarGranularity;
    use xvision_engine::eval::scenario::{
        AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
        LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
        TimeWindow, Venue, VenueSettings,
    };
    use xvision_engine::safety::VenueLabel;

    let s = Scenario {
        id: "sc-test".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: "test".into(),
        description: "".into(),
        tags: vec![],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
        },
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: Some("sip".into()),
            adjustment: AdjustmentMode::SplitAdjusted,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
            },
            slippage: xvision_engine::eval::scenario::SlippageModel::None,
            latency: LatencyModel {
                decision_to_fill_ms: 0,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::NextBarOpen,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: "k".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 200,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: "t".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    };

    let manifest = s.data_manifest();
    assert_eq!(manifest.feed, FeedKind::Sip);
    assert_eq!(manifest.adjustment, AdjustmentKind::SplitAdjusted);
    assert_eq!(manifest.timezone, "UTC");
    assert_eq!(manifest.calendar, "Continuous24x7");

    // canonical hash is deterministic
    let h1 = manifest.canonical_hash();
    let h2 = s.data_manifest().canonical_hash();
    assert_eq!(h1, h2);
}
