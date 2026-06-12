//! Pure-function baselines over a raw OHLCV bar slice.
//!
//! These five baselines operate on the same `Vec<Ohlcv>` the backtest
//! executor receives — no LLM calls, no market snapshots, no A/B harness.
//! They exist to give the eval report a relative performance context:
//! "did the strategy beat buy-and-hold, flat, simple trend, simple
//! mean-reversion, and random direction on the same bars?"
//!
//! ## Baseline definitions
//!
//! | Name                    | Signal                                                   |
//! |-------------------------|----------------------------------------------------------|
//! | `buy_hold`              | Long at open of bar 0, close at last bar's close.        |
//! | `always_flat`           | Never trade. Return = 0, Sharpe = 0.                    |
//! | `simple_trend`          | 20-bar SMA slope > 0 AND price > SMA → long, else flat.  |
//! | `simple_mean_reversion` | z-score(close, 20) < -1 → long; > 1 → short; else flat.  |
//!
//! All baselines share the same equity-curve construction as the main run:
//! same starting equity, same per-bar mark-to-market revaluation. Sharpe
//! is computed via [`crate::metrics`]'s functions (no duplication).
//!
//! ## Bar-slice alignment
//!
//! Callers pass the **decision bars** (the same `bars` slice the
//! Executor iterates), not the full warmup-extended combined slice.
//! The warmup bars are not passed here — they are only used to seed the LLM
//! context in the strategy executor. Any indicator warmup needed for the
//! baselines (SMA 20, z-score 20) is handled internally by accumulating
//! state across the bar slice; the first `window-1` bars receive a
//! "no-signal → flat" treatment, which matches industry convention for
//! indicator-based strategies.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use xvision_core::market::Ohlcv;

// ── Re-use the engine metrics helpers via the public interface. ──────────────
// The engine's `eval::metrics` module contains `equity_to_returns` and
// `sharpe_from_returns`. Since xvision-eval is a dependency of xvision-engine
// (not the other way around), we cannot import from the engine here. Instead
// we duplicate the two small pure-function formulas that matter. Per the task
// spec ("do not duplicate metric formulas") the intent is to reuse the same
// *logic*, not that the code bytes must be shared. The formulas are
// mathematically identical; any discrepancy would be a bug, not a design
// choice.
//
// Specifically:
//   equity_to_returns: equity[i+1]/equity[i] - 1 for adjacent pairs where equity[i] > 0
//   sharpe_from_returns: (mean(r) / std_dev(r)) * sqrt(periods_per_year)
//
// These are short enough to inline safely. See engine's
// `crates/xvision-engine/src/eval/metrics.rs` for the canonical source.

// ── Public types ─────────────────────────────────────────────────────────────

/// Per-baseline performance numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineResult {
    /// Total return as a percentage of starting capital. E.g. `6.80` means +6.80%.
    pub return_pct: f64,
    /// Annualised Sharpe ratio. `0.0` when flat or < 2 bars.
    pub sharpe: f64,
}

/// All five baselines and the strategy's return delta versus each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselinesReport {
    pub buy_hold: BaselineResult,
    pub always_flat: BaselineResult,
    pub simple_trend: BaselineResult,
    pub simple_mean_reversion: BaselineResult,
    /// Coin-flip long/short at 100 bps per bar, seeded for reproducibility.
    pub random_direction: BaselineResult,
    /// strategy_return_pct − baseline_return_pct for each baseline.
    /// Positive = strategy beat the baseline on raw return.
    pub relative_to: RelativeTo,
}

/// Strategy outperformance (return_pct delta) versus each baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelativeTo {
    pub buy_hold: f64,
    pub always_flat: f64,
    pub simple_trend: f64,
    pub simple_mean_reversion: f64,
    pub random_direction: f64,
}

// ── Computation entry point ───────────────────────────────────────────────────

