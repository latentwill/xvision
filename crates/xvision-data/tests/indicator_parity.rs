//! Indicator parity tests — lock each public indicator to hand-computed or
//! reference-implementation values so the chart payload (TV-1 M1) and MCP tool
//! layer stay byte-identical for the same input.
//!
//! Conventions documented per indicator so future readers know exactly what is
//! being asserted.

use xvision_data::indicators::{atr, bollinger, donchian, ema, fib_retracements, macd, rsi, sma, Direction};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert two series are equal within `tol`, treating (NaN, NaN) at the same
/// index as equal.
fn assert_series_eq(label: &str, actual: &[f64], expected: &[f64], tol: f64) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: length mismatch (actual={}, expected={})",
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        match (a.is_nan(), e.is_nan()) {
            (true, true) => continue,
            (false, false) => assert!(
                (a - e).abs() < tol,
                "{label}[{i}]: actual={a:.10}, expected={e:.10}, diff={:.2e}",
                (a - e).abs()
            ),
            _ => panic!("{label}[{i}]: NaN mismatch (actual={a}, expected={e})"),
        }
    }
}

// ---------------------------------------------------------------------------
// SMA — simple moving average
//
// Convention: output[i] = mean of prices[i-period+1..=i]. NaN for i < period-1.
// No smoothing; exact floating-point arithmetic on small integers → tolerance 1e-12.
// ---------------------------------------------------------------------------

#[test]
fn sma_period3_5bars() {
    // prices = [10, 20, 30, 40, 50], period = 3
    // out[0] = NaN, out[1] = NaN
    // out[2] = (10+20+30)/3 = 20
    // out[3] = (20+30+40)/3 = 30
    // out[4] = (30+40+50)/3 = 40
    let prices = [10.0, 20.0, 30.0, 40.0, 50.0];
    let actual = sma(&prices, 3);
    let expected = [f64::NAN, f64::NAN, 20.0, 30.0, 40.0];
    assert_series_eq("sma_period3_5bars", &actual, &expected, 1e-12);
}

#[test]
fn sma_period1_is_identity() {
    // period=1 → out[i] = prices[i] for all i
    let prices = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0];
    let actual = sma(&prices, 1);
    assert_series_eq("sma_period1", &actual, &prices, 1e-12);
}

#[test]
fn sma_period_equals_length() {
    // period = n → only out[n-1] is valid; equals the mean of all prices
    let prices = [2.0, 4.0, 6.0, 8.0];
    let actual = sma(&prices, 4);
    let expected = [f64::NAN, f64::NAN, f64::NAN, 5.0];
    assert_series_eq("sma_period=len", &actual, &expected, 1e-12);
}

#[test]
fn sma_longer_series_hand_computed() {
    // 10 bars, period=4
    // prices: 1 2 3 4 5 6 7 8 9 10
    // out[3] = (1+2+3+4)/4 = 2.5
    // out[4] = (2+3+4+5)/4 = 3.5
    // out[5] = (3+4+5+6)/4 = 4.5
    // out[6] = 5.5, out[7] = 6.5, out[8] = 7.5, out[9] = 8.5
    let prices: Vec<f64> = (1..=10).map(|i| i as f64).collect();
    let actual = sma(&prices, 4);
    let expected = [f64::NAN, f64::NAN, f64::NAN, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5];
    assert_series_eq("sma_longer", &actual, &expected, 1e-12);
}

// ---------------------------------------------------------------------------
// EMA — exponential moving average
//
// Convention: alpha = 2/(period+1). Seed = SMA of first `period` bars.
// out[period-1] = seed; out[i] = alpha*price[i] + (1-alpha)*out[i-1] for i>=period.
// NaN for i < period-1. Tolerance 1e-12 (no recursion accumulation for tiny series).
// ---------------------------------------------------------------------------

#[test]
fn ema_period2_hand_computed() {
    // prices = [1, 2, 3, 4, 5], period = 2
    // alpha = 2/(2+1) = 2/3
    // seed at [1] = (1+2)/2 = 1.5
    // [2]: (2/3)*3 + (1/3)*1.5 = 2.0 + 0.5 = 2.5
    // [3]: (2/3)*4 + (1/3)*2.5 = 8/3 + 5/6 = 16/6 + 5/6 = 21/6 = 3.5
    // [4]: (2/3)*5 + (1/3)*3.5 = 10/3 + 7/6 = 20/6 + 7/6 = 27/6 = 4.5
    let prices = [1.0, 2.0, 3.0, 4.0, 5.0];
    let actual = ema(&prices, 2);
    let expected = [f64::NAN, 1.5, 2.5, 3.5, 4.5];
    assert_series_eq("ema_period2", &actual, &expected, 1e-12);
}

