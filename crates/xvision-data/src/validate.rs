//! OHLCV candle integrity validator.
//!
//! `validate_ohlcv` runs a battery of structural and statistical checks over a
//! slice of bars at the specified cadence. Returns a `Vec<DataDefect>` — the
//! empty vec means the bars passed all checks. Never panics.
//!
//! # Severity tiers
//!
//! | Tier    | Defects                                                      |
//! |---------|--------------------------------------------------------------|
//! | Error   | `NonMonotonicTimestamp`, `DuplicateTimestamp`,               |
//! |         | `OhlcViolation`, `NegativeOrNanField`                        |
//! | Warning | `MissingBar`, `WickShockOutlier`                             |
//! | Info    | `ZeroVolumeBar`                                              |
//!
//! A scenario with **any** `Error`-tier defect requires `--allow-defective-data`
//! to proceed. `Warning` and `Info` defects are surfaced as findings but do not
//! block execution.
//!
//! # Calendar-aware gap detection
//!
//! For equity scenarios (calendar `UsEquities` / `NYSE`), `MissingBar`
//! detection skips weekends and US federal holidays. For continuous crypto
//! scenarios (`Continuous24x7`), every bar at the cadence is expected.
//! Custom calendars fall back to continuous (conservative — fewer false
//! positives in edge cases).

use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use xvision_core::market::Ohlcv;

// ── Public types ─────────────────────────────────────────────────────────────

/// A single detected problem in an OHLCV bar slice.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "defect_kind", rename_all = "snake_case")]
pub enum DataDefect {
    /// A bar's timestamp is earlier than or equal to the previous bar's.
    NonMonotonicTimestamp {
        /// Zero-based index into the bar slice.
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        prev_ts: DateTime<Utc>,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        this_ts: DateTime<Utc>,
    },
    /// Two consecutive bars share the same timestamp.
    DuplicateTimestamp {
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        ts: DateTime<Utc>,
    },
    /// One or more bars are absent between the previous bar and the current
    /// bar at the expected cadence.
    MissingBar {
        /// Index of the bar *after* the gap (where the next bar was expected
        /// but was absent).
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        expected_ts: DateTime<Utc>,
        /// Number of bars that should have existed in the gap.
        gap_bars: u32,
    },
    /// A bar violates an OHLC invariant.
    OhlcViolation {
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        ts: DateTime<Utc>,
        kind: OhlcViolationKind,
    },
    /// A field contains a negative value or NaN.
    NegativeOrNanField {
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        ts: DateTime<Utc>,
        field: String,
    },
    /// A bar has zero volume. Common on illiquid overnight crypto pairs;
    /// emitted at `Info` severity rather than blocking.
    ZeroVolumeBar {
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        ts: DateTime<Utc>,
    },
    /// The bar's wick (high-low range) is an extreme outlier compared to the
    /// rolling 200-bar median range. Often indicates a feed glitch.
    ///
    /// `sigma = (high - low) / rolling_median_range`; threshold is 8.
    WickShockOutlier {
        at: usize,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        ts: DateTime<Utc>,
        sigma: f64,
    },
}

/// Specific OHLC invariant that was violated.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OhlcViolationKind {
    /// `low > open`
    LowAboveOpen,
    /// `low > close`
    LowAboveClose,
    /// `high < open`
    HighBelowOpen,
    /// `high < close`
    HighBelowClose,
    /// `high < low`
    HighBelowLow,
}

/// Per-defect severity tier.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectSeverity {
    Info,
    Warning,
    Error,
}

/// Calendar hint for gap detection.
///
/// Passed at call sites to let the validator skip non-trading periods for
/// equity scenarios without dragging in a full holiday library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarHint {
    /// Continuous 24×7 market — every bar at the cadence is expected.
    Continuous24x7,
    /// US equity session — weekends and federal holidays are skipped.
    UsEquities,
}

impl DataDefect {
    /// Returns the severity tier for this defect.
    pub fn severity(&self) -> DefectSeverity {
        match self {
            DataDefect::NonMonotonicTimestamp { .. }
            | DataDefect::DuplicateTimestamp { .. }
            | DataDefect::OhlcViolation { .. }
            | DataDefect::NegativeOrNanField { .. } => DefectSeverity::Error,

            DataDefect::MissingBar { .. } | DataDefect::WickShockOutlier { .. } => DefectSeverity::Warning,

            DataDefect::ZeroVolumeBar { .. } => DefectSeverity::Info,
        }
    }

