//! Regime-label derivation for scenario bar windows.
//!
//! This module derives a best-effort set of market regime labels from a
//! window of OHLCV bars.  The results are operator-overridable; the heuristics
//! will not always be correct, and that is by design.
//!
//! # Heuristic details
//!
//! ## trend_direction
//! Ordinary-least-squares slope over `close` prices.  We normalise the slope
//! by the first close so the threshold is percentage-based rather than
//! price-level dependent.
//!   - slope / first_close > 0.0005 per bar → `"up"`   (~0.05% per bar avg)
//!   - slope / first_close < −0.0005 per bar → `"down"`
//!   - otherwise → `"sideways"`
//!
//! ## volatility_label
//! Standard deviation of per-bar log-returns, binned into four crypto-tuned
//! buckets.  Thresholds are calibrated on hourly BTC/ETH data and deliberately
//! conservative — operators in different markets should override:
//!   - stddev < 0.005  ( < 0.5% / bar) → `"low"`
//!   - stddev < 0.020  ( < 2.0% / bar) → `"normal"`
//!   - stddev < 0.050  ( < 5.0% / bar) → `"high"`
//!   - stddev ≥ 0.050                  → `"extreme"`
//!
//! ## regime_label
//! Combines the other two labels plus a max-drawdown-from-peak test:
//!   - Max drawdown > 25 % from peak → `"crash"`
//!   - trend_direction = "up" + recovery signal (first-20% avg < last-20% avg
//!     by > 5% relative) → `"recovery"`
//!   - trend_direction = "up" → `"expansion"`
//!   - trend_direction = "sideways" → `"chop"`
//!   - otherwise → `"trend"`  (falling trend that didn't hit crash threshold)
//!
//! # Minimum bar count
//! At least 2 bars are required; a single bar produces all-None output.

use xvision_data::alpaca::MarketBar;

/// Output of [`derive_regime_labels`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegimeLabels {
    /// Broad market character: `"trend"` | `"chop"` | `"crash"` |
    /// `"expansion"` | `"recovery"` | `None` (< 2 bars).
    pub regime_label: Option<String>,
    /// Per-bar volatility bucket: `"low"` | `"normal"` | `"high"` |
    /// `"extreme"` | `None` (< 2 bars).
    pub volatility_label: Option<String>,
    /// Net price direction: `"up"` | `"down"` | `"sideways"` | `None`
    /// (< 2 bars).
    pub trend_direction: Option<String>,
}

/// Derive best-effort regime labels from a slice of bars.
///
/// Returns all-None when `bars.len() < 2` — there is not enough data to
/// compute a meaningful classification.
pub fn derive_regime_labels(bars: &[MarketBar]) -> RegimeLabels {
    if bars.len() < 2 {
        return RegimeLabels {
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
        };
    }

    let trend_direction = compute_trend_direction(bars);
    let volatility_label = compute_volatility_label(bars);
    let regime_label = compute_regime_label(bars, &trend_direction);

    RegimeLabels {
        regime_label: Some(regime_label),
        volatility_label: Some(volatility_label),
        trend_direction: Some(trend_direction),
    }
}

// ---------------------------------------------------------------------------
// Trend direction (OLS slope, normalised by first close)
// ---------------------------------------------------------------------------

/// Ordinary-least-squares slope of close prices over bar index.
/// Returns `slope / first_close` (dimensionless, per-bar).
fn ols_normalised_slope(bars: &[MarketBar]) -> f64 {
    let n = bars.len() as f64;
    // x = bar index [0, n-1], y = close price.
    let sum_x: f64 = (0..bars.len()).map(|i| i as f64).sum();
    let sum_y: f64 = bars.iter().map(|b| b.close).sum();
    let sum_xx: f64 = (0..bars.len()).map(|i| (i as f64) * (i as f64)).sum();
    let sum_xy: f64 = bars
        .iter()
        .enumerate()
        .map(|(i, b)| (i as f64) * b.close)
        .sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < f64::EPSILON {
        return 0.0;
    }
    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let first_close = bars[0].close;
    if first_close.abs() < f64::EPSILON {
        return 0.0;
    }
    slope / first_close
}