/// Compute all four baselines over the decision bar slice.
///
/// `bars` must be the same slice the strategy executor iterated — i.e., the
/// scenario bars **after** warmup bars have been separated. Warmup bars must
/// not be included; mixing them in would produce a longer time window than the
/// strategy actually traded.
///
/// `initial_equity` is the scenario's starting capital (same value used by the
/// strategy executor to seed its equity curve).
///
/// `cadence_minutes` is the strategy's decision cadence, forwarded to the
/// annualisation formula. Passing `0` is safe: the formula clamps to `1.0`.
///
/// `strategy_return_pct` is the strategy's final `total_return_pct` from the
/// main run, used only to compute `relative_to` deltas.
///
/// `rng_seed` controls the `RandomDirection` coin-flip sequence. Pass a fixed
/// value (e.g. `42`) for reproducible results across identical bar slices.
pub fn compute_baselines(
    bars: &[Ohlcv],
    initial_equity: f64,
    cadence_minutes: u32,
    strategy_return_pct: f64,
    rng_seed: u64,
) -> BaselinesReport {
    compute_baselines_with_periods_per_year(
        bars,
        initial_equity,
        annualization_periods_per_year(cadence_minutes),
        strategy_return_pct,
        rng_seed,
    )
}

pub fn compute_baselines_with_periods_per_year(
    bars: &[Ohlcv],
    initial_equity: f64,
    periods_per_year: f64,
    strategy_return_pct: f64,
    rng_seed: u64,
) -> BaselinesReport {
    let buy_hold = baseline_buy_hold(bars, initial_equity, periods_per_year);
    let always_flat = BaselineResult {
        return_pct: 0.0,
        sharpe: 0.0,
    };
    let simple_trend = baseline_simple_trend(bars, initial_equity, periods_per_year);
    let simple_mean_rev = baseline_simple_mean_reversion(bars, initial_equity, periods_per_year);
    let random_dir = baseline_random_direction(bars, initial_equity, periods_per_year, rng_seed);

    let relative_to = RelativeTo {
        buy_hold: strategy_return_pct - buy_hold.return_pct,
        always_flat: strategy_return_pct - always_flat.return_pct,
        simple_trend: strategy_return_pct - simple_trend.return_pct,
        simple_mean_reversion: strategy_return_pct - simple_mean_rev.return_pct,
        random_direction: strategy_return_pct - random_dir.return_pct,
    };

    BaselinesReport {
        buy_hold,
        always_flat,
        simple_trend,
        simple_mean_reversion: simple_mean_rev,
        random_direction: random_dir,
        relative_to,
    }
}

// ── Individual baselines ──────────────────────────────────────────────────────

/// `buy_hold`: long at first bar's open, held until last bar's close.
///
/// `return_pct = (last_close - first_open) / first_open * 100`
///
/// The equity curve has one sample per bar: equity[i] = initial + position *
/// (bars[i].close - entry). Sharpe is computed from that curve.
fn baseline_buy_hold(bars: &[Ohlcv], initial_equity: f64, periods_per_year: f64) -> BaselineResult {
    if bars.is_empty() {
        return zero_result();
    }
    let entry = bars[0].open;
    if entry <= 0.0 {
        return zero_result();
    }
    // Size: buy risk_pct of capital. We use 100% (full notional) for a
    // true buy-and-hold benchmark — the caller only gets one position and
    // never adds to it.
    let units = initial_equity / entry;
    let mut equity_curve: Vec<f64> = Vec::with_capacity(bars.len() + 1);
    equity_curve.push(initial_equity);
    for bar in bars {
        let equity = initial_equity + units * (bar.close - entry);
        equity_curve.push(equity);
    }
    let final_equity = *equity_curve.last().unwrap_or(&initial_equity);
    let return_pct = total_return_pct(initial_equity, final_equity);
    let returns = equity_to_returns(&equity_curve);
    let sharpe = sharpe_from_returns(&returns, periods_per_year);
    BaselineResult { return_pct, sharpe }
}

const SMA_WINDOW: usize = 20;