    /// Returns the finding kind string used in `findings.jsonl`.
    pub fn finding_kind(&self) -> &'static str {
        "data_defect"
    }

    /// Returns the index into the bar slice where this defect was detected.
    pub fn at(&self) -> usize {
        match self {
            DataDefect::NonMonotonicTimestamp { at, .. }
            | DataDefect::DuplicateTimestamp { at, .. }
            | DataDefect::MissingBar { at, .. }
            | DataDefect::OhlcViolation { at, .. }
            | DataDefect::NegativeOrNanField { at, .. }
            | DataDefect::ZeroVolumeBar { at, .. }
            | DataDefect::WickShockOutlier { at, .. } => *at,
        }
    }
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Rolling window for the wick-shock outlier median computation.
const WICK_SHOCK_WINDOW: usize = 200;
/// Wick-shock outlier threshold in multiples of the rolling median range.
const WICK_SHOCK_SIGMA_THRESHOLD: f64 = 8.0;

// ── Main validator ────────────────────────────────────────────────────────────

/// Validate a slice of OHLCV bars at the expected `cadence`.
///
/// Returns every detected defect in bar-index order. An empty vec means all
/// checks passed. Never panics.
///
/// # Arguments
///
/// - `bars` — chronological slice of OHLCV bars (oldest first).
/// - `cadence` — expected time step between consecutive bars.
/// - `calendar` — controls whether non-trading periods are excluded from gap
///   detection. Pass `CalendarHint::Continuous24x7` for crypto; pass
///   `CalendarHint::UsEquities` for equity scenarios.
pub fn validate_ohlcv(bars: &[Ohlcv], cadence: Duration, calendar: CalendarHint) -> Vec<DataDefect> {
    let mut defects = Vec::new();

    // Precompute per-bar wick ranges for the wick-shock outlier check.
    let ranges: Vec<f64> = bars.iter().map(|b| (b.high - b.low).abs()).collect();

    for (i, bar) in bars.iter().enumerate() {
        // ── NaN / negative guard ──────────────────────────────────────────
        check_field_sanity(bar, i, &mut defects);

        // ── OHLC sanity ───────────────────────────────────────────────────
        check_ohlc(bar, i, &mut defects);

        // ── Zero volume (warn only) ───────────────────────────────────────
        if bar.volume == 0.0 {
            defects.push(DataDefect::ZeroVolumeBar {
                at: i,
                ts: bar.timestamp,
            });
        }

        if i == 0 {
            continue;
        }
        let prev = &bars[i - 1];

        // ── Monotonicity + duplicates ────────────────────────────────────
        if bar.timestamp <= prev.timestamp {
            if bar.timestamp == prev.timestamp {
                defects.push(DataDefect::DuplicateTimestamp {
                    at: i,
                    ts: bar.timestamp,
                });
            } else {
                defects.push(DataDefect::NonMonotonicTimestamp {
                    at: i,
                    prev_ts: prev.timestamp,
                    this_ts: bar.timestamp,
                });
            }
            // Skip gap detection for this pair — timestamps are already broken.
            continue;
        }

        // ── Gap detection ─────────────────────────────────────────────────
        check_gap(prev.timestamp, bar.timestamp, i, cadence, calendar, &mut defects);

        // ── Wick-shock outlier ────────────────────────────────────────────
        check_wick_shock(&ranges, i, bar, &mut defects);
    }

    defects
}

// ── Internal checks ────────────────────────────────────────────────────────────

fn check_field_sanity(bar: &Ohlcv, at: usize, defects: &mut Vec<DataDefect>) {
    let fields = [
        ("open", bar.open),
        ("high", bar.high),
        ("low", bar.low),
        ("close", bar.close),
        ("volume", bar.volume),
    ];
    for (name, val) in fields {
        if val.is_nan() || val < 0.0 {
            defects.push(DataDefect::NegativeOrNanField {
                at,
                ts: bar.timestamp,
                field: name.to_string(),
            });
        }
    }
}

fn check_ohlc(bar: &Ohlcv, at: usize, defects: &mut Vec<DataDefect>) {
    let checks: &[(bool, OhlcViolationKind)] = &[
        (bar.low > bar.open, OhlcViolationKind::LowAboveOpen),
        (bar.low > bar.close, OhlcViolationKind::LowAboveClose),
        (bar.high < bar.open, OhlcViolationKind::HighBelowOpen),
        (bar.high < bar.close, OhlcViolationKind::HighBelowClose),
        (bar.high < bar.low, OhlcViolationKind::HighBelowLow),
    ];
    for &(violated, kind) in checks {
        if violated {
            defects.push(DataDefect::OhlcViolation {
                at,
                ts: bar.timestamp,
                kind,
            });
        }
    }
}

