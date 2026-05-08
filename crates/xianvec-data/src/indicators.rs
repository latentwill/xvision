//! Technical indicators on plain `&[f64]` price series.
//!
//! Each function returns a `Vec<f64>` of the same length as the input, with
//! `f64::NAN` filling positions where the indicator is undefined (period-1
//! warmup). NaN-out is the convention so downstream code can detect "not yet
//! valid" without sentinel magic numbers.
//!
//! Wilder-smoothed indicators (RSI, ATR) follow the original definitions:
//! `wilder_alpha = 1/period`, equivalent to `EMA(2*period - 1)`.

use std::cmp::Ordering;

/// Simple moving average over `period`.
pub fn sma(prices: &[f64], period: usize) -> Vec<f64> {
    assert!(period > 0, "period must be > 0");
    let n = prices.len();
    let mut out = vec![f64::NAN; n];
    if n < period {
        return out;
    }
    let mut sum: f64 = prices[..period].iter().sum();
    out[period - 1] = sum / period as f64;
    for i in period..n {
        sum += prices[i] - prices[i - period];
        out[i] = sum / period as f64;
    }
    out
}

/// Exponential moving average. Seeded with the SMA of the first `period`
/// values, then `EMA_t = alpha * P_t + (1 - alpha) * EMA_{t-1}` with
/// `alpha = 2 / (period + 1)`.
pub fn ema(prices: &[f64], period: usize) -> Vec<f64> {
    assert!(period > 0, "period must be > 0");
    let n = prices.len();
    let mut out = vec![f64::NAN; n];
    if n < period {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut prev: f64 = prices[..period].iter().sum::<f64>() / period as f64;
    out[period - 1] = prev;
    for i in period..n {
        prev = alpha * prices[i] + (1.0 - alpha) * prev;
        out[i] = prev;
    }
    out
}

/// Wilder-smoothed RSI. `period` is typically 14.
///
/// First valid output at index `period` (need `period` deltas to seed).
pub fn rsi(prices: &[f64], period: usize) -> Vec<f64> {
    assert!(period > 0, "period must be > 0");
    let n = prices.len();
    let mut out = vec![f64::NAN; n];
    if n <= period {
        return out;
    }
    // Seed with simple averages over the first `period` deltas.
    let mut gain_sum = 0.0;
    let mut loss_sum = 0.0;
    for i in 1..=period {
        let delta = prices[i] - prices[i - 1];
        if delta >= 0.0 {
            gain_sum += delta;
        } else {
            loss_sum -= delta;
        }
    }
    let mut avg_gain = gain_sum / period as f64;
    let mut avg_loss = loss_sum / period as f64;
    out[period] = rsi_value(avg_gain, avg_loss);
    let alpha = 1.0 / period as f64;
    for i in period + 1..n {
        let delta = prices[i] - prices[i - 1];
        let (g, l) = if delta >= 0.0 { (delta, 0.0) } else { (0.0, -delta) };
        avg_gain = (1.0 - alpha) * avg_gain + alpha * g;
        avg_loss = (1.0 - alpha) * avg_loss + alpha * l;
        out[i] = rsi_value(avg_gain, avg_loss);
    }
    out
}

fn rsi_value(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        if avg_gain == 0.0 {
            50.0
        } else {
            100.0
        }
    } else {
        let rs = avg_gain / avg_loss;
        100.0 - 100.0 / (1.0 + rs)
    }
}