#[test]
fn ema_period3_hand_computed() {
    // prices = [1, 2, 3, 4, 5, 6], period = 3
    // alpha = 2/(3+1) = 0.5
    // seed at [2] = (1+2+3)/3 = 2.0
    // [3]: 0.5*4 + 0.5*2.0 = 2.0 + 1.0 = 3.0
    // [4]: 0.5*5 + 0.5*3.0 = 2.5 + 1.5 = 4.0
    // [5]: 0.5*6 + 0.5*4.0 = 3.0 + 2.0 = 5.0
    let prices = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let actual = ema(&prices, 3);
    let expected = [f64::NAN, f64::NAN, 2.0, 3.0, 4.0, 5.0];
    assert_series_eq("ema_period3", &actual, &expected, 1e-12);
}

#[test]
fn ema_period4_hand_computed() {
    // prices = [10, 20, 30, 40, 50, 60], period = 4
    // alpha = 2/(4+1) = 0.4
    // seed at [3] = (10+20+30+40)/4 = 25.0
    // [4]: 0.4*50 + 0.6*25 = 20 + 15 = 35.0
    // [5]: 0.4*60 + 0.6*35 = 24 + 21 = 45.0
    let prices = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
    let actual = ema(&prices, 4);
    let expected = [f64::NAN, f64::NAN, f64::NAN, 25.0, 35.0, 45.0];
    assert_series_eq("ema_period4", &actual, &expected, 1e-12);
}

