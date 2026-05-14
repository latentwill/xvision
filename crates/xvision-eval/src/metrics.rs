//! Phase 8.1 + 8.3 — pure stat functions over return slices, plus the
//! `PreCommittedMetrics` aggregation struct.
//!
//! No I/O, no async. All functions operate on `&[f32]` (returns) or `&[f64]`
//! (equity curves denominated in USD). Tier 1 fix #8: returns use a constant
//! initial-NAV denominator so they are order-invariant.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use xvision_core::trading::Regime;

use crate::bootstrap::BootstrapResult;
use crate::result::BacktestResult;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error("arm '{0}' not found in BacktestResult")]
    ArmNotFound(String),
    #[error("arms must have the same number of returns (got {a} vs {b})")]
    LengthMismatch { a: usize, b: usize },
    #[error("bootstrap error: {0}")]
    Bootstrap(#[from] crate::bootstrap::BootstrapError),
}

// ---------------------------------------------------------------------------
// Pure stat functions
// ---------------------------------------------------------------------------

/// Tier 1 fix #8: convert a parallel slice of realized PnL values (in USD)
/// to returns using a constant initial NAV denominator. Order-invariant.
///
/// `pnls` is the per-setup net PnL. `nav_initial` is the starting equity.
/// Returns an empty `Vec` when `nav_initial == 0.0`.
pub fn returns_from_pnl(pnls: &[f32], nav_initial: f32) -> Vec<f32> {
    if nav_initial == 0.0 {
        return Vec::new();
    }
    pnls.iter().map(|&p| p / nav_initial).collect()
}

/// Annualised Sharpe ratio: `(mean(r) - rf) / std(r) * sqrt(periods_per_year)`.
///
/// Uses sample standard deviation (n-1 denominator).
/// Returns `0.0` when:
/// - `returns` is empty (n=0)
/// - `returns` has only one element (n=1, std undefined)
/// - standard deviation is zero (all returns identical)
///
/// Risk-free rate `rf` is assumed 0.0 in v1 (short-horizon crypto perps).
pub fn sharpe_annualized(returns: &[f32], periods_per_year: f32) -> f32 {
    let n = returns.len();
    if n < 2 {
        return 0.0;
    }
    let mean = returns.iter().copied().sum::<f32>() / n as f32;
    let var = returns
        .iter()
        .map(|&r| (r - mean) * (r - mean))
        .sum::<f32>()
        / (n - 1) as f32;
    let std = var.sqrt();
    // Guard: treat std as zero if it's not finite or if it is negligibly small
    // relative to the mean magnitude. f32 arithmetic on identical values
    // produces a tiny spurious variance (~1e-9) rather than exact zero; we
    // suppress that with a relative-to-mean threshold.
    let scale = mean.abs().max(1e-8);
    if !std.is_finite() || std / scale < 1e-5 {
        return 0.0;
    }
    mean / std * periods_per_year.sqrt()
}

/// Maximum drawdown as a positive percentage of peak NAV.
///
/// Walks the equity curve computing running peak → trough depth.
/// Returns `0.0` for a monotonically rising (or flat) curve.
pub fn max_drawdown_pct(equity_curve: &[f64]) -> f32 {
    if equity_curve.len() < 2 {
        return 0.0;
    }
    let mut peak = equity_curve[0];
    let mut max_dd = 0.0_f64;
    for &nav in equity_curve {
        if nav > peak {
            peak = nav;
        }
        if peak > 0.0 {
            let dd = (peak - nav) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }
    (max_dd * 100.0) as f32
}

/// Profit factor: `sum(positive returns) / |sum(negative returns)|`.
///
/// Returns `f32::INFINITY` when there are no negative returns (pure winners).
/// Returns `0.0` when there are no positive returns.
pub fn profit_factor(returns: &[f32]) -> f32 {
    let gross_profit: f32 = returns.iter().filter(|&&r| r > 0.0).sum();
    let gross_loss: f32 = returns.iter().filter(|&&r| r < 0.0).sum::<f32>().abs();
    if gross_loss == 0.0 {
        if gross_profit > 0.0 {
            f32::INFINITY
        } else {
            0.0
        }
    } else {
        gross_profit / gross_loss
    }
}

/// Win rate: fraction of returns that are strictly positive.
///
/// Returns `0.0` for an empty slice.
pub fn win_rate(returns: &[f32]) -> f32 {
    if returns.is_empty() {
        return 0.0;
    }
    let wins = returns.iter().filter(|&&r| r > 0.0).count();
    wins as f32 / returns.len() as f32
}

// ---------------------------------------------------------------------------
// Phase 8.3 — Pre-committed metrics structs
// ---------------------------------------------------------------------------

/// Per-regime Δ-Sharpe summary for the anti-overfit gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeMetrics {
    /// Number of setup evaluations in this regime stratum.
    pub n_cycles: usize,
    /// Bootstrapped Δ-Sharpe (arm_a − arm_b) for this regime.
    pub delta_sharpe: BootstrapResult,
    /// Name of the arm with the higher point-estimate Sharpe within this
    /// regime, or `None` if both are tied / the stratum is empty.
    pub winner: Option<String>,
}