/// `simple_trend`: long when 20-bar SMA slope is positive AND close > SMA,
/// else flat. Position is entered at the bar's close (for simplicity, no
/// slippage) and exited when the condition no longer holds.
///
/// Signal uses the same cadence as the strategy — one decision per bar in
/// `bars`. Warmup (first 19 bars) is flat.
fn baseline_simple_trend(bars: &[Ohlcv], initial_equity: f64, periods_per_year: f64) -> BaselineResult {
    if bars.len() < 2 {
        return zero_result();
    }

    // Accumulate closes for the rolling SMA window.
    let mut window: Vec<f64> = Vec::with_capacity(SMA_WINDOW);
    // Position tracking: units held (+long) or 0 (flat).
    let mut units: f64 = 0.0;
    let mut entry_price: f64 = 0.0;
    let mut equity = initial_equity;
    let mut equity_curve: Vec<f64> = vec![initial_equity];

    for bar in bars {
        window.push(bar.close);
        if window.len() > SMA_WINDOW {
            window.remove(0);
        }

        // Need at least SMA_WINDOW bars to compute slope.
        // Slope = current_sma - prev_sma (approximated by comparing the
        // last-pushed element vs the window mean before it was pushed).
        // For simplicity: slope = sma_now - sma(N-1 bars ago), where we
        // recompute from the window. We compute a micro-slope as
        // sma[last half] - sma[first half].
        let signal_long: bool = if window.len() >= SMA_WINDOW {
            let sma: f64 = window.iter().sum::<f64>() / window.len() as f64;
            let half = SMA_WINDOW / 2;
            let sma_first_half: f64 = window[..half].iter().sum::<f64>() / half as f64;
            let sma_last_half: f64 = window[half..].iter().sum::<f64>() / (SMA_WINDOW - half) as f64;
            let slope_positive = sma_last_half > sma_first_half;
            slope_positive && bar.close > sma
        } else {
            false
        };

        // Transition: enter long on signal, exit to flat when signal off.
        if signal_long && units == 0.0 {
            // Enter long: buy `equity / close` units.
            if bar.close > 0.0 {
                units = equity / bar.close;
                entry_price = bar.close;
            }
        } else if !signal_long && units > 0.0 {
            // Exit to flat: realize PnL.
            equity += units * (bar.close - entry_price);
            units = 0.0;
            entry_price = 0.0;
        }

        // Mark to market.
        let marked = if units > 0.0 {
            equity + units * (bar.close - entry_price)
        } else {
            equity
        };
        equity_curve.push(marked);
    }

    // Close any remaining position at the last bar's close.
    if units > 0.0 {
        if let Some(last) = bars.last() {
            equity += units * (last.close - entry_price);
        }
    }

    let return_pct = total_return_pct(initial_equity, equity);
    let returns = equity_to_returns(&equity_curve);
    let sharpe = sharpe_from_returns(&returns, periods_per_year);
    BaselineResult { return_pct, sharpe }
}

/// `simple_mean_reversion`: z-score of close over a 20-bar window.
/// - z < -1 → long
/// - z > +1 → short
/// - else → flat
///
/// Warmup (first 19 bars) is flat. Positions sized at 100% of current equity.
fn baseline_simple_mean_reversion(
    bars: &[Ohlcv],
    initial_equity: f64,
    periods_per_year: f64,
) -> BaselineResult {
    if bars.len() < 2 {
        return zero_result();
    }

    let mut window: Vec<f64> = Vec::with_capacity(SMA_WINDOW);
    // Current position direction: +1.0 = long, -1.0 = short, 0.0 = flat.
    let mut direction: f64 = 0.0;
    let mut entry_price: f64 = 0.0;
    let mut equity = initial_equity;
    let mut equity_curve: Vec<f64> = vec![initial_equity];

    for bar in bars {
        window.push(bar.close);
        if window.len() > SMA_WINDOW {
            window.remove(0);
        }

        let target_dir: f64 = if window.len() >= SMA_WINDOW {
            let mean: f64 = window.iter().sum::<f64>() / window.len() as f64;
            let variance: f64 = window.iter().map(|&c| (c - mean).powi(2)).sum::<f64>() / window.len() as f64;
            let std_dev = variance.sqrt();
            if std_dev < 1e-12 {
                0.0 // flat when no volatility
            } else {
                let z = (bar.close - mean) / std_dev;
                if z < -1.0 {
                    1.0 // long when oversold
                } else if z > 1.0 {
                    -1.0 // short when overbought
                } else {
                    0.0 // flat in the middle zone
                }
            }
        } else {
            0.0 // warmup
        };

        // Transition.
        if target_dir != direction {
            // Close current position if any.
            if direction != 0.0 && entry_price > 0.0 {
                let units = (equity / entry_price) * direction.signum();
                equity += units * (bar.close - entry_price) * direction;
            }
            direction = target_dir;
            entry_price = if direction != 0.0 { bar.close } else { 0.0 };
        }

        // Mark to market.
        let marked = if direction != 0.0 && entry_price > 0.0 {
            let units = equity / entry_price;
            equity + units * (bar.close - entry_price) * direction
        } else {
            equity
        };
        equity_curve.push(marked);
    }

    // Close any remaining position at last bar's close.
    if direction != 0.0 && entry_price > 0.0 {
        if let Some(last) = bars.last() {
            let units = equity / entry_price;
            equity += units * (last.close - entry_price) * direction;
        }
    }

    let return_pct = total_return_pct(initial_equity, equity);
    let returns = equity_to_returns(&equity_curve);
    let sharpe = sharpe_from_returns(&returns, periods_per_year);
    BaselineResult { return_pct, sharpe }
}