#[test]
fn ema_constant_series_equals_constant() {
    // For a constant series all prices = C, EMA should converge to C immediately.
    // seed = C, each subsequent step: alpha*C + (1-alpha)*C = C.
    let prices = vec![42.0_f64; 20];
    let actual = ema(&prices, 5);
    for (i, &v) in actual.iter().enumerate() {
        if i < 4 {
            assert!(v.is_nan(), "ema_constant[{i}] should be NaN");
        } else {
            assert!(
                (v - 42.0).abs() < 1e-12,
                "ema_constant[{i}]: expected 42.0, got {v}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// RSI — Wilder-smoothed Relative Strength Index
//
// Convention (from impl):
//   - Seed: simple average of gains/losses over the first `period` deltas
//     (i.e., prices[1..=period]).
//   - Smoothing: Wilder EMA, alpha = 1/period.
//   - avg_gain_t = (1 - 1/period) * avg_gain_{t-1} + (1/period) * gain_t
//   - avg_loss_t = (1 - 1/period) * avg_loss_{t-1} + (1/period) * loss_t
//   - First valid output at index `period` (uses period deltas to seed).
//   - NaN for indices 0..period.
//   - When avg_loss == 0 and avg_gain == 0: RSI = 50.
//   - When avg_loss == 0 and avg_gain > 0:  RSI = 100.
//
// Tolerance 1e-4 (Wilder smoothing accumulates rounding across 14+ steps).
// ---------------------------------------------------------------------------

#[test]
fn rsi_period2_hand_computed() {
    // Small period=2 so we can hand-compute exactly.
    // prices = [10, 12, 11, 14, 13, 16]
    // deltas: +2, -1, +3, -1, +3
    //
    // Seed (first 2 deltas: +2, -1):
    //   avg_gain = 2/2 = 1.0
    //   avg_loss = 1/2 = 0.5
    //   RSI[2] = 100 - 100/(1 + 1.0/0.5) = 100 - 100/3 ≈ 66.6667
    //
    // alpha = 1/2 = 0.5
    //
    // i=3 (delta=+3, gain=3, loss=0):
    //   avg_gain = 0.5*1.0 + 0.5*3 = 0.5 + 1.5 = 2.0
    //   avg_loss = 0.5*0.5 + 0.5*0 = 0.25
    //   RSI[3] = 100 - 100/(1 + 2.0/0.25) = 100 - 100/9 ≈ 88.8889
    //
    // i=4 (delta=-1, gain=0, loss=1):
    //   avg_gain = 0.5*2.0 + 0.5*0 = 1.0
    //   avg_loss = 0.5*0.25 + 0.5*1 = 0.125 + 0.5 = 0.625
    //   RSI[4] = 100 - 100/(1 + 1.0/0.625) = 100 - 100/(1 + 1.6) = 100 - 100/2.6 ≈ 61.5385
    //
    // i=5 (delta=+3, gain=3, loss=0):
    //   avg_gain = 0.5*1.0 + 0.5*3 = 2.0
    //   avg_loss = 0.5*0.625 + 0.5*0 = 0.3125
    //   RSI[5] = 100 - 100/(1 + 2.0/0.3125) = 100 - 100/(1 + 6.4) = 100 - 100/7.4 ≈ 86.4865
    let prices = [10.0_f64, 12.0, 11.0, 14.0, 13.0, 16.0];
    let actual = rsi(&prices, 2);
    // NaN warmup: indices 0 and 1
    assert!(actual[0].is_nan(), "rsi[0] must be NaN");
    assert!(actual[1].is_nan(), "rsi[1] must be NaN");
    let expected_valid = [
        100.0 - 100.0 / (1.0 + 1.0 / 0.5),    // [2] ≈ 66.6667
        100.0 - 100.0 / (1.0 + 2.0 / 0.25),   // [3] ≈ 88.8889
        100.0 - 100.0 / (1.0 + 1.0 / 0.625),  // [4] ≈ 61.5385
        100.0 - 100.0 / (1.0 + 2.0 / 0.3125), // [5] ≈ 86.4865
    ];
    for (offset, &exp) in expected_valid.iter().enumerate() {
        let i = 2 + offset;
        assert!(
            (actual[i] - exp).abs() < 1e-4,
            "rsi_period2[{i}]: actual={:.6}, expected={:.6}",
            actual[i],
            exp
        );
    }
}

#[test]
fn rsi_period14_wilder_reference() {
    // 20-bar series from Wilder's original 1978 examples (values rounded to 2dp
    // in the original text; we use them to verify structural correctness).
    // Convention: Wilder seeds with simple avg of first 14 deltas; then applies
    // alpha=1/14 smoothing. First valid output at index 14.
    let prices = [
        44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28,
        46.28, 46.00, 46.03, 46.41, 46.22, 45.64,
    ];
    let actual = rsi(&prices, 14);

    // Indices 0..14 must be NaN.
    for i in 0..14 {
        assert!(
            actual[i].is_nan(),
            "rsi_period14[{i}] must be NaN (warmup), got {}",
            actual[i]
        );
    }

    // Hand-compute the seed (first 14 deltas = prices[1..=14]):
    // Δs: -0.25, +0.06, -0.54, +0.72, +0.50, +0.27, +0.32, +0.42, +0.24,
    //     -0.19, +0.14, -0.42, +0.67, +0.00
    // gains: 0.06+0.72+0.50+0.27+0.32+0.42+0.24+0.14+0.67+0.00 = 3.34
    // losses: 0.25+0.54+0.19+0.42 = 1.40
    // avg_gain seed = 3.34/14 ≈ 0.23857
    // avg_loss seed = 1.40/14 = 0.10000
    // RSI[14] = 100 - 100/(1 + 0.23857/0.10000) ≈ 100 - 100/3.3857 ≈ 70.46
    // We allow ±0.5 for paper-rounding variance.
    let rsi14 = actual[14];
    assert!(
        (rsi14 - 70.46).abs() < 0.5,
        "rsi_period14[14]: expected ≈70.46, got {rsi14:.4}"
    );

    // All valid values must be in [0, 100].
    for (i, &v) in actual.iter().enumerate() {
        if !v.is_nan() {
            assert!(v >= 0.0 && v <= 100.0, "rsi_period14[{i}] out of range: {v}");
        }
    }
}

#[test]
fn rsi_constant_series_is_50() {
    // All deltas = 0; both avg_gain and avg_loss seed to 0 → RSI = 50 per impl.
    let prices = vec![100.0_f64; 20];
    let actual = rsi(&prices, 5);
    for (i, &v) in actual.iter().enumerate() {
        if i >= 5 {
            assert!(
                (v - 50.0).abs() < 1e-9,
                "rsi_constant[{i}]: expected 50.0, got {v}"
            );
        }
    }
}

#[test]
fn rsi_monotone_up_pegs_to_100() {
    // All deltas > 0 → avg_loss stays 0 → RSI = 100.
    let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    let actual = rsi(&prices, 14);
    for (i, &v) in actual.iter().enumerate() {
        if i >= 14 {
            assert!(
                (v - 100.0).abs() < 1e-9,
                "rsi_monotone_up[{i}]: expected 100.0, got {v}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Bollinger Bands
//
// Convention (from impl):
//   middle = SMA(period)
//   σ = population std dev (divisor = period, not period-1) — matches TA-Lib.
//   upper = middle + k*σ
//   lower = middle - k*σ
//   NaN for i < period-1.
//
// Tolerance 1e-12 for simple integer series.
// ---------------------------------------------------------------------------

#[test]
fn bollinger_period3_k2_hand_computed() {
    // prices = [2, 4, 6, 8, 10], period = 3, k = 2.0
    //
    // out[2]: window = [2,4,6], mean=4, var=((2-4)²+(4-4)²+(6-4)²)/3=(4+0+4)/3=8/3, σ=√(8/3)
    //         upper=4+2*√(8/3), lower=4-2*√(8/3)
    // out[3]: window = [4,6,8], mean=6, var=((4-6)²+(6-6)²+(8-6)²)/3=(4+0+4)/3=8/3, σ=√(8/3)
    //         upper=6+2*√(8/3), lower=6-2*√(8/3)
    // out[4]: window = [6,8,10], mean=8, var=8/3, σ=√(8/3)
    //         upper=8+2*√(8/3), lower=8-2*√(8/3)
    let prices = [2.0_f64, 4.0, 6.0, 8.0, 10.0];
    let bb = bollinger(&prices, 3, 2.0);
    let sigma = (8.0_f64 / 3.0).sqrt();

    // middle
    assert_series_eq(
        "bb_middle",
        &bb.middle,
        &[f64::NAN, f64::NAN, 4.0, 6.0, 8.0],
        1e-12,
    );
    // upper
    assert_series_eq(
        "bb_upper",
        &bb.upper,
        &[
            f64::NAN,
            f64::NAN,
            4.0 + 2.0 * sigma,
            6.0 + 2.0 * sigma,
            8.0 + 2.0 * sigma,
        ],
        1e-12,
    );
    // lower
    assert_series_eq(
        "bb_lower",
        &bb.lower,
        &[
            f64::NAN,
            f64::NAN,
            4.0 - 2.0 * sigma,
            6.0 - 2.0 * sigma,
            8.0 - 2.0 * sigma,
        ],
        1e-12,
    );
}

#[test]
fn bollinger_constant_series_zero_width() {
    // Constant series → σ = 0 → upper == lower == middle at all valid positions.
    let prices = vec![50.0_f64; 20];
    let bb = bollinger(&prices, 5, 2.0);
    for i in 4..20 {
        assert!(
            (bb.upper[i] - bb.lower[i]).abs() < 1e-12,
            "bollinger_constant[{i}]: bands should collapse"
        );
        assert!(
            (bb.middle[i] - 50.0).abs() < 1e-12,
            "bollinger_constant[{i}]: middle should be 50"
        );
    }
}

#[test]
fn bollinger_population_stddev_not_sample() {
    // Verify the impl uses population std dev (divisor=period), not sample (period-1).
    // For window [1,3]: mean=2, population var=((1-2)²+(3-2)²)/2=1, σ=1.
    //                          sample var = 2/(2-1) = 2, σ=√2.
    // We use period=2, k=1 so upper = middle + σ.
    let prices = [1.0_f64, 3.0];
    let bb = bollinger(&prices, 2, 1.0);
    // population σ = 1.0
    assert!(
        (bb.upper[1] - 3.0).abs() < 1e-12,
        "upper should be 3.0 (population σ=1), got {}",
        bb.upper[1]
    );
    assert!(
        (bb.lower[1] - 1.0).abs() < 1e-12,
        "lower should be 1.0 (population σ=1), got {}",
        bb.lower[1]
    );
}

// ---------------------------------------------------------------------------
// ATR — Average True Range (Wilder smoothed)
//
// Convention (from impl):
//   TR[0] = high[0] - low[0]  (no previous close at bar 0)
//   TR[i] = max(H-L, |H-PC|, |L-PC|) for i >= 1
//   Seed: simple mean of TR[1..=period] (NOTE: TR[0] is excluded from seed).
//   ATR[period] = seed; then Wilder EMA, alpha = 1/period.
//   NaN for i < period.
//
// Tolerance 1e-4 (Wilder smoothing accumulates rounding).
// ---------------------------------------------------------------------------

#[test]
fn atr_period2_hand_computed() {
    // period=2 for tractable hand computation.
    // high  = [10, 12, 11, 14, 13]
    // low   = [ 8,  9,  9, 10, 11]
    // close = [ 9, 11, 10, 12, 12]
    //
    // TR[0] = 10-8 = 2 (no prev close)
    // TR[1] = max(12-9, |12-9|, |9-9|) = max(3, 3, 0) = 3
    // TR[2] = max(11-9, |11-11|, |9-11|) = max(2, 0, 2) = 2
    // TR[3] = max(14-10, |14-10|, |10-10|) = max(4, 4, 0) = 4
    // TR[4] = max(13-11, |13-12|, |11-12|) = max(2, 1, 1) = 2
    //
    // Seed = mean(TR[1..=2]) = (3+2)/2 = 2.5
    // ATR[2] = 2.5
    // alpha = 1/2 = 0.5
    // ATR[3] = 0.5*2.5 + 0.5*4 = 1.25 + 2.0 = 3.25
    // ATR[4] = 0.5*3.25 + 0.5*2 = 1.625 + 1.0 = 2.625
    let high = [10.0_f64, 12.0, 11.0, 14.0, 13.0];
    let low = [8.0_f64, 9.0, 9.0, 10.0, 11.0];
    let close = [9.0_f64, 11.0, 10.0, 12.0, 12.0];
    let actual = atr(&high, &low, &close, 2);
    let expected = [f64::NAN, f64::NAN, 2.5, 3.25, 2.625];
    assert_series_eq("atr_period2", &actual, &expected, 1e-10);
}

#[test]
fn atr_period3_hand_computed() {
    // period=3
    // high  = [20, 22, 21, 23, 22, 24]
    // low   = [18, 19, 18, 20, 19, 21]
    // close = [19, 21, 20, 22, 21, 23]
    //
    // TR[0] = 20-18 = 2
    // TR[1] = max(22-19, |22-19|, |19-19|) = max(3,3,0) = 3
    // TR[2] = max(21-18, |21-21|, |18-21|) = max(3,0,3) = 3
    // TR[3] = max(23-20, |23-20|, |20-20|) = max(3,3,0) = 3
    // TR[4] = max(22-19, |22-22|, |19-22|) = max(3,0,3) = 3
    // TR[5] = max(24-21, |24-21|, |21-21|) = max(3,3,0) = 3
    //
    // Seed = mean(TR[1..=3]) = (3+3+3)/3 = 3.0
    // ATR[3] = 3.0
    // alpha = 1/3
    // ATR[4] = (2/3)*3.0 + (1/3)*3 = 3.0
    // ATR[5] = (2/3)*3.0 + (1/3)*3 = 3.0
    let high = [20.0_f64, 22.0, 21.0, 23.0, 22.0, 24.0];
    let low = [18.0_f64, 19.0, 18.0, 20.0, 19.0, 21.0];
    let close = [19.0_f64, 21.0, 20.0, 22.0, 21.0, 23.0];
    let actual = atr(&high, &low, &close, 3);
    let expected = [f64::NAN, f64::NAN, f64::NAN, 3.0, 3.0, 3.0];
    assert_series_eq("atr_period3", &actual, &expected, 1e-10);
}

#[test]
fn atr_constant_range_converges() {
    // When TR is constant (= C) at every bar, ATR converges to C.
    // With Wilder smoothing alpha=1/14: ATR_t = C*(1-(1-1/14)^t) + ATR_0*(1-1/14)^t
    // As t→∞, ATR→C. After 40 bars the convergence error < 0.01 for C=5.
    let n = 60usize;
    let high: Vec<f64> = (0..n).map(|_| 105.0).collect();
    let low: Vec<f64> = (0..n).map(|_| 100.0).collect();
    let close: Vec<f64> = (0..n).map(|_| 102.5).collect();
    let actual = atr(&high, &low, &close, 14);
    // TR[0] = 5, TR[i>0] = max(5, |105-102.5|, |100-102.5|) = max(5,2.5,2.5) = 5
    // Seed = mean(TR[1..=14]) = 5.0, so ATR[14] = 5.0 exactly.
    assert!(
        (actual[14] - 5.0).abs() < 1e-10,
        "atr seed should be 5.0, got {}",
        actual[14]
    );
    // By bar 50 Wilder has fully converged.
    assert!(
        (actual[50] - 5.0).abs() < 0.001,
        "atr should converge to 5.0, got {}",
        actual[50]
    );
}

// ---------------------------------------------------------------------------
// MACD
//
// Convention (from impl):
//   fast_ema  = ema(prices, fast)      valid from index fast-1
//   slow_ema  = ema(prices, slow)      valid from index slow-1
//   macd_line = fast_ema - slow_ema    valid from index slow-1
//   signal    = ema(macd_line[slow-1..], signal_period)
//               valid from slow-1 + signal_period - 1 = slow + signal - 2 (0-indexed)
//   histogram = macd_line - signal     valid wherever both are valid
//
// For standard (12, 26, 9): first valid signal = index 25+8 = 33.
//
// Tolerance 1e-4 (EMA recursion over 26+ steps accumulates rounding).
// ---------------------------------------------------------------------------

#[test]
fn macd_period_small_hand_computed() {
    // Use fast=2, slow=3, signal=2 on 8 bars so we can hand-compute.
    // prices = [1,2,3,4,5,6,7,8]
    //
    // EMA(2): alpha=2/3, seed at [1]=1.5
    //   [2]: 2/3*3 + 1/3*1.5 = 2+0.5 = 2.5
    //   [3]: 2/3*4 + 1/3*2.5 = 8/3+5/6 = 16/6+5/6 = 21/6 = 3.5
    //   [4]: 2/3*5 + 1/3*3.5 = 10/3+7/6 = 20/6+7/6 = 27/6 = 4.5
    //   [5]: 2/3*6 + 1/3*4.5 = 4+1.5 = 5.5
    //   [6]: 2/3*7 + 1/3*5.5 = 14/3+11/6 = 28/6+11/6 = 39/6 = 6.5
    //   [7]: 2/3*8 + 1/3*6.5 = 16/3+13/6 = 32/6+13/6 = 45/6 = 7.5
    //
    // EMA(3): alpha=0.5, seed at [2]=(1+2+3)/3=2.0
    //   [3]: 0.5*4+0.5*2=3.0
    //   [4]: 0.5*5+0.5*3=4.0
    //   [5]: 0.5*6+0.5*4=5.0
    //   [6]: 0.5*7+0.5*5=6.0
    //   [7]: 0.5*8+0.5*6=7.0
    //
    // macd_line (valid from index 2 = slow-1):
    //   [2]: NaN (fast valid, slow valid at 2: fast[2]=2.5, slow[2]=2.0 → 0.5)
    //   Wait — fast is valid from index 1 (period=2), slow from index 2 (period=3).
    //   So macd_line[2] = 2.5 - 2.0 = 0.5
    //   [3]: 3.5 - 3.0 = 0.5
    //   [4]: 4.5 - 4.0 = 0.5
    //   [5]: 5.5 - 5.0 = 0.5
    //   [6]: 6.5 - 6.0 = 0.5
    //   [7]: 7.5 - 7.0 = 0.5
    //
    // Signal = ema(macd_line[2..], 2), valid_start=2
    //   valid = [0.5, 0.5, 0.5, 0.5, 0.5, 0.5] (6 elements)
    //   alpha = 2/3
    //   seed at local[1] = (0.5+0.5)/2 = 0.5
    //   local[2]: 2/3*0.5 + 1/3*0.5 = 0.5
    //   ... all remain 0.5
    //   signal_partial = [NaN, 0.5, 0.5, 0.5, 0.5, 0.5]
    //   mapped back: signal_full[2+0..2+5] = [NaN,0.5,0.5,0.5,0.5,0.5]
    //   → signal_full = [NaN,NaN,NaN,0.5,0.5,0.5,0.5,0.5]
    //
    // histogram = macd - signal (where both valid):
    //   [3..7] = 0.5 - 0.5 = 0.0
    let prices = [1.0_f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let m = macd(&prices, 2, 3, 2);

    // macd_line
    let nan = f64::NAN;
    let expected_macd = [nan, nan, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
    assert_series_eq("macd_line", &m.macd, &expected_macd, 1e-10);

    // signal: NaN at [0,1,2], valid from [3]
    for i in 0..3 {
        assert!(m.signal[i].is_nan(), "macd_signal[{i}] must be NaN");
    }
    for i in 3..8 {
        assert!(
            (m.signal[i] - 0.5).abs() < 1e-10,
            "macd_signal[{i}]: expected 0.5, got {}",
            m.signal[i]
        );
    }

    // histogram
    for i in 0..3 {
        assert!(m.histogram[i].is_nan(), "macd_histogram[{i}] must be NaN");
    }
    for i in 3..8 {
        assert!(
            (m.histogram[i] - 0.0).abs() < 1e-10,
            "macd_histogram[{i}]: expected 0.0, got {}",
            m.histogram[i]
        );
    }
}

#[test]
fn macd_period_small_nonconstant_signal_hand_computed() {
    // Non-linear prices make the MACD input to the signal EMA vary.
    // fast=2 alpha=2/3, slow=3 alpha=1/2, signal=2 alpha=2/3.
    //
    // prices = [1,3,2,6,4,7,5,9]
    //
    // fast EMA valid values:
    //   [1]=2, [2]=2, [3]=14/3, [4]=38/9,
    //   [5]=164/27, [6]=434/81, [7]=1892/243
    // slow EMA valid values:
    //   [2]=2, [3]=4, [4]=4, [5]=11/2, [6]=21/4, [7]=57/8
    // macd_line:
    //   [2]=0, [3]=2/3, [4]=2/9, [5]=31/54, [6]=35/324, [7]=1285/1944
    //
    // Signal is EMA(2) over macd_line[2..]:
    //   seed local[1] = (0 + 2/3)/2 = 1/3
    //   local[2] = 7/27, local[3] = 38/81,
    //   local[4] = 37/162, local[5] = 1507/2916
    let prices = [1.0_f64, 3.0, 2.0, 6.0, 4.0, 7.0, 5.0, 9.0];
    let m = macd(&prices, 2, 3, 2);

    let nan = f64::NAN;
    let expected_macd = [
        nan,
        nan,
        0.0,
        2.0 / 3.0,
        2.0 / 9.0,
        31.0 / 54.0,
        35.0 / 324.0,
        1285.0 / 1944.0,
    ];
    let expected_signal = [
        nan,
        nan,
        nan,
        1.0 / 3.0,
        7.0 / 27.0,
        38.0 / 81.0,
        37.0 / 162.0,
        1507.0 / 2916.0,
    ];
    let expected_histogram = [
        nan,
        nan,
        nan,
        1.0 / 3.0,
        -1.0 / 27.0,
        17.0 / 162.0,
        -13.0 / 108.0,
        841.0 / 5832.0,
    ];

    assert_series_eq("macd_nonconstant_line", &m.macd, &expected_macd, 1e-10);
    assert_series_eq("macd_nonconstant_signal", &m.signal, &expected_signal, 1e-10);
    assert_series_eq(
        "macd_nonconstant_histogram",
        &m.histogram,
        &expected_histogram,
        1e-10,
    );
}

#[test]
fn macd_standard_warmup_indices() {
    // Standard MACD(12, 26, 9) on 100 bars:
    // - slow_ema valid from index 25
    // - macd_line valid from index 25 (= slow-1)
    // - signal = ema of macd_line[25..], period=9 → valid at local index 8
    //   → global index = 25 + 8 = 33
    // - histogram valid from index 33
    let prices: Vec<f64> = (1..=100).map(|i| i as f64).collect();
    let m = macd(&prices, 12, 26, 9);

    assert_eq!(m.macd.len(), 100, "macd length");
    assert_eq!(m.signal.len(), 100, "signal length");
    assert_eq!(m.histogram.len(), 100, "histogram length");

    // All NaN before macd warm-up (index 25)
    for i in 0..25 {
        assert!(m.macd[i].is_nan(), "macd_line[{i}] must be NaN");
    }
    // macd_line valid from index 25
    assert!(m.macd[25].is_finite(), "macd_line[25] should be finite");

    // Signal NaN before index 33
    for i in 0..33 {
        assert!(m.signal[i].is_nan(), "signal[{i}] must be NaN");
    }
    assert!(m.signal[33].is_finite(), "signal[33] should be finite");
    assert!(m.histogram[33].is_finite(), "histogram[33] should be finite");
}

#[test]
fn macd_histogram_equals_macd_minus_signal() {
    // histogram = macd - signal at every valid position
    let prices: Vec<f64> = (1..=60).map(|i| (i as f64) * 1.5 + 50.0).collect();
    let m = macd(&prices, 12, 26, 9);
    for i in 0..60 {
        if !m.macd[i].is_nan() && !m.signal[i].is_nan() {
            let expected_hist = m.macd[i] - m.signal[i];
            assert!(
                (m.histogram[i] - expected_hist).abs() < 1e-10,
                "histogram[{i}]: expected macd-signal={expected_hist:.8}, got {}",
                m.histogram[i]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Donchian Channels
//
// Convention (from impl):
//   upper[i] = max(high[i+1-period..=i])
//   lower[i] = min(low[i+1-period..=i])
//   NaN for i < period-1.
//
// Tolerance 1e-12 (simple max/min on exact floats).
// ---------------------------------------------------------------------------

#[test]
fn donchian_period3_hand_computed() {
    // high = [1,3,2,5,4,7,6], low = [0,2,1,4,3,6,5], period = 3
    //
    // upper[2] = max(1,3,2) = 3;  lower[2] = min(0,2,1) = 0
    // upper[3] = max(3,2,5) = 5;  lower[3] = min(2,1,4) = 1
    // upper[4] = max(2,5,4) = 5;  lower[4] = min(1,4,3) = 1
    // upper[5] = max(5,4,7) = 7;  lower[5] = min(4,3,6) = 3
    // upper[6] = max(4,7,6) = 7;  lower[6] = min(3,6,5) = 3
    let high = [1.0_f64, 3.0, 2.0, 5.0, 4.0, 7.0, 6.0];
    let low = [0.0_f64, 2.0, 1.0, 4.0, 3.0, 6.0, 5.0];
    let d = donchian(&high, &low, 3);

    let expected_upper = [f64::NAN, f64::NAN, 3.0, 5.0, 5.0, 7.0, 7.0];
    let expected_lower = [f64::NAN, f64::NAN, 0.0, 1.0, 1.0, 3.0, 3.0];
    assert_series_eq("donchian_upper", &d.upper, &expected_upper, 1e-12);
    assert_series_eq("donchian_lower", &d.lower, &expected_lower, 1e-12);
}

#[test]
fn donchian_period1_equals_input() {
    // period=1 → window is just the current bar → upper=high, lower=low
    let high = [5.0_f64, 3.0, 8.0, 1.0];
    let low = [2.0_f64, 1.0, 4.0, 0.5];
    let d = donchian(&high, &low, 1);
    assert_series_eq("donchian_upper_p1", &d.upper, &high, 1e-12);
    assert_series_eq("donchian_lower_p1", &d.lower, &low, 1e-12);
}

#[test]
fn donchian_period_equals_length() {
    // period = n → only out[n-1] is valid; equals global max/min
    let high = [3.0_f64, 7.0, 2.0, 9.0, 4.0];
    let low = [1.0_f64, 5.0, 0.0, 6.0, 2.0];
    let d = donchian(&high, &low, 5);
    for i in 0..4 {
        assert!(d.upper[i].is_nan(), "donchian_upper[{i}] must be NaN");
        assert!(d.lower[i].is_nan(), "donchian_lower[{i}] must be NaN");
    }
    assert!(
        (d.upper[4] - 9.0).abs() < 1e-12,
        "donchian upper[4] = global max = 9.0, got {}",
        d.upper[4]
    );
    assert!(
        (d.lower[4] - 0.0).abs() < 1e-12,
        "donchian lower[4] = global min = 0.0, got {}",
        d.lower[4]
    );
}

// ---------------------------------------------------------------------------
// Fibonacci Retracements
//
// Convention (from impl):
//   Uses the last `lookback` bars; finds global high and low within that window.
//   direction = Down if high_idx < low_idx (high comes before low — downtrend).
//   direction = Up if low_idx < high_idx.
//   levels computed as: level = high - ratio * (high - low)
//   (i.e., levels are measured from the swing high downward regardless of direction).
//   Standard ratios: 0.236, 0.382, 0.500, 0.618, 0.786.
//
// Tolerance 1e-9.
// ---------------------------------------------------------------------------

#[test]
fn fib_uptrend_hand_computed() {
    // Low precedes high → direction = Up
    // low=10 at index 0, high=20 at index 4; span = 10
    // 0.236 level: 20 - 0.236*10 = 17.64
    // 0.382 level: 20 - 0.382*10 = 16.18
    // 0.500 level: 20 - 0.500*10 = 15.00
    // 0.618 level: 20 - 0.618*10 = 13.82
    // 0.786 level: 20 - 0.786*10 = 12.14
    let prices = [10.0_f64, 12.0, 15.0, 18.0, 20.0];
    let f = fib_retracements(&prices, 5).expect("should compute");
    assert_eq!(f.direction, Direction::Up);
    assert!((f.high - 20.0).abs() < 1e-9);
    assert!((f.low - 10.0).abs() < 1e-9);
    let expected = [
        (0.236, 17.64),
        (0.382, 16.18),
        (0.500, 15.00),
        (0.618, 13.82),
        (0.786, 12.14),
    ];
    for (i, (&(ratio, level), &(expected_ratio, exp))) in f.levels.iter().zip(expected.iter()).enumerate() {
        assert!(
            (ratio - expected_ratio).abs() < 1e-12,
            "fib_uptrend ratio[{i}]: expected={expected_ratio}, got={ratio}"
        );
        assert!(
            (level - exp).abs() < 1e-9,
            "fib_uptrend level[{i}] ratio={ratio}: expected={exp:.4}, got={level:.4}"
        );
    }
}

#[test]
fn fib_downtrend_hand_computed() {
    // High precedes low → direction = Down
    // high=100 at index 0, low=60 at index 4; span = 40
    // 0.236 level: 100 - 0.236*40 = 100 - 9.44 = 90.56
    // 0.382 level: 100 - 0.382*40 = 100 - 15.28 = 84.72
    // 0.500 level: 100 - 0.500*40 = 80.00
    // 0.618 level: 100 - 0.618*40 = 100 - 24.72 = 75.28
    // 0.786 level: 100 - 0.786*40 = 100 - 31.44 = 68.56
    let prices = [100.0_f64, 90.0, 80.0, 70.0, 60.0];
    let f = fib_retracements(&prices, 5).expect("should compute");
    assert_eq!(f.direction, Direction::Down);
    assert!((f.high - 100.0).abs() < 1e-9);
    assert!((f.low - 60.0).abs() < 1e-9);
    let span = 40.0_f64;
    let ratios = [0.236, 0.382, 0.500, 0.618, 0.786];
    for (i, (&(ratio, level), &r)) in f.levels.iter().zip(ratios.iter()).enumerate() {
        let exp = 100.0 - r * span;
        assert!(
            (ratio - r).abs() < 1e-12,
            "fib_downtrend ratio[{i}]: expected={r}, got={ratio}"
        );
        assert!(
            (level - exp).abs() < 1e-9,
            "fib_downtrend level[{i}] ratio={ratio}: expected={exp:.4}, got={level:.4}"
        );
    }
}

#[test]
fn fib_returns_none_for_flat_series() {
    // All prices equal → span == 0 → None
    let prices = vec![50.0_f64; 10];
    assert!(fib_retracements(&prices, 10).is_none());
}

#[test]
fn fib_returns_none_when_lookback_too_small() {
    let prices = [10.0_f64, 20.0, 15.0];
    // lookback < 3 → None
    assert!(fib_retracements(&prices, 2).is_none());
}

#[test]
fn fib_uses_lookback_window_not_full_series() {
    // Full series has high=100 at index 0, but lookback only covers the last 3
    // bars where the high is 20 and low is 10.
    let prices = [100.0_f64, 10.0, 15.0, 12.0, 20.0];
    let f = fib_retracements(&prices, 3).expect("should compute");
    // Window = [15, 12, 20] → high=20, low=12
    assert!(
        (f.high - 20.0).abs() < 1e-9,
        "should use window high=20, got {}",
        f.high
    );
    assert!(
        (f.low - 12.0).abs() < 1e-9,
        "should use window low=12, got {}",
        f.low
    );
}