fn check_gap(
    prev_ts: DateTime<Utc>,
    this_ts: DateTime<Utc>,
    at: usize,
    cadence: Duration,
    calendar: CalendarHint,
    defects: &mut Vec<DataDefect>,
) {
    // Count how many bars *should* have occurred between prev_ts and this_ts.
    let gap_bars = count_expected_bars(prev_ts, this_ts, cadence, calendar);
    // One gap_bar means no missing bars (exactly one step from prev → this).
    if gap_bars > 1 {
        let expected_ts = prev_ts + cadence;
        defects.push(DataDefect::MissingBar {
            at,
            expected_ts,
            gap_bars: (gap_bars - 1) as u32,
        });
    }
}

fn check_wick_shock(ranges: &[f64], i: usize, bar: &Ohlcv, defects: &mut Vec<DataDefect>) {
    let window_start = i.saturating_sub(WICK_SHOCK_WINDOW);
    let window = &ranges[window_start..i];
    if window.is_empty() {
        return;
    }
    let median = rolling_median(window);
    if median <= 0.0 {
        return;
    }
    let sigma = ranges[i] / median;
    if sigma > WICK_SHOCK_SIGMA_THRESHOLD {
        defects.push(DataDefect::WickShockOutlier {
            at: i,
            ts: bar.timestamp,
            sigma,
        });
    }
}

// ── Calendar-aware gap counting ────────────────────────────────────────────────

/// Count the number of cadence-sized steps from `from` (exclusive) to `to`
/// (inclusive), respecting the calendar. Returns 1 when no bar is missing
/// (i.e. `to == from + cadence`).
fn count_expected_bars(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    cadence: Duration,
    calendar: CalendarHint,
) -> i64 {
    match calendar {
        CalendarHint::Continuous24x7 => {
            let delta = (to - from).num_seconds();
            let step = cadence.num_seconds();
            if step <= 0 {
                return 1;
            }
            (delta / step).max(1)
        }
        CalendarHint::UsEquities => {
            // For sub-day cadences we count trading-session seconds.
            // For daily cadences we count trading days.
            count_us_equity_bars(from, to, cadence)
        }
    }
}

/// Count expected bars for a US equity calendar. Skips weekends and an
/// approximated set of US federal holidays.
fn count_us_equity_bars(from: DateTime<Utc>, to: DateTime<Utc>, cadence: Duration) -> i64 {
    let cadence_secs = cadence.num_seconds();
    if cadence_secs <= 0 {
        return 1;
    }

    // Daily cadence: count trading days.
    if cadence_secs >= 60 * 60 * 18 {
        let mut count = 0i64;
        let mut cursor = from + cadence;
        while cursor <= to {
            if is_us_trading_day(cursor) {
                count += 1;
            }
            cursor += Duration::days(1);
        }
        return count.max(1);
    }

    // Sub-day cadence (minutes, hours): count slots inside trading session.
    // NYSE regular session: 09:30–16:00 ET = 14:30–21:00 UTC.
    // We treat non-session slots as non-existent.
    let mut count = 0i64;
    let mut cursor = from + cadence;
    while cursor <= to {
        if is_us_trading_day(cursor) && is_within_session(cursor) {
            count += 1;
        }
        cursor += cadence;
    }
    count.max(1)
}

/// True if the given UTC timestamp falls on a US equity trading day
/// (Monday–Friday, not a federal holiday).
fn is_us_trading_day(dt: DateTime<Utc>) -> bool {
    let weekday = dt.weekday();
    if weekday == Weekday::Sat || weekday == Weekday::Sun {
        return false;
    }
    !is_us_federal_holiday(dt)
}

/// True if the UTC time falls within the NYSE regular session (14:30–21:00 UTC).
fn is_within_session(dt: DateTime<Utc>) -> bool {
    let hour = dt.hour();
    let minute = dt.minute();
    let total_minutes = hour * 60 + minute;
    // 14:30 UTC = 14*60+30 = 870
    // 21:00 UTC = 21*60 = 1260
    (870..1260).contains(&total_minutes)
}