/// All headline metrics committed before any human inspection of results.
/// Serialisable so they can be written to disk alongside the `BacktestResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCommittedMetrics {
    /// Primary: bootstrapped Δ-Sharpe between arm_a and arm_b.
    pub delta_sharpe: BootstrapResult,
    /// Maximum drawdown (%) per arm.
    pub max_drawdown_pct: BTreeMap<String, f32>,
    /// Profit factor per arm.
    pub profit_factor: BTreeMap<String, f32>,
    /// Win rate per arm.
    pub win_rate: BTreeMap<String, f32>,
    /// Fraction of paired cycles where `(action, direction, size_bucket)`
    /// differs between arm_a and arm_b.
    pub decision_divergence_rate: f32,
    /// Δ-Sharpe stratified by regime. Keyed by `Regime` (HashMap because
    /// `Regime` does not implement `Ord`).
    pub regime_stratified: HashMap<Regime, RegimeMetrics>,
}

/// Compute the full `PreCommittedMetrics` from a finished `BacktestResult`.
///
/// `arm_a` / `arm_b` are the arm names to compare (both must be present).
/// `n_resamples` and `block_size` are forwarded to the paired bootstrap.
pub fn compute_pre_committed(
    result: &BacktestResult,
    arm_a: &str,
    arm_b: &str,
    n_resamples: usize,
    block_size: Option<usize>,
) -> Result<PreCommittedMetrics, MetricsError> {
    use crate::bootstrap::paired_bootstrap_sharpe_delta;

    let a = result
        .arms
        .get(arm_a)
        .ok_or_else(|| MetricsError::ArmNotFound(arm_a.to_owned()))?;
    let b = result
        .arms
        .get(arm_b)
        .ok_or_else(|| MetricsError::ArmNotFound(arm_b.to_owned()))?;

    if a.returns.len() != b.returns.len() {
        return Err(MetricsError::LengthMismatch {
            a: a.returns.len(),
            b: b.returns.len(),
        });
    }

    // Periods per year: assume hourly returns scaled to 8_760 h/yr.
    // The constant is immaterial for the bootstrap comparison; the caller may
    // override by interpreting the result directly.
    const PERIODS_PER_YEAR: f32 = 8_760.0;
    const SEED: u64 = 0xdeadbeef_cafef00d;

    // Primary Δ-Sharpe bootstrap
    let delta_sharpe = paired_bootstrap_sharpe_delta(
        &a.returns,
        &b.returns,
        n_resamples,
        block_size,
        PERIODS_PER_YEAR,
        SEED,
    )?;

    // Per-arm secondary metrics
    let mut max_dd = BTreeMap::new();
    let mut pf = BTreeMap::new();
    let mut wr = BTreeMap::new();
    for (name, arm) in &result.arms {
        let navs: Vec<f64> = arm.equity_curve.iter().map(|ep| ep.nav_usd).collect();
        max_dd.insert(name.clone(), max_drawdown_pct(&navs));
        pf.insert(name.clone(), profit_factor(&arm.returns));
        wr.insert(name.clone(), win_rate(&arm.returns));
    }

    // Decision divergence rate — paired by cycle_id index
    let divergence_rate = compute_divergence_rate(a, b);

    // Regime-stratified Δ-Sharpe
    let regime_stratified = compute_regime_stratified(a, b, n_resamples, block_size, PERIODS_PER_YEAR, SEED)?;

    Ok(PreCommittedMetrics {
        delta_sharpe,
        max_drawdown_pct: max_dd,
        profit_factor: pf,
        win_rate: wr,
        decision_divergence_rate: divergence_rate,
        regime_stratified,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Decision divergence rate: walks decisions of arm_a and arm_b paired by
/// index (same cycle ordering guaranteed by the harness). Denominator is the
/// total number of paired cycles from arm_a (not union of both arms).
fn compute_divergence_rate(
    a: &crate::result::ArmResult,
    b: &crate::result::ArmResult,
) -> f32 {
    let n = a.decisions.len().min(b.decisions.len());
    if n == 0 {
        return 0.0;
    }
    let diverged = a
        .decisions
        .iter()
        .zip(b.decisions.iter())
        .filter(|(da, db)| da.divergence_key() != db.divergence_key())
        .count();
    diverged as f32 / n as f32
}

/// Stratify returns and regimes by `Regime`, run a per-regime bootstrap.
fn compute_regime_stratified(
    a: &crate::result::ArmResult,
    b: &crate::result::ArmResult,
    n_resamples: usize,
    block_size: Option<usize>,
    periods_per_year: f32,
    seed: u64,
) -> Result<HashMap<Regime, RegimeMetrics>, MetricsError> {
    use crate::bootstrap::paired_bootstrap_sharpe_delta;

    // Collect (ret_a, ret_b) pairs per regime, using arm_a's regime labels.
    let mut buckets: HashMap<Regime, (Vec<f32>, Vec<f32>)> = HashMap::new();
    let n = a.returns.len().min(b.returns.len()).min(a.regimes.len());
    for i in 0..n {
        let regime = a.regimes[i];
        let entry = buckets.entry(regime).or_insert_with(|| (Vec::new(), Vec::new()));
        entry.0.push(a.returns[i]);
        entry.1.push(b.returns[i]);
    }

    let mut out: HashMap<Regime, RegimeMetrics> = HashMap::new();
    for (regime, (ra, rb)) in buckets {
        let n_cycles = ra.len();
        // Need at least 2 samples for a meaningful bootstrap
        if n_cycles < 2 {
            // Build a degenerate result
            let sharpe_a = sharpe_annualized(&ra, periods_per_year);
            let sharpe_b = sharpe_annualized(&rb, periods_per_year);
            let pe = sharpe_a - sharpe_b;
            let winner = if sharpe_a > sharpe_b {
                Some(a.name.clone())
            } else if sharpe_b > sharpe_a {
                Some(b.name.clone())
            } else {
                None
            };
            out.insert(
                regime,
                RegimeMetrics {
                    n_cycles,
                    delta_sharpe: BootstrapResult {
                        point_estimate: pe,
                        ci_low: pe,
                        ci_high: pe,
                        n_resamples: 0,
                        block_size,
                    },
                    winner,
                },
            );
            continue;
        }

        let bootstrap = paired_bootstrap_sharpe_delta(
            &ra,
            &rb,
            n_resamples,
            block_size,
            periods_per_year,
            seed ^ (regime as u64).wrapping_mul(0x9e3779b97f4a7c15),
        )?;

        let sharpe_a = sharpe_annualized(&ra, periods_per_year);
        let sharpe_b = sharpe_annualized(&rb, periods_per_year);
        let winner = if sharpe_a > sharpe_b {
            Some(a.name.clone())
        } else if sharpe_b > sharpe_a {
            Some(b.name.clone())
        } else {
            None
        };

        out.insert(
            regime,
            RegimeMetrics {
                n_cycles,
                delta_sharpe: bootstrap,
                winner,
            },
        );
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use xvision_core::trading::{Action, Direction, Regime, TraderDecision};

    // -----------------------------------------------------------------------
    // returns_from_pnl
    // -----------------------------------------------------------------------

    #[test]
    fn returns_from_pnl_basic() {
        let pnls = vec![100.0_f32, -50.0, 200.0];
        let nav = 1_000.0_f32;
        let ret = returns_from_pnl(&pnls, nav);
        assert_eq!(ret.len(), 3);
        assert!((ret[0] - 0.1).abs() < 1e-6);
        assert!((ret[1] - (-0.05)).abs() < 1e-6);
        assert!((ret[2] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn returns_from_pnl_zero_nav_returns_empty() {
        let ret = returns_from_pnl(&[100.0], 0.0);
        assert!(ret.is_empty());
    }

    #[test]
    fn returns_from_pnl_constant_denominator_order_invariant() {
        let pnls_a = vec![100.0_f32, -50.0];
        let pnls_b = vec![-50.0_f32, 100.0];
        let nav = 500.0;
        let ra = returns_from_pnl(&pnls_a, nav);
        let rb = returns_from_pnl(&pnls_b, nav);
        // Sum of returns must be identical regardless of order
        let sum_a: f32 = ra.iter().sum();
        let sum_b: f32 = rb.iter().sum();
        assert!((sum_a - sum_b).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // sharpe_annualized
    // -----------------------------------------------------------------------

    #[test]
    fn sharpe_returns_zero_for_empty() {
        assert_eq!(sharpe_annualized(&[], 252.0), 0.0);
    }

    #[test]
    fn sharpe_returns_zero_for_single_element() {
        assert_eq!(sharpe_annualized(&[0.05], 252.0), 0.0);
    }

    #[test]
    fn sharpe_returns_zero_for_zero_std() {
        // All identical returns → std = 0
        let returns = vec![0.01_f32; 10];
        assert_eq!(sharpe_annualized(&returns, 252.0), 0.0);
    }

    #[test]
    fn sharpe_basic_positive() {
        // Constant positive return with tiny noise → positive Sharpe
        let returns: Vec<f32> = (0..100).map(|i| 0.01 + (i as f32) * 0.0001).collect();
        let s = sharpe_annualized(&returns, 252.0);
        assert!(s > 0.0, "Sharpe should be positive for consistently positive returns");
    }

    // -----------------------------------------------------------------------
    // max_drawdown_pct
    // -----------------------------------------------------------------------

    #[test]
    fn max_drawdown_monotonic_up_is_zero() {
        let curve = vec![100.0_f64, 110.0, 120.0, 130.0];
        assert_eq!(max_drawdown_pct(&curve), 0.0);
    }

    #[test]
    fn max_drawdown_basic() {
        // Peak 100, drops to 80 → 20% drawdown
        let curve = vec![80.0_f64, 100.0, 80.0, 90.0];
        let dd = max_drawdown_pct(&curve);
        assert!((dd - 20.0).abs() < 0.01, "expected ~20% drawdown, got {dd}");
    }

    #[test]
    fn max_drawdown_single_element_is_zero() {
        assert_eq!(max_drawdown_pct(&[100.0]), 0.0);
    }

    // -----------------------------------------------------------------------
    // profit_factor
    // -----------------------------------------------------------------------

    #[test]
    fn profit_factor_no_losses_is_infinity() {
        let returns = vec![0.1_f32, 0.2, 0.05];
        assert_eq!(profit_factor(&returns), f32::INFINITY);
    }

    #[test]
    fn profit_factor_no_wins_is_zero() {
        let returns = vec![-0.1_f32, -0.05];
        assert_eq!(profit_factor(&returns), 0.0);
    }

    #[test]
    fn profit_factor_mixed() {
        // gains = 0.3, losses = 0.1 → pf = 3.0
        let returns = vec![0.2_f32, 0.1, -0.1];
        let pf = profit_factor(&returns);
        assert!((pf - 3.0).abs() < 1e-5, "expected 3.0, got {pf}");
    }

    // -----------------------------------------------------------------------
    // win_rate
    // -----------------------------------------------------------------------

    #[test]
    fn win_rate_empty_is_zero() {
        assert_eq!(win_rate(&[]), 0.0);
    }

    #[test]
    fn win_rate_all_wins() {
        assert!((win_rate(&[0.1, 0.2, 0.3]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn win_rate_mixed() {
        // 2 wins out of 4
        let returns = vec![0.1_f32, -0.05, 0.2, -0.1];
        assert!((win_rate(&returns) - 0.5).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // decision_divergence_rate (via compute_divergence_rate directly)
    // -----------------------------------------------------------------------

    fn make_decision(action: Action, direction: Direction, size_bps: u32) -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::new_v4(),
            action,
            size_bps,
            direction,
            stop_loss_pct: 2.0,
            take_profit_pct: 4.0,
            trader_summary: "test decision fixture for divergence test".into(),
            asset: None,
        }
    }

    fn make_arm(decisions: Vec<TraderDecision>, returns: Vec<f32>) -> crate::result::ArmResult {
        let n = returns.len();
        crate::result::ArmResult {
            name: "test".into(),
            equity_curve: vec![],
            fills: vec![],
            decisions,
            risk_outcomes: vec![],
            returns,
            realized_pnl_total_usd: 0.0,
            regimes: vec![Regime::Chop; n],
        }
    }

    #[test]
    fn divergence_rate_identical_decisions_zero() {
        let d = make_decision(Action::Buy, Direction::Long, 500);
        let a = make_arm(vec![d.clone(), d.clone()], vec![0.01, 0.02]);
        let b = make_arm(vec![d.clone(), d.clone()], vec![0.01, 0.02]);
        assert_eq!(compute_divergence_rate(&a, &b), 0.0);
    }

    #[test]
    fn divergence_rate_all_different() {
        let d_buy = make_decision(Action::Buy, Direction::Long, 500);
        let d_sell = make_decision(Action::Sell, Direction::Short, 500);
        let a = make_arm(vec![d_buy.clone(), d_buy.clone()], vec![0.01, 0.02]);
        let b = make_arm(vec![d_sell.clone(), d_sell.clone()], vec![0.01, 0.02]);
        assert!((compute_divergence_rate(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn divergence_rate_half_different() {
        let d_buy = make_decision(Action::Buy, Direction::Long, 500);
        let d_sell = make_decision(Action::Sell, Direction::Short, 500);
        let a = make_arm(vec![d_buy.clone(), d_buy.clone()], vec![0.01, 0.02]);
        let b = make_arm(vec![d_buy.clone(), d_sell.clone()], vec![0.01, 0.02]);
        assert!((compute_divergence_rate(&a, &b) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn divergence_rate_empty_is_zero() {
        let a = make_arm(vec![], vec![]);
        let b = make_arm(vec![], vec![]);
        assert_eq!(compute_divergence_rate(&a, &b), 0.0);
    }
}