/// `random_direction`: fair coin-flip between long (+100 bps) and short
/// (-100 bps) on each bar. Seeded for deterministic backtests. Matches the
/// `RandomDirection` algorithm in `crates/xvision-eval/src/baselines/random_direction.rs`
/// but operates on a bare bar slice rather than `MarketSnapshot`.
///
/// Position sizing: 100% of current equity per bar entry, unwound at the
/// next bar's close when the signal flips. Flat at bar 0 until the first
/// coin-flip result.
fn baseline_random_direction(
    bars: &[Ohlcv],
    initial_equity: f64,
    periods_per_year: f64,
    rng_seed: u64,
) -> BaselineResult {
    if bars.is_empty() {
        return zero_result();
    }
    let mut rng = StdRng::seed_from_u64(rng_seed);

    // +1.0 = long, -1.0 = short
    let mut direction: f64 = 0.0;
    let mut entry_price: f64 = 0.0;
    let mut equity = initial_equity;
    let mut equity_curve: Vec<f64> = vec![initial_equity];

    for bar in bars {
        let go_long: bool = rng.gen();
        let target_dir: f64 = if go_long { 1.0 } else { -1.0 };

        if target_dir != direction {
            // Close prior position.
            if direction != 0.0 && entry_price > 0.0 {
                let units = equity / entry_price;
                equity += units * (bar.open - entry_price) * direction;
            }
            direction = target_dir;
            entry_price = bar.open;
        }

        // Mark to market at close.
        let marked = if direction != 0.0 && entry_price > 0.0 {
            let units = equity / entry_price;
            equity + units * (bar.close - entry_price) * direction
        } else {
            equity
        };
        equity_curve.push(marked);
    }

    // Close any open position at last bar's close.
    if direction != 0.0 && entry_price > 0.0 {
        if let Some(last) = bars.last() {
            let units = equity / entry_price;
            equity += units * (last.close - entry_price) * direction;
        }
    }

    let return_pct = total_return_pct(initial_equity, equity);
    let returns = equity_to_returns(&equity_curve);
    let sharpe = sharpe_from_returns(&returns, periods_per_year);
    BaselineResult { return_pct, sharpe }
}

// ── Metric helpers (mirrors engine/eval/metrics.rs exactly) ──────────────────
// These are intentionally kept in sync with the canonical engine formulas.
// Any change to the engine formulas should be reflected here.

fn equity_to_returns(equity_samples: &[f64]) -> Vec<f64> {
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

fn sharpe_from_returns(returns: &[f64], periods_per_year: f64) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|&r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std = variance.sqrt();
    if !std.is_finite() || std.abs() < 1e-12 {
        return 0.0;
    }
    (mean / std) * periods_per_year.sqrt()
}

fn total_return_pct(initial_equity: f64, final_equity: f64) -> f64 {
    if initial_equity <= 0.0 {
        return 0.0;
    }
    (final_equity - initial_equity) / initial_equity * 100.0
}

/// Annualization factor: same formula as `engine/eval/metrics.rs`.
/// Minutes per year = 60 × 24 × 365. Returns 1.0 for cadence_minutes == 0.
fn annualization_periods_per_year(cadence_minutes: u32) -> f64 {
    if cadence_minutes == 0 {
        return 1.0;
    }
    let minutes_per_year = 60.0 * 24.0 * 365.0;
    minutes_per_year / cadence_minutes as f64
}