/// Approximation of US federal holidays. Good enough for gap detection;
/// not a substitute for a full holiday calendar.
fn is_us_federal_holiday(dt: DateTime<Utc>) -> bool {
    let month = dt.month();
    let day = dt.day();
    let weekday = dt.weekday();

    match (month, day) {
        // New Year's Day (Jan 1, or nearest weekday)
        (1, 1) => true,
        // MLK Day (3rd Monday of January)
        // Presidents' Day (3rd Monday of February)
        // Memorial Day (last Monday of May)
        // Juneteenth (June 19)
        (6, 19) => true,
        // Independence Day (Jul 4, or nearest weekday)
        (7, 4) => true,
        (7, 3) if weekday == Weekday::Fri => true,
        (7, 5) if weekday == Weekday::Mon => true,
        // Labor Day (1st Monday of September) — approximated
        // Thanksgiving (4th Thursday of November) — approximated
        // Christmas (Dec 25, or nearest weekday)
        (12, 25) => true,
        (12, 24) if weekday == Weekday::Fri => true,
        (12, 26) if weekday == Weekday::Mon => true,
        _ => false,
    }
}

// ── Median helper ─────────────────────────────────────────────────────────────

/// Compute the median of a slice. Returns 0.0 for an empty slice.
fn rolling_median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

// ── Finding helper ─────────────────────────────────────────────────────────────

