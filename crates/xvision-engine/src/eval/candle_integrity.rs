//! Candle-integrity validation for loaded OHLCV bar series.
//!
//! Two tiers per the 2026-06-02 synthetic-eval-fill-path spec (Decision 2):
//!
//! - **Structural corruption** → [`IntegrityError`] (hard-fail). Includes OHLC
//!   sanity violations, non-monotonic / duplicate timestamps, NaN / non-positive
//!   prices. The run must not proceed on corrupted bars.
//!
//! - **Gaps** → [`GapFinding`] (tolerated). Missing expected bars are flagged
//!   and the run continues, because a strategy should be evaluated against
//!   real-world data gaps.
//!
//! Entry point: [`validate_bar_series`].

use std::fmt;

use chrono::{DateTime, Utc};
use xvision_core::market::Ohlcv;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A gap in the bar series: one or more expected bars are missing.
#[derive(Debug, Clone, PartialEq)]
pub struct GapFinding {
    /// Timestamp of the last bar before the gap.
    pub gap_start_ts: DateTime<Utc>,
    /// Timestamp of the first bar after the gap.
    pub gap_end_ts: DateTime<Utc>,
    /// Number of bars that would be expected in `[gap_start_ts, gap_end_ts)`.
    pub expected_bars: u64,
}

impl fmt::Display for GapFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bars missing between {} and {}",
            self.expected_bars, self.gap_start_ts, self.gap_end_ts,
        )
    }
}

/// A structural corruption in the bar series that hard-fails the run.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityError {
    /// Timestamp of the offending bar.
    pub bar_ts: DateTime<Utc>,
    /// Human-readable description of the violation.
    pub kind: String,
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bar {} integrity violation: {}", self.bar_ts, self.kind)
    }
}

impl std::error::Error for IntegrityError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate a loaded bar series.
///
/// Returns `Ok(gaps)` when the series passes all structural checks. `gaps` may
/// be non-empty if the series has temporal holes (tolerated). Returns
/// `Err(IntegrityError)` on the first structural corruption detected — the
/// caller must not proceed with fills on a corrupted series.
///
/// `expected_step_secs`: the granularity of the series in seconds. Used for
/// gap detection. Pass `None` to skip gap detection (useful in tests).
pub fn validate_bar_series(
    bars: &[Ohlcv],
    expected_step_secs: Option<u64>,
) -> Result<Vec<GapFinding>, IntegrityError> {
    validate_ohlcv_structure(bars)?;
    validate_timestamps(bars)?;
    let gaps = match expected_step_secs {
        Some(step) if step > 0 => find_gaps(bars, step),
        _ => Vec::new(),
    };
    Ok(gaps)
}

// ---------------------------------------------------------------------------
// Internal helpers (each under 60 lines, NASA P10)
// ---------------------------------------------------------------------------

fn validate_ohlcv_structure(bars: &[Ohlcv]) -> Result<(), IntegrityError> {
    for bar in bars {
        check_prices_finite(bar)?;
        check_ohlc_sanity(bar)?;
    }
    Ok(())
}

fn check_prices_finite(bar: &Ohlcv) -> Result<(), IntegrityError> {
    let fields = [
        ("open", bar.open),
        ("high", bar.high),
        ("low", bar.low),
        ("close", bar.close),
        ("volume", bar.volume),
    ];
    for (name, val) in fields {
        if !val.is_finite() {
            return Err(IntegrityError {
                bar_ts: bar.timestamp,
                kind: format!("{name} is not finite ({val})"),
            });
        }
        if val <= 0.0 && name != "volume" {
            return Err(IntegrityError {
                bar_ts: bar.timestamp,
                kind: format!("{name} must be > 0 (got {val})"),
            });
        }
        if name == "volume" && val < 0.0 {
            return Err(IntegrityError {
                bar_ts: bar.timestamp,
                kind: format!("volume must be >= 0 (got {val})"),
            });
        }
    }
    Ok(())
}

fn check_ohlc_sanity(bar: &Ohlcv) -> Result<(), IntegrityError> {
    let max_oc = bar.open.max(bar.close);
    let min_oc = bar.open.min(bar.close);
    if bar.high < max_oc {
        return Err(IntegrityError {
            bar_ts: bar.timestamp,
            kind: format!(
                "high ({}) < max(open, close) ({})",
                bar.high, max_oc
            ),
        });
    }
    if bar.low > min_oc {
        return Err(IntegrityError {
            bar_ts: bar.timestamp,
            kind: format!(
                "low ({}) > min(open, close) ({})",
                bar.low, min_oc
            ),
        });
    }
    Ok(())
}