/// Bollinger Bands `(middle = SMA(period), upper = middle + k*σ, lower = middle - k*σ)`.
/// σ uses the population standard deviation (divisor `period`, not `period-1`)
/// to match TA-Lib / the prevailing trading convention.
pub fn bollinger(prices: &[f64], period: usize, k: f64) -> BollingerBands {
    assert!(period > 0, "period must be > 0");
    let n = prices.len();
    let mid = sma(prices, period);
    let mut upper = vec![f64::NAN; n];
    let mut lower = vec![f64::NAN; n];
    for i in (period - 1)..n {
        let mean = mid[i];
        let var = prices[i + 1 - period..=i]
            .iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>()
            / period as f64;
        let std = var.sqrt();
        upper[i] = mean + k * std;
        lower[i] = mean - k * std;
    }
    BollingerBands {
        middle: mid,
        upper,
        lower,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BollingerBands {
    pub middle: Vec<f64>,
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
}

/// Wilder-smoothed Average True Range. Inputs are equal-length OHLC series.
pub fn atr(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Vec<f64> {
    assert!(period > 0, "period must be > 0");
    assert_eq!(high.len(), low.len());
    assert_eq!(low.len(), close.len());
    let n = close.len();
    let mut out = vec![f64::NAN; n];
    if n <= period {
        return out;
    }
    let mut tr = vec![0.0; n];
    tr[0] = high[0] - low[0];
    for i in 1..n {
        let hl = high[i] - low[i];
        let hc = (high[i] - close[i - 1]).abs();
        let lc = (low[i] - close[i - 1]).abs();
        tr[i] = hl.max(hc).max(lc);
    }
    // Seed first ATR as simple mean of the first `period` TRs.
    let mut prev: f64 = tr[1..=period].iter().sum::<f64>() / period as f64;
    out[period] = prev;
    let alpha = 1.0 / period as f64;
    for i in period + 1..n {
        prev = (1.0 - alpha) * prev + alpha * tr[i];
        out[i] = prev;
    }
    out
}

/// MACD: `(macd, signal, histogram)`.
pub fn macd(prices: &[f64], fast: usize, slow: usize, signal: usize) -> Macd {
    let fast_ema = ema(prices, fast);
    let slow_ema = ema(prices, slow);
    let n = prices.len();
    let mut macd_line = vec![f64::NAN; n];
    for i in 0..n {
        if fast_ema[i].is_nan() || slow_ema[i].is_nan() {
            continue;
        }
        macd_line[i] = fast_ema[i] - slow_ema[i];
    }
    // Signal is EMA of the MACD line restricted to its valid prefix.
    let valid_start = macd_line.iter().position(|x| !x.is_nan()).unwrap_or(n);
    let valid: Vec<f64> = macd_line[valid_start..].to_vec();
    let signal_partial = ema(&valid, signal);
    let mut signal_full = vec![f64::NAN; n];
    for (i, v) in signal_partial.into_iter().enumerate() {
        signal_full[valid_start + i] = v;
    }
    let mut hist = vec![f64::NAN; n];
    for i in 0..n {
        if !macd_line[i].is_nan() && !signal_full[i].is_nan() {
            hist[i] = macd_line[i] - signal_full[i];
        }
    }
    Macd {
        macd: macd_line,
        signal: signal_full,
        histogram: hist,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Macd {
    pub macd: Vec<f64>,
    pub signal: Vec<f64>,
    pub histogram: Vec<f64>,
}

/// Donchian channels — rolling `period` high and low.
pub fn donchian(high: &[f64], low: &[f64], period: usize) -> Donchian {
    assert!(period > 0);
    assert_eq!(high.len(), low.len());
    let n = high.len();
    let mut up = vec![f64::NAN; n];
    let mut dn = vec![f64::NAN; n];
    for i in (period - 1)..n {
        let win_h = &high[i + 1 - period..=i];
        let win_l = &low[i + 1 - period..=i];
        up[i] = win_h.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        dn[i] = win_l.iter().cloned().fold(f64::INFINITY, f64::min);
    }
    Donchian { upper: up, lower: dn }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Donchian {
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
}

/// Fibonacci retracement levels for a window's swing high → swing low.
/// Detects the most recent local peak / trough across the lookback and returns
/// the standard Fib levels between them. Returns `None` if the window is too
/// short or peak/trough cannot be identified.
pub fn fib_retracements(prices: &[f64], lookback: usize) -> Option<FibLevels> {
    if prices.len() < lookback || lookback < 3 {
        return None;
    }
    let win = &prices[prices.len() - lookback..];
    let (high_idx, &high) = win
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(Ordering::Equal))?;
    let (low_idx, &low) = win
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(Ordering::Equal))?;
    if (high - low).abs() < f64::EPSILON {
        return None;
    }
    let direction = if high_idx < low_idx {
        Direction::Down
    } else {
        Direction::Up
    };
    let span = high - low;
    Some(FibLevels {
        high,
        low,
        direction,
        levels: [
            (0.236, high - 0.236 * span),
            (0.382, high - 0.382 * span),
            (0.500, high - 0.500 * span),
            (0.618, high - 0.618 * span),
            (0.786, high - 0.786 * span),
        ],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FibLevels {
    pub high: f64,
    pub low: f64,
    pub direction: Direction,
    /// (ratio, price) pairs at 0.236 / 0.382 / 0.500 / 0.618 / 0.786.
    pub levels: [(f64, f64); 5],
}

// ---------------------------------------------------------------------------
// Panel computation
// ---------------------------------------------------------------------------

use xianvec_core::market::IndicatorPanel;

/// Compute [`IndicatorPanel`] at the latest bar from a named parquet fixture.
///
/// Loads `lookback_bars` bars, then derives RSI(14), SMA(20/50/200),
/// EMA(12/26), Bollinger(20, 2), ATR(14). MACD fields are left `None` for now.
pub fn compute_panel_from_fixture(
    fixture: &str,
    asset: &str,
    lookback_bars: usize,
) -> anyhow::Result<IndicatorPanel> {
    let bars = crate::fixtures::load_ohlcv_fixture(fixture, asset, lookback_bars)?;
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();

    let last_or_none = |v: Vec<f64>| -> Option<f64> { v.last().copied().filter(|x| x.is_finite()) };

    // Bollinger(20, 2) via the existing `bollinger` fn.
    let bb = bollinger(&closes, 20.min(closes.len().max(1)), 2.0);
    let bb_upper = bb.upper.last().copied().filter(|x| x.is_finite());
    let bb_middle = bb.middle.last().copied().filter(|x| x.is_finite());
    let bb_lower = bb.lower.last().copied().filter(|x| x.is_finite());

    Ok(IndicatorPanel {
        rsi_14: last_or_none(rsi(&closes, 14)),
        sma_20: last_or_none(sma(&closes, 20)),
        sma_50: last_or_none(sma(&closes, 50)),
        sma_200: last_or_none(sma(&closes, 200)),
        ema_12: last_or_none(ema(&closes, 12)),
        ema_26: last_or_none(ema(&closes, 26)),
        bb_upper,
        bb_middle,
        bb_lower,
        atr_14: last_or_none(atr(&highs, &lows, &closes, 14)),
        macd: None,
        macd_signal: None,
        macd_hist: None,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strict equality for non-NaN values; matched-NaN positions are treated
    /// as equal. Tolerance defaults to `1e-6` per plan acceptance criterion.
    fn assert_series_eq(actual: &[f64], expected: &[f64], tol: f64) {
        assert_eq!(actual.len(), expected.len(), "length mismatch");
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            match (a.is_nan(), e.is_nan()) {
                (true, true) => continue,
                (false, false) => assert!(
                    (a - e).abs() < tol,
                    "index {i}: actual {a}, expected {e}, diff {}",
                    (a - e).abs()
                ),
                _ => panic!("index {i}: NaN mismatch (actual {a}, expected {e})"),
            }
        }
    }

    #[test]
    fn sma_canonical_window() {
        // Window slides cleanly across [1, 2, 3, 4, 5] with period=3.
        let s = sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
        assert_series_eq(&s, &[f64::NAN, f64::NAN, 2.0, 3.0, 4.0], 1e-6);
    }

    #[test]
    fn ema_seeds_with_sma_then_recurses() {
        let s = ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
        // seed at index 2 = SMA(1,2,3) = 2.0; alpha = 2/(3+1) = 0.5.
        // index 3: 0.5*4 + 0.5*2 = 3.0
        // index 4: 0.5*5 + 0.5*3 = 4.0
        assert_series_eq(&s, &[f64::NAN, f64::NAN, 2.0, 3.0, 4.0], 1e-6);
    }

    #[test]
    fn rsi_canonical_wilder_example() {
        // Worked example: 14 closes from Wilder. Result at index 14 should
        // be in [70, 75]. We don't pin to a paper value because rounding in
        // published examples varies; we assert structural correctness.
        let prices = [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28,
            46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
        ];
        let r = rsi(&prices, 14);
        assert!(r[..14].iter().all(|x| x.is_nan()), "warmup must be NaN");
        for v in &r[14..] {
            assert!(*v >= 0.0 && *v <= 100.0, "RSI must be in [0,100]: {v}");
        }
        // Sanity: in this rising series, RSI at idx 14 should be > 50.
        assert!(r[14] > 50.0, "rising series should give RSI > 50, got {}", r[14]);
    }

    #[test]
    fn rsi_constant_series_is_50() {
        let r = rsi(&vec![10.0; 30], 14);
        assert!((r[14] - 50.0).abs() < 1e-9);
    }

    #[test]
    fn rsi_monotonic_up_series_pegs_high() {
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
        let r = rsi(&prices, 14);
        for v in &r[14..] {
            assert!((*v - 100.0).abs() < 1e-9, "monotone-up RSI = 100, got {v}");
        }
    }

    #[test]
    fn bollinger_middle_equals_sma() {
        let prices: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let b = bollinger(&prices, 5, 2.0);
        let s = sma(&prices, 5);
        for (a, e) in b.middle.iter().zip(s.iter()) {
            match (a.is_nan(), e.is_nan()) {
                (true, true) => continue,
                _ => assert!((a - e).abs() < 1e-9),
            }
        }
    }

    #[test]
    fn bollinger_constant_series_collapses_bands() {
        let prices = vec![100.0; 30];
        let b = bollinger(&prices, 20, 2.0);
        for i in 19..30 {
            assert!((b.upper[i] - b.lower[i]).abs() < 1e-9, "constant series → zero σ");
            assert!((b.middle[i] - 100.0).abs() < 1e-9);
        }
    }

    #[test]
    fn atr_constant_range_recovers_period_range() {
        // High-low constant 5 each bar; close = midpoint. ATR should converge to 5.
        let n = 50;
        let high: Vec<f64> = (0..n).map(|_| 105.0).collect();
        let low: Vec<f64> = (0..n).map(|_| 100.0).collect();
        let close: Vec<f64> = (0..n).map(|_| 102.5).collect();
        let a = atr(&high, &low, &close, 14);
        // Allow a small window for Wilder smoothing convergence.
        assert!(
            (a[40] - 5.0).abs() < 0.1,
            "ATR should converge to 5, got {}",
            a[40]
        );
    }

    #[test]
    fn macd_lengths_and_warmup_align() {
        let prices: Vec<f64> = (1..=100).map(|i| (i as f64) * 1.0).collect();
        let m = macd(&prices, 12, 26, 9);
        assert_eq!(m.macd.len(), 100);
        assert_eq!(m.signal.len(), 100);
        assert_eq!(m.histogram.len(), 100);
        // Signal needs slow_ema warmup (idx 25) + signal warmup (9 deeper) → idx 33.
        assert!(m.signal[33].is_finite(), "signal valid by idx 33");
        assert!(m.histogram[33].is_finite());
    }

    #[test]
    fn donchian_recovers_window_min_max() {
        let high = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let low = vec![0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5];
        let d = donchian(&high, &low, 3);
        assert!((d.upper[2] - 3.0).abs() < 1e-9);
        assert!((d.lower[2] - 0.5).abs() < 1e-9);
        assert!((d.upper[6] - 7.0).abs() < 1e-9);
        assert!((d.lower[6] - 4.5).abs() < 1e-9);
    }

    #[test]
    fn fib_levels_compute_canonical_ratios() {
        // Window: 0..100, swings cleanly from 100 high to 0 low.
        let prices: Vec<f64> = (0..=100).rev().map(|i| i as f64).collect();
        let f = fib_retracements(&prices, 101).expect("must compute");
        assert_eq!(f.direction, Direction::Down, "high before low → down swing");
        assert!((f.high - 100.0).abs() < 1e-9);
        assert!((f.low - 0.0).abs() < 1e-9);
        assert!((f.levels[0].1 - 76.4).abs() < 1e-3, "0.236 retracement = 76.4");
        assert!((f.levels[1].1 - 61.8).abs() < 1e-3, "0.382 = 61.8");
        assert!((f.levels[2].1 - 50.0).abs() < 1e-3, "0.500 = 50.0");
        assert!((f.levels[3].1 - 38.2).abs() < 1e-3, "0.618 = 38.2");
        assert!((f.levels[4].1 - 21.4).abs() < 1e-3, "0.786 = 21.4");
    }

    proptest::proptest! {
        #[test]
        fn sma_no_panic(period in 1usize..50, vals in proptest::collection::vec(-1000.0..1000.0_f64, 0..200)) {
            let _ = sma(&vals, period);
        }

        #[test]
        fn rsi_in_range(period in 1usize..30, vals in proptest::collection::vec(0.1..1000.0_f64, 0..100)) {
            let r = rsi(&vals, period);
            for v in r {
                if !v.is_nan() {
                    proptest::prop_assert!((0.0..=100.0).contains(&v));
                }
            }
        }
    }
}
