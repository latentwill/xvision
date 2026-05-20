//! Pure-compute metrics over an evaluation run's equity curve.
//!
//! The Phase 3.B-paper PaperExecutor records one equity sample per cadence
//! tick. This module turns that curve into Sharpe ratio, max drawdown, and
//! total return — replacing the 0.0 placeholders the paper executor used
//! before this module existed.
//!
//! Bootstrap CI helpers + per-decision-driven win-rate computation come
//! later (Phase 3.C compare + Phase 3.D); they need a real PnL-realized
//! pipeline that PaperExecutor does not yet emit.
//!
//! ## Net-of-inference-cost (V2E item 25)
//!
//! `compute_net_return_pct` computes `net_return_pct` from `gross_return_pct`,
//! `inference_cost_quote_total`, and `capital_initial`. Returns `None` when
//! `inference_cost_quote_total` is unknown. The math:
//!
//!   `net_return_pct = gross_return_pct − (inference_cost_quote_total / capital_initial × 100)`

use statrs::statistics::Statistics;

/// Convert a series of equity samples into per-period percentage returns.
///
/// `equity[i+1] / equity[i] - 1` for each adjacent pair where `equity[i] > 0`.
/// Pairs with zero or negative baselines are skipped (rather than producing
/// garbage `inf` / `-1.0` values that would corrupt downstream Sharpe math).
pub fn equity_to_returns(equity_samples: &[f64]) -> Vec<f64> {
    equity_samples
        .windows(2)
        .filter_map(|w| {
            if w[0] > 0.0 {
                Some((w[1] - w[0]) / w[0])
            } else {
                None
            }
        })
        .collect()
}

/// Annualized Sharpe ratio computed from per-period returns.
///
/// Sharpe = (mean(r) / std_dev(r)) * sqrt(periods_per_year). Returns `0.0`
/// when there are no returns or when standard deviation is zero (no
/// volatility = Sharpe is undefined; we surface 0 by convention so callers
/// don't have to special-case `NaN`).
pub fn sharpe_from_returns(returns: &[f64], periods_per_year: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    let mean = returns.mean();
    let std = returns.std_dev();
    // Guard against floating-point noise: when every return is mathematically
    // identical, statrs's sample-variance formula can return a value near
    // 1e-18 instead of exactly zero. Treat that as no volatility.
    if !std.is_finite() || std.abs() < 1e-12 {
        return 0.0;
    }
    (mean / std) * periods_per_year.sqrt()
}

/// Max drawdown across an equity curve, expressed as a percentage of the
/// running peak. Returns 0.0 for monotone-increasing curves and for curves
/// with fewer than 2 samples.
pub fn max_drawdown_pct(equity_samples: &[f64]) -> f64 {
    if equity_samples.len() < 2 {
        return 0.0;
    }
    let mut peak = f64::MIN;
    let mut max_dd = 0.0_f64;
    for &e in equity_samples {
        if e > peak {
            peak = e;
        }
        if peak > 0.0 {
            let dd = (peak - e) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd * 100.0
}

/// Total return as a percentage of the initial equity. Returns 0.0 when
/// `initial <= 0.0` rather than producing an undefined ratio.
pub fn total_return_pct(initial_equity: f64, final_equity: f64) -> f64 {
    if initial_equity <= 0.0 {
        return 0.0;
    }
    (final_equity - initial_equity) / initial_equity * 100.0
}

/// Annualization factor for Sharpe given a per-decision cadence in
/// minutes. 60-min cadence → 8760 (24 × 365); 15-min → 35040; daily → 365.
/// Returns 1.0 for non-positive inputs (avoids divide-by-zero downstream).
pub fn annualization_periods_per_year(cadence_minutes: u32) -> f64 {
    if cadence_minutes == 0 {
        return 1.0;
    }
    let minutes_per_year = 60.0 * 24.0 * 365.0;
    minutes_per_year / cadence_minutes as f64
}

/// Compute net return after deducting LLM inference cost.
///
/// `net_return_pct = gross_return_pct − (inference_cost_quote_total / capital_initial × 100)`
///
/// Returns `None` when `inference_cost_quote_total` is `None` (model not in
/// pricing catalog) or when `capital_initial ≤ 0` (undefined ratio).
///
/// The gross value is the trading P&L only; the deduction converts an absolute
/// USD cost into the same "% of starting capital" units so both sides of the
/// subtraction are comparable.
pub fn compute_net_return_pct(
    gross_return_pct: f64,
    inference_cost_quote_total: Option<f64>,
    capital_initial: f64,
) -> Option<f64> {
    let cost = inference_cost_quote_total?;
    if capital_initial <= 0.0 {
        return None;
    }
    Some(gross_return_pct - (cost / capital_initial * 100.0))
}

/// Default dominance threshold k: finding fires when
/// `|inference_cost_quote_total| > k × |gross_return_quote|`.
pub const INFERENCE_COST_DOMINANCE_THRESHOLD: f64 = 0.5;

/// Check whether inference cost dominates gross trading return.
///
/// Returns `true` when `|inference_cost_quote_total| > k × |gross_return_quote|`.
/// `k` defaults to [`INFERENCE_COST_DOMINANCE_THRESHOLD`].
/// When `gross_return_quote` is zero, any positive inference cost dominates
/// (ratio is infinite).
pub fn inference_cost_dominates(
    gross_return_quote: f64,
    inference_cost_quote_total: f64,
    threshold_k: f64,
) -> bool {
    let cost_abs = inference_cost_quote_total.abs();
    if gross_return_quote.abs() < f64::EPSILON {
        return cost_abs > f64::EPSILON;
    }
    cost_abs > threshold_k * gross_return_quote.abs()
}
