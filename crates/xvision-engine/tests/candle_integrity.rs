//! Integration tests for the candle-integrity pre-pass.
//! Covers M2 (integrity half) of the 2026-06-02 synthetic-eval-fill-path spec.

use chrono::{TimeZone, Utc};
use xvision_core::market::Ohlcv;
use xvision_engine::eval::candle_integrity::{validate_bar_series, GapFinding, IntegrityError};

fn bar(ts_secs: i64, o: f64, h: f64, l: f64, c: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.timestamp_opt(ts_secs, 0).unwrap(),
        open: o,
        high: h,
        low: l,
        close: c,
        volume: 1_000.0,
    }
}

fn hourly(ts_secs: i64, price: f64) -> Ohlcv {
    bar(ts_secs, price, price * 1.01, price * 0.99, price)
}

// ---------------------------------------------------------------------------
// Structural corruption → hard-fail
// ---------------------------------------------------------------------------

#[test]
fn high_less_than_close_hard_fails_with_ts() {
    let ts = Utc.timestamp_opt(3_600, 0).unwrap();
    let b = Ohlcv {
        timestamp: ts,
        open: 100.0,
        high: 99.0,  // violation: high < close
        low: 98.0,
        close: 100.0,
        volume: 1.0,
    };
    let err: IntegrityError = validate_bar_series(&[b], None).unwrap_err();
    assert_eq!(err.bar_ts, ts, "error must name the offending bar ts");
    assert!(err.kind.contains("high"), "kind must mention 'high': {}", err.kind);
}

#[test]
fn low_greater_than_open_hard_fails() {
    let b = bar(0, 100.0, 102.0, 101.0, 100.0); // low > min(open, close)
    let err = validate_bar_series(&[b], None).unwrap_err();
    assert!(err.kind.contains("low"), "kind must mention 'low': {}", err.kind);
}

#[test]
fn nan_open_hard_fails() {
    let b = Ohlcv {
        timestamp: Utc.timestamp_opt(0, 0).unwrap(),
        open: f64::NAN,
        high: 102.0,
        low: 98.0,
        close: 100.0,
        volume: 1.0,
    };
    let err = validate_bar_series(&[b], None).unwrap_err();
    assert!(err.kind.contains("open") || err.kind.contains("finite"),
        "NaN open must be detected: {}", err.kind);
}

#[test]
fn inf_close_hard_fails() {
    let b = Ohlcv {
        timestamp: Utc.timestamp_opt(0, 0).unwrap(),
        open: 100.0,
        high: f64::INFINITY,
        low: 98.0,
        close: f64::INFINITY,
        volume: 1.0,
    };
    let err = validate_bar_series(&[b], None).unwrap_err();
    assert!(err.kind.contains("finite") || err.kind.contains("high"),
        "Inf must be detected: {}", err.kind);
}

#[test]
fn zero_open_hard_fails() {
    let b = bar(0, 0.0, 1.0, 0.0, 0.5);
    let err = validate_bar_series(&[b], None).unwrap_err();
    assert!(err.kind.contains("> 0") || err.kind.contains("open"),
        "zero open must be rejected: {}", err.kind);
}

#[test]
fn negative_close_hard_fails() {
    let b = Ohlcv {
        timestamp: Utc.timestamp_opt(0, 0).unwrap(),
        open: -100.0,
        high: 0.0,
        low: -200.0,
        close: -50.0,
        volume: 1.0,
    };
    let err = validate_bar_series(&[b], None).unwrap_err();
    assert!(err.kind.contains("> 0") || err.kind.contains("open") || err.kind.contains("finite"),
        "negative price must be rejected: {}", err.kind);
}

#[test]
fn duplicate_timestamp_hard_fails() {
    let bars = vec![hourly(3_600, 100.0), hourly(3_600, 101.0)];
    let err = validate_bar_series(&bars, None).unwrap_err();
    assert!(err.kind.contains("duplicate"), "duplicate ts must be named: {}", err.kind);
}

#[test]
fn non_monotonic_timestamp_hard_fails() {
    let bars = vec![hourly(7_200, 100.0), hourly(3_600, 101.0)];
    let err = validate_bar_series(&bars, None).unwrap_err();
    assert!(err.kind.contains("non-monotonic"), "non-monotonic must be named: {}", err.kind);
}

// ---------------------------------------------------------------------------
// Gaps → tolerated, surfaced as findings
// ---------------------------------------------------------------------------

#[test]
fn single_gap_in_hourly_series_emits_one_finding() {
    // bars at t=0h, t=2h — one bar missing at t=1h
    let bars = vec![hourly(0, 100.0), hourly(7_200, 101.0)];
    let findings: Vec<GapFinding> = validate_bar_series(&bars, Some(3_600)).expect("should not fail");
    assert_eq!(findings.len(), 1, "one gap expected");
    assert_eq!(findings[0].expected_bars, 1);
    assert_eq!(findings[0].gap_start_ts, Utc.timestamp_opt(0, 0).unwrap());
    assert_eq!(findings[0].gap_end_ts, Utc.timestamp_opt(7_200, 0).unwrap());
}

#[test]
fn gap_does_not_fail_run() {
    let bars = vec![hourly(0, 100.0), hourly(7_200, 101.0)];
    assert!(
        validate_bar_series(&bars, Some(3_600)).is_ok(),
        "gap must not fail the run"
    );
}

#[test]
fn multiple_missing_bars_counted_correctly() {
    // 4h gap in hourly series → 3 bars missing
    let bars = vec![hourly(0, 100.0), hourly(14_400, 101.0)];
    let findings = validate_bar_series(&bars, Some(3_600)).unwrap();
    assert_eq!(findings[0].expected_bars, 3);
}

// ---------------------------------------------------------------------------
// Clean series
// ---------------------------------------------------------------------------

#[test]
fn clean_hourly_series_passes_with_no_findings() {
    let bars = (0..10).map(|i| hourly(i * 3_600, 100.0 + i as f64)).collect::<Vec<_>>();
    let result = validate_bar_series(&bars, Some(3_600));
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty(), "clean series must have no gap findings");
}

#[test]
fn single_bar_always_passes() {
    let bars = vec![hourly(0, 100.0)];
    let result = validate_bar_series(&bars, Some(3_600));
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn empty_series_passes() {
    let result = validate_bar_series(&[], Some(3_600));
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}