fn validate_timestamps(bars: &[Ohlcv]) -> Result<(), IntegrityError> {
    for window in bars.windows(2) {
        let prev = &window[0];
        let curr = &window[1];
        if curr.timestamp == prev.timestamp {
            return Err(IntegrityError {
                bar_ts: curr.timestamp,
                kind: "duplicate timestamp".to_string(),
            });
        }
        if curr.timestamp < prev.timestamp {
            return Err(IntegrityError {
                bar_ts: curr.timestamp,
                kind: format!(
                    "non-monotonic timestamp (prev={}, curr={})",
                    prev.timestamp, curr.timestamp
                ),
            });
        }
    }
    Ok(())
}

fn find_gaps(bars: &[Ohlcv], step_secs: u64) -> Vec<GapFinding> {
    let mut gaps = Vec::new();
    let step = step_secs as i64;
    for window in bars.windows(2) {
        let prev_ts = window[0].timestamp;
        let curr_ts = window[1].timestamp;
        let delta = (curr_ts - prev_ts).num_seconds();
        if delta > step {
            let missing = (delta / step).saturating_sub(1) as u64;
            if missing > 0 {
                gaps.push(GapFinding {
                    gap_start_ts: prev_ts,
                    gap_end_ts: curr_ts,
                    expected_bars: missing,
                });
            }
        }
    }
    gaps
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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

    #[test]
    fn clean_series_returns_ok_no_findings() {
        let bars = vec![hourly(0, 100.0), hourly(3600, 101.0), hourly(7200, 102.0)];
        let result = validate_bar_series(&bars, Some(3600));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn high_less_than_close_hard_fails() {
        let b = bar(0, 100.0, 99.0, 98.0, 100.0); // high < close
        let err = validate_bar_series(&[b], None).unwrap_err();
        assert!(err.kind.contains("high"), "expected high violation, got: {err}");
    }

    #[test]
    fn low_greater_than_open_hard_fails() {
        let b = bar(0, 100.0, 102.0, 101.0, 100.0); // low > open/close
        let err = validate_bar_series(&[b], None).unwrap_err();
        assert!(err.kind.contains("low"), "expected low violation, got: {err}");
    }

    #[test]
    fn nan_open_hard_fails() {
        let b = bar(0, f64::NAN, 102.0, 98.0, 100.0);
        let err = validate_bar_series(&[b], None).unwrap_err();
        assert!(err.kind.contains("open"));
    }

    #[test]
    fn negative_price_hard_fails() {
        let b = bar(0, -1.0, 1.0, -2.0, 0.5);
        let err = validate_bar_series(&[b], None).unwrap_err();
        assert!(err.kind.contains("open") || err.kind.contains("> 0"));
    }

    #[test]
    fn duplicate_timestamp_hard_fails() {
        let bars = vec![hourly(3600, 100.0), hourly(3600, 101.0)];
        let err = validate_bar_series(&bars, None).unwrap_err();
        assert!(err.kind.contains("duplicate"));
    }

    #[test]
    fn non_monotonic_timestamp_hard_fails() {
        let bars = vec![hourly(7200, 100.0), hourly(3600, 101.0)];
        let err = validate_bar_series(&bars, None).unwrap_err();
        assert!(err.kind.contains("non-monotonic"));
    }

    #[test]
    fn gap_in_hourly_series_emits_finding_and_does_not_fail() {
        let bars = vec![hourly(0, 100.0), hourly(7200, 101.0)]; // one bar missing at 3600
        let findings = validate_bar_series(&bars, Some(3600)).expect("should not fail");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].expected_bars, 1);
    }

    #[test]
    fn integrity_error_includes_bar_timestamp() {
        let ts = Utc.timestamp_opt(12_345, 0).unwrap();
        let b = Ohlcv {
            timestamp: ts,
            open: 100.0,
            high: 99.0, // violation
            low: 98.0,
            close: 100.0,
            volume: 1.0,
        };
        let err = validate_bar_series(&[b], None).unwrap_err();
        assert_eq!(err.bar_ts, ts);
    }
}