/// Convert a `DataDefect` to the `evidence` JSON blob used in `findings.jsonl`.
///
/// The `produced_by_check` field is always `"validator:ohlcv"`. The
/// `evidence_cycle_ids` field is always empty (data defects pre-exist the
/// cycle).
pub fn defect_to_finding_evidence(defect: &DataDefect) -> serde_json::Value {
    serde_json::json!({
        "produced_by_check": "validator:ohlcv",
        "evidence_cycle_ids": [],
        "defect": defect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use xvision_core::market::Ohlcv;

    fn bar(ts: DateTime<Utc>, o: f64, h: f64, l: f64, c: f64, v: f64) -> Ohlcv {
        Ohlcv {
            timestamp: ts,
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
        }
    }

    fn hour(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 6, 3, h, 0, 0).unwrap()
    }

    fn clean_bar(at_hour: u32) -> Ohlcv {
        bar(hour(at_hour), 100.0, 101.0, 99.0, 100.5, 50.0)
    }

    // ─── NonMonotonicTimestamp ─────────────────────────────────────────────────

    #[test]
    fn detects_non_monotonic_timestamp() {
        let bars = vec![
            clean_bar(1),
            clean_bar(0), // goes backward
        ];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::NonMonotonicTimestamp { .. })),
            "expected NonMonotonicTimestamp, got: {defects:?}"
        );
    }

    // ─── DuplicateTimestamp ────────────────────────────────────────────────────

    #[test]
    fn detects_duplicate_timestamp() {
        let bars = vec![
            clean_bar(1),
            clean_bar(1), // same timestamp
        ];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::DuplicateTimestamp { .. })),
            "expected DuplicateTimestamp, got: {defects:?}"
        );
    }

    // ─── MissingBar ────────────────────────────────────────────────────────────

    #[test]
    fn detects_missing_bar_continuous() {
        let bars = vec![
            clean_bar(1),
            clean_bar(3), // gap: hour 2 is missing
        ];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        let missing: Vec<_> = defects
            .iter()
            .filter(|d| matches!(d, DataDefect::MissingBar { .. }))
            .collect();
        assert_eq!(missing.len(), 1, "expected exactly one MissingBar: {defects:?}");
        if let DataDefect::MissingBar { gap_bars, .. } = missing[0] {
            assert_eq!(*gap_bars, 1);
        }
    }

    // ─── OhlcViolation ─────────────────────────────────────────────────────────

    #[test]
    fn detects_low_above_open() {
        let bars = vec![bar(hour(1), 100.0, 102.0, 101.0, 100.5, 50.0)]; // low > open
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects.iter().any(|d| matches!(
                d,
                DataDefect::OhlcViolation {
                    kind: OhlcViolationKind::LowAboveOpen,
                    ..
                }
            )),
            "expected LowAboveOpen: {defects:?}"
        );
    }

    #[test]
    fn detects_low_above_close() {
        // low > close: low=101 close=100
        let bars = vec![bar(hour(1), 100.0, 102.0, 101.0, 100.0, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects.iter().any(|d| matches!(
                d,
                DataDefect::OhlcViolation {
                    kind: OhlcViolationKind::LowAboveClose,
                    ..
                }
            )),
            "expected LowAboveClose: {defects:?}"
        );
    }

    #[test]
    fn detects_high_below_open() {
        // high < open: high=99 open=100
        let bars = vec![bar(hour(1), 100.0, 99.0, 97.0, 98.0, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects.iter().any(|d| matches!(
                d,
                DataDefect::OhlcViolation {
                    kind: OhlcViolationKind::HighBelowOpen,
                    ..
                }
            )),
            "expected HighBelowOpen: {defects:?}"
        );
    }

    #[test]
    fn detects_high_below_close() {
        // high < close: high=99 close=100
        let bars = vec![bar(hour(1), 98.0, 99.0, 97.0, 100.0, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects.iter().any(|d| matches!(
                d,
                DataDefect::OhlcViolation {
                    kind: OhlcViolationKind::HighBelowClose,
                    ..
                }
            )),
            "expected HighBelowClose: {defects:?}"
        );
    }

    #[test]
    fn detects_high_below_low() {
        // high < low: high=98 low=100
        let bars = vec![bar(hour(1), 99.0, 98.0, 100.0, 99.0, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects.iter().any(|d| matches!(
                d,
                DataDefect::OhlcViolation {
                    kind: OhlcViolationKind::HighBelowLow,
                    ..
                }
            )),
            "expected HighBelowLow: {defects:?}"
        );
    }

    // ─── NegativeOrNanField ────────────────────────────────────────────────────

    #[test]
    fn detects_negative_field() {
        let bars = vec![bar(hour(1), -1.0, 1.0, 0.0, 0.5, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::NegativeOrNanField { field, .. } if field == "open")),
            "expected NegativeOrNanField for open: {defects:?}"
        );
    }

    #[test]
    fn detects_nan_field() {
        let bars = vec![bar(hour(1), f64::NAN, 101.0, 99.0, 100.0, 50.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::NegativeOrNanField { .. })),
            "expected NegativeOrNanField: {defects:?}"
        );
    }

    // ─── ZeroVolumeBar ─────────────────────────────────────────────────────────

    #[test]
    fn detects_zero_volume_bar() {
        let bars = vec![bar(hour(1), 100.0, 101.0, 99.0, 100.5, 0.0)];
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::ZeroVolumeBar { .. })),
            "expected ZeroVolumeBar: {defects:?}"
        );
        // Must be Info severity
        let z = defects
            .iter()
            .find(|d| matches!(d, DataDefect::ZeroVolumeBar { .. }))
            .unwrap();
        assert_eq!(z.severity(), DefectSeverity::Info);
    }

    // ─── WickShockOutlier ──────────────────────────────────────────────────────

    #[test]
    fn detects_wick_shock_outlier() {
        // Build 200 normal bars (range ~2), then one extreme bar (range 200).
        let mut bars: Vec<Ohlcv> = (0..200u32)
            .map(|i| {
                let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + Duration::hours(i as i64);
                bar(ts, 100.0, 101.0, 99.0, 100.5, 50.0)
            })
            .collect();
        // outlier: range = 200 vs median ~2 → sigma = 100 >> 8
        let outlier_ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + Duration::hours(200);
        bars.push(bar(outlier_ts, 100.0, 200.0, 0.0, 100.0, 50.0));

        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(
            defects
                .iter()
                .any(|d| matches!(d, DataDefect::WickShockOutlier { .. })),
            "expected WickShockOutlier: {defects:?}"
        );
    }

    // ─── Clean bars ───────────────────────────────────────────────────────────

    #[test]
    fn clean_bars_return_no_defects() {
        let bars: Vec<Ohlcv> = (0..10).map(|i| clean_bar(i)).collect();
        let defects = validate_ohlcv(&bars, Duration::hours(1), CalendarHint::Continuous24x7);
        assert!(defects.is_empty(), "expected no defects: {defects:?}");
    }

    // ─── Severity tiers ───────────────────────────────────────────────────────

    #[test]
    fn severity_tiers_are_correct() {
        assert_eq!(
            DataDefect::NonMonotonicTimestamp {
                at: 0,
                prev_ts: hour(0),
                this_ts: hour(0),
            }
            .severity(),
            DefectSeverity::Error
        );
        assert_eq!(
            DataDefect::DuplicateTimestamp { at: 0, ts: hour(0) }.severity(),
            DefectSeverity::Error
        );
        assert_eq!(
            DataDefect::MissingBar {
                at: 1,
                expected_ts: hour(1),
                gap_bars: 1,
            }
            .severity(),
            DefectSeverity::Warning
        );
        assert_eq!(
            DataDefect::WickShockOutlier {
                at: 0,
                ts: hour(0),
                sigma: 10.0,
            }
            .severity(),
            DefectSeverity::Warning
        );
        assert_eq!(
            DataDefect::ZeroVolumeBar { at: 0, ts: hour(0) }.severity(),
            DefectSeverity::Info
        );
    }

    // ─── Median ───────────────────────────────────────────────────────────────

    #[test]
    fn rolling_median_odd_even() {
        assert_eq!(rolling_median(&[1.0, 3.0, 5.0]), 3.0);
        assert_eq!(rolling_median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(rolling_median(&[]), 0.0);
    }
}