/// Classify net price direction.
///
/// Threshold: normalised slope > 0.0005/bar → `"up"`, < −0.0005/bar →
/// `"down"`, otherwise `"sideways"`.  This corresponds to roughly 0.05%
/// average move per bar, which on hourly data is about 1.2% per day.
fn compute_trend_direction(bars: &[MarketBar]) -> String {
    const UP_THRESHOLD: f64 = 0.0005;
    const DOWN_THRESHOLD: f64 = -0.0005;

    let slope = ols_normalised_slope(bars);
    if slope > UP_THRESHOLD {
        "up".to_string()
    } else if slope < DOWN_THRESHOLD {
        "down".to_string()
    } else {
        "sideways".to_string()
    }
}

// ---------------------------------------------------------------------------
// Volatility label (stddev of log-returns)
// ---------------------------------------------------------------------------

fn compute_volatility_label(bars: &[MarketBar]) -> String {
    // Crypto-tuned thresholds (per-bar log-return stddev).
    // Calibrated on hourly BTC/ETH; operators should override for other assets.
    //   < 0.5% / bar  → "low"
    //   < 2.0% / bar  → "normal"
    //   < 5.0% / bar  → "high"
    //   ≥ 5.0% / bar  → "extreme"
    const LOW_MAX: f64 = 0.005;
    const NORMAL_MAX: f64 = 0.020;
    const HIGH_MAX: f64 = 0.050;

    let stddev = log_return_stddev(bars);
    if stddev < LOW_MAX {
        "low".to_string()
    } else if stddev < NORMAL_MAX {
        "normal".to_string()
    } else if stddev < HIGH_MAX {
        "high".to_string()
    } else {
        "extreme".to_string()
    }
}