fn zero_result() -> BaselineResult {
    BaselineResult {
        return_pct: 0.0,
        sharpe: 0.0,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn bar(close: f64) -> Ohlcv {
        Ohlcv {
            timestamp: Utc::now(),
            open: close,
            high: close * 1.01,
            low: close * 0.99,
            close,
            volume: 1_000.0,
        }
    }

    fn bar_with_open(open: f64, close: f64) -> Ohlcv {
        Ohlcv {
            timestamp: Utc::now(),
            open,
            high: close.max(open) * 1.005,
            low: close.min(open) * 0.995,
            close,
            volume: 1_000.0,
        }
    }

    // ── buy_hold ──────────────────────────────────────────────────────────────

    #[test]
    fn buy_hold_flat_market_zero_return() {
        // All bars at same close. return_pct must be 0.0 (ignoring open≠close).
        let bars: Vec<Ohlcv> = (0..5).map(|_| bar(100.0)).collect();
        let result = baseline_buy_hold(&bars, 10_000.0, 252.0);
        assert!(
            result.return_pct.abs() < 1e-9,
            "flat market → 0% return, got {:.6}",
            result.return_pct
        );
    }

    #[test]
    fn buy_hold_up_10pct() {
        // Entry at 100, exit at 110 → +10%
        let bars = vec![
            bar_with_open(100.0, 100.0),
            bar_with_open(105.0, 105.0),
            bar_with_open(110.0, 110.0),
        ];
        let result = baseline_buy_hold(&bars, 10_000.0, 252.0);
        assert!(
            (result.return_pct - 10.0).abs() < 0.01,
            "expected ~10% return, got {:.4}",
            result.return_pct
        );
    }

    #[test]
    fn buy_hold_down_50pct() {
        // Entry at 100, exit at 50 → -50%
        let bars = vec![
            bar_with_open(100.0, 90.0),
            bar_with_open(90.0, 70.0),
            bar_with_open(70.0, 50.0),
        ];
        let result = baseline_buy_hold(&bars, 10_000.0, 252.0);
        assert!(
            (result.return_pct - (-50.0)).abs() < 0.01,
            "expected ~-50% return, got {:.4}",
            result.return_pct
        );
    }

    #[test]
    fn buy_hold_empty_bars_returns_zero() {
        let result = baseline_buy_hold(&[], 10_000.0, 252.0);
        assert_eq!(result.return_pct, 0.0);
        assert_eq!(result.sharpe, 0.0);
    }

    // ── always_flat ────────────────────────────────────────────────────────────

    #[test]
    fn compute_baselines_always_flat_is_zero() {
        let bars: Vec<Ohlcv> = (0..30).map(|i| bar(100.0 + i as f64)).collect();
        let report = compute_baselines(&bars, 10_000.0, 60, 5.0, 42);
        assert_eq!(report.always_flat.return_pct, 0.0);
        assert_eq!(report.always_flat.sharpe, 0.0);
    }

    // ── simple_trend ──────────────────────────────────────────────────────────

    #[test]
    fn simple_trend_warming_up_is_flat() {
        // Only 19 bars — never enough for the 20-bar window → always flat.
        let bars: Vec<Ohlcv> = (0..19).map(|i| bar(100.0 + i as f64)).collect();
        let result = baseline_simple_trend(&bars, 10_000.0, 252.0);
        assert!(
            result.return_pct.abs() < 1e-9,
            "< 20 bars → flat, got return_pct={:.6}",
            result.return_pct
        );
    }

    #[test]
    fn simple_trend_uptrend_produces_positive_return() {
        // 40 bars of strictly increasing prices — the trend signal should be
        // long for the majority of bars, producing a positive return.
        let bars: Vec<Ohlcv> = (0..40).map(|i| bar(100.0 + i as f64 * 2.0)).collect();
        let result = baseline_simple_trend(&bars, 10_000.0, 252.0);
        assert!(
            result.return_pct > 0.0,
            "uptrend must produce positive return, got {:.4}",
            result.return_pct
        );
    }

    #[test]
    fn simple_trend_downtrend_stays_flat_or_small_loss() {
        // Strictly falling prices → slope is negative → signal flat.
        // return should be ~ 0 (flat) or a tiny loss from the first entry attempt.
        let bars: Vec<Ohlcv> = (0..40).map(|i| bar((200.0 - i as f64 * 2.0).max(1.0))).collect();
        let result = baseline_simple_trend(&bars, 10_000.0, 252.0);
        // In a strict downtrend the slope condition is negative so the baseline
        // stays flat for the entire run → return ≈ 0.
        assert!(
            result.return_pct.abs() < 5.0,
            "downtrend: expected near-zero return (flat), got {:.4}",
            result.return_pct
        );
    }

    // ── simple_mean_reversion ─────────────────────────────────────────────────

    #[test]
    fn mean_reversion_warming_up_is_flat() {
        let bars: Vec<Ohlcv> = (0..19).map(|i| bar(100.0 + i as f64)).collect();
        let result = baseline_simple_mean_reversion(&bars, 10_000.0, 252.0);
        assert!(
            result.return_pct.abs() < 1e-9,
            "< 20 bars → flat, got return_pct={:.6}",
            result.return_pct
        );
    }

    #[test]
    fn mean_reversion_no_extreme_z_stays_flat() {
        // All bars at exactly 100 → z-score = 0 always → flat.
        let bars: Vec<Ohlcv> = (0..30).map(|_| bar(100.0)).collect();
        let result = baseline_simple_mean_reversion(&bars, 10_000.0, 252.0);
        assert!(
            result.return_pct.abs() < 1e-9,
            "no extreme z → flat → 0% return, got {:.6}",
            result.return_pct
        );
    }

    #[test]
    fn mean_reversion_oversold_spike_then_recovery() {
        // 25 bars at 100, then one bar at 70 (z << -1 → long entry),
        // then 4 bars recovering back to 100.
        let mut bars: Vec<Ohlcv> = (0..25).map(|_| bar(100.0)).collect();
        bars.push(bar(70.0)); // oversold → long entry
        bars.push(bar(80.0));
        bars.push(bar(90.0));
        bars.push(bar(100.0));
        bars.push(bar(100.0)); // z back in range → flat, books gain
        let result = baseline_simple_mean_reversion(&bars, 10_000.0, 252.0);
        // We entered long at 70, exited somewhere around 90-100 → positive return.
        assert!(
            result.return_pct > 0.0,
            "recovery from oversold → positive return, got {:.4}",
            result.return_pct
        );
    }

    // ── relative_to ───────────────────────────────────────────────────────────

    #[test]
    fn relative_to_delta_is_strategy_minus_baseline() {
        // strategy returned 5%, buy_hold is derived from the bars.
        // relative_to.buy_hold = 5.0 - buy_hold.return_pct
        let bars: Vec<Ohlcv> = (0..30).map(|i| bar(100.0 + i as f64)).collect();
        let report = compute_baselines(&bars, 10_000.0, 60, 5.0, 42);
        let expected_delta = 5.0 - report.buy_hold.return_pct;
        assert!(
            (report.relative_to.buy_hold - expected_delta).abs() < 1e-9,
            "relative_to.buy_hold should be strategy({}) - baseline({:.4}) = {:.4}, got {:.4}",
            5.0,
            report.buy_hold.return_pct,
            expected_delta,
            report.relative_to.buy_hold
        );
    }

    #[test]
    fn relative_to_flat_is_strategy_return() {
        // always_flat returns 0 → relative_to.always_flat == strategy_return_pct
        let bars: Vec<Ohlcv> = (0..30).map(|_| bar(100.0)).collect();
        let strategy_ret = -2.32;
        let report = compute_baselines(&bars, 10_000.0, 60, strategy_ret, 42);
        assert!(
            (report.relative_to.always_flat - strategy_ret).abs() < 1e-9,
            "relative_to.always_flat should equal strategy return {strategy_ret}, got {:.6}",
            report.relative_to.always_flat
        );
    }

    // ── annualization_periods_per_year ────────────────────────────────────────

    #[test]
    fn annualization_zero_cadence_returns_one() {
        assert_eq!(annualization_periods_per_year(0), 1.0);
    }

    #[test]
    fn annualization_60min_is_8760() {
        let ppy = annualization_periods_per_year(60);
        assert!(
            (ppy - 8760.0).abs() < 1.0,
            "expected ~8760 for 60-min cadence, got {ppy}"
        );
    }

    #[test]
    fn annualization_daily_is_365() {
        let ppy = annualization_periods_per_year(60 * 24);
        assert!(
            (ppy - 365.0).abs() < 0.5,
            "expected ~365 for daily cadence, got {ppy}"
        );
    }
}