/// Standard deviation of per-bar log-returns.  Returns 0.0 when fewer than 2
/// consecutive valid pairs exist (degenerate input guard).
fn log_return_stddev(bars: &[MarketBar]) -> f64 {
    let returns: Vec<f64> = bars
        .windows(2)
        .filter_map(|w| {
            let prev = w[0].close;
            let curr = w[1].close;
            if prev > 0.0 && curr > 0.0 {
                Some((curr / prev).ln())
            } else {
                None
            }
        })
        .collect();

    if returns.is_empty() {
        return 0.0;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt()
}

// ---------------------------------------------------------------------------
// Max drawdown from peak
// ---------------------------------------------------------------------------

/// Maximum percentage drawdown from any running peak over the window.
/// Returns a value in [0, 1] where 1.0 = 100% drawdown.
fn max_drawdown(bars: &[MarketBar]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0_f64;
    for b in bars {
        let c = b.close;
        if c > peak {
            peak = c;
        }
        if peak > 0.0 {
            let dd = (peak - c) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

// ---------------------------------------------------------------------------
// Recovery signal
// ---------------------------------------------------------------------------

/// True when the window shows a V-shaped recovery pattern:
///
/// 1. The last 20% of bars averages more than 5% above the first 20% of bars
///    (the window ends higher than it started).
/// 2. There is a local minimum in the *first half* of the window that is at
///    least 5% below the first bar's close — evidence of a dip/trough that
///    the price then recovered from.
///
/// Condition 2 distinguishes recovery (V-shape: starts, dips, then rises)
/// from pure expansion (window starts low and rises steadily throughout with
/// no dip — there is no prior trough within the window).
fn looks_like_recovery(bars: &[MarketBar]) -> bool {
    let n = bars.len();
    let window = (n / 5).max(1);

    let first_avg = bars[..window]
        .iter()
        .map(|b| b.close)
        .sum::<f64>()
        / window as f64;
    let last_avg = bars[(n - window)..]
        .iter()
        .map(|b| b.close)
        .sum::<f64>()
        / window as f64;

    if first_avg <= 0.0 {
        return false;
    }

    // Condition 1: last segment > first segment by more than 5%.
    let upward = (last_avg - first_avg) / first_avg > 0.05;

    // Condition 2: there must be a trough in the first half of the window that
    // is at least 5% below the first bar's close (V-shape evidence).
    let first_close = bars[0].close;
    let half = (n / 2).max(1);
    let first_half_min = bars[..half]
        .iter()
        .map(|b| b.close)
        .fold(f64::INFINITY, f64::min);
    let has_dip = first_close > 0.0 && (first_close - first_half_min) / first_close > 0.05;

    upward && has_dip
}

// ---------------------------------------------------------------------------
// Regime label composition
// ---------------------------------------------------------------------------

fn compute_regime_label(bars: &[MarketBar], trend_direction: &str) -> String {
    // Crash takes priority: max drawdown > 25%.
    if max_drawdown(bars) > 0.25 {
        return "crash".to_string();
    }

    match trend_direction {
        "up" => {
            if looks_like_recovery(bars) {
                "recovery".to_string()
            } else {
                "expansion".to_string()
            }
        }
        "sideways" => "chop".to_string(),
        _ => "trend".to_string(), // "down" without crash threshold
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bar(close: f64) -> MarketBar {
        MarketBar {
            timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            open: close,
            high: close * 1.001,
            low: close * 0.999,
            close,
            volume: 1000.0,
        }
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn empty_bars_produces_all_none() {
        let result = derive_regime_labels(&[]);
        assert_eq!(result, RegimeLabels { regime_label: None, volatility_label: None, trend_direction: None });
    }

    #[test]
    fn single_bar_produces_all_none() {
        let result = derive_regime_labels(&[bar(100.0)]);
        assert_eq!(result, RegimeLabels { regime_label: None, volatility_label: None, trend_direction: None });
    }

    // ── trend_direction ───────────────────────────────────────────────────

    #[test]
    fn strong_uptrend_produces_trend_up() {
        // 100 bars rising 1% per bar → strong up.
        let bars: Vec<MarketBar> = (0..100)
            .map(|i| bar(100.0 * 1.01_f64.powi(i)))
            .collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.trend_direction.as_deref(), Some("up"));
    }

    #[test]
    fn strong_downtrend_produces_trend_down() {
        // 100 bars falling 1% per bar → down.
        let bars: Vec<MarketBar> = (0..100)
            .map(|i| bar(100.0 * 0.99_f64.powi(i)))
            .collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.trend_direction.as_deref(), Some("down"));
    }

    #[test]
    fn flat_price_produces_trend_sideways() {
        // 100 bars at constant price → sideways.
        let bars: Vec<MarketBar> = (0..100).map(|_| bar(100.0)).collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.trend_direction.as_deref(), Some("sideways"));
    }

    // ── volatility_label ─────────────────────────────────────────────────

    #[test]
    fn very_low_vol_produces_low_label() {
        // Tiny random noise around 100.0 → "low".
        // stddev ≈ 0.001 → < LOW_MAX (0.005).
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + ((i % 3) as f64 - 1.0) * 0.1)
            .collect();
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.volatility_label.as_deref(), Some("low"), "expected low vol");
    }

    #[test]
    fn extreme_vol_produces_extreme_label() {
        // Alternating ×2 and ÷2 on every bar → ~70% per-bar moves → extreme.
        let prices: Vec<f64> = (0..100)
            .map(|i| if i % 2 == 0 { 100.0 } else { 200.0 })
            .collect();
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.volatility_label.as_deref(), Some("extreme"));
    }

    // ── crash detection ───────────────────────────────────────────────────

    #[test]
    fn large_drawdown_produces_crash() {
        // Price rises to 200 then drops to 100 (50% drawdown) → crash.
        let mut prices: Vec<f64> = (0..50).map(|i| 100.0 + i as f64 * 2.0).collect(); // 100→198
        prices.extend((0..50).map(|i| 198.0 - i as f64 * 2.0)); // 198→100 (≈49% dd)
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.regime_label.as_deref(), Some("crash"));
    }

    #[test]
    fn small_drawdown_does_not_trigger_crash() {
        // 10% drawdown from peak — stays within the 25% threshold.
        let mut prices: Vec<f64> = (0..50).map(|i| 100.0 + i as f64 * 0.1).collect();
        prices.extend((0..50).map(|i| 105.0 - i as f64 * 0.1)); // mild pullback
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        assert_ne!(result.regime_label.as_deref(), Some("crash"));
    }

    // ── regime_label ─────────────────────────────────────────────────────

    #[test]
    fn bull_expansion_produces_expansion() {
        // 100 bars rising steadily, no crash, no recovery pattern.
        let bars: Vec<MarketBar> = (0..100)
            .map(|i| bar(100.0 + i as f64 * 0.5))
            .collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.trend_direction.as_deref(), Some("up"));
        assert_eq!(result.regime_label.as_deref(), Some("expansion"));
    }

    #[test]
    fn sideways_chop_produces_chop() {
        // Price oscillates but net is sideways → "chop".
        let prices: Vec<f64> = (0..100)
            .map(|i| 100.0 + ((i % 4) as f64 - 2.0) * 0.5)
            .collect();
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.regime_label.as_deref(), Some("chop"));
    }

    #[test]
    fn downtrend_without_crash_produces_trend() {
        // Gradual 15% decline (below 25% crash threshold) → "trend".
        let bars: Vec<MarketBar> = (0..100)
            .map(|i| bar(100.0 - i as f64 * 0.15))
            .collect();
        let result = derive_regime_labels(&bars);
        assert_eq!(result.trend_direction.as_deref(), Some("down"));
        assert_eq!(result.regime_label.as_deref(), Some("trend"));
    }

    #[test]
    fn recovery_pattern_produces_recovery() {
        // Classic V-shape: price starts at 100, dips to 70 in the first half
        // (30% dip), then recovers to 130 by the end.
        // trend_direction will be "up" (OLS slope is positive over the whole
        // window), and the first-half minimum (70) is > 5% below first_close
        // (100) → has_dip = true, last-20 avg >> first-20 avg.
        // V-shape: starts at 100, dips to 82 in first half (18% dip, below the
        // 25% crash threshold), then recovers to 130.  Max drawdown ≈ 18% < 25%
        // so no "crash" label.  has_dip: (100-82)/100 = 18% > 5% ✓.
        // last-20 avg ≈ 126 vs first-20 avg ≈ 98: (126-98)/98 ≈ 29% > 5% ✓.
        let n = 100usize;
        let mut prices: Vec<f64> = Vec::with_capacity(n);
        // Bars 0-49: decline from 100 to 82 (18% dip).
        prices.extend((0..50).map(|i| 100.0 - i as f64 * 0.36)); // 100 → 82.36
        // Bars 50-99: recovery from 82 to 130.
        prices.extend((0..50).map(|i| 82.0 + i as f64 * 0.96)); // 82 → 129.04
        let bars: Vec<MarketBar> = prices.into_iter().map(bar).collect();
        let result = derive_regime_labels(&bars);
        // OLS over 100 bars with first-half decline and steeper second-half rise
        // → net positive slope → trend_direction = "up".
        assert_eq!(result.trend_direction.as_deref(), Some("up"), "should be uptrend");
        assert_eq!(result.regime_label.as_deref(), Some("recovery"), "should detect recovery");
    }

    // ── integration: all labels present for 2+ bars ───────────────────────

    #[test]
    fn two_bars_produces_all_labels() {
        let bars = vec![bar(100.0), bar(101.0)];
        let result = derive_regime_labels(&bars);
        assert!(result.regime_label.is_some());
        assert!(result.volatility_label.is_some());
        assert!(result.trend_direction.is_some());
    }
}
