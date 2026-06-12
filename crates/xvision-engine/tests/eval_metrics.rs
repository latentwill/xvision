//! Tests for the eval metrics module — pure-compute helpers for Sharpe,
//! max drawdown, and per-period returns over an equity curve.

use xvision_engine::eval::metrics::{
    annualization_calendar_for_asset_class, annualization_periods_per_year,
    annualization_periods_per_year_for_asset_class, equity_to_returns, max_drawdown_pct,
    sharpe_from_returns, total_return_pct,
};
use xvision_engine::eval::scenario::AssetClass;

#[test]
fn equity_to_returns_yields_n_minus_one_periods() {
    let samples = vec![100.0, 110.0, 121.0, 132.0];
    let returns = equity_to_returns(&samples);
    assert_eq!(returns.len(), 3);
    assert!((returns[0] - 0.10).abs() < 1e-9, "first period 10%");
    assert!((returns[1] - 0.10).abs() < 1e-9);
    assert!((returns[2] - (11.0 / 121.0)).abs() < 1e-9);
}

#[test]
fn equity_to_returns_empty_or_single_returns_empty() {
    assert!(equity_to_returns(&[]).is_empty());
    assert!(equity_to_returns(&[100.0]).is_empty());
}

#[test]
fn equity_to_returns_handles_decline() {
    let samples = vec![100.0, 80.0, 90.0];
    let returns = equity_to_returns(&samples);
    assert!((returns[0] - (-0.20)).abs() < 1e-9);
    assert!((returns[1] - 0.125).abs() < 1e-9);
}

#[test]
fn equity_to_returns_skips_zero_or_negative_baselines() {
    // A 0 or negative baseline can't yield a sane percentage return; skip.
    let samples = vec![0.0, -100.0, 100.0, 110.0];
    let returns = equity_to_returns(&samples);
    // Only the 100 → 110 transition has a positive baseline.
    assert_eq!(returns.len(), 1);
    assert!((returns[0] - 0.10).abs() < 1e-9);
}

#[test]
fn sharpe_from_returns_zero_when_empty() {
    assert_eq!(sharpe_from_returns(&[], 8760.0), 0.0);
}

#[test]
fn sharpe_from_returns_zero_when_all_returns_equal() {
    // No volatility → Sharpe is undefined; return 0.0 by convention.
    let returns = vec![0.01; 50];
    assert_eq!(sharpe_from_returns(&returns, 8760.0), 0.0);
}

#[test]
fn sharpe_from_returns_positive_for_steady_positive_returns() {
    // Slight upward drift with some variance → positive Sharpe.
    let returns = vec![0.01, 0.02, 0.005, 0.015, 0.018, 0.012];
    let sharpe = sharpe_from_returns(&returns, 8760.0);
    assert!(sharpe > 0.0, "expected positive Sharpe, got {sharpe}");
}

#[test]
fn sharpe_from_returns_negative_for_losing_strategy() {
    let returns = vec![-0.01, -0.02, -0.005, -0.015];
    let sharpe = sharpe_from_returns(&returns, 8760.0);
    assert!(sharpe < 0.0, "expected negative Sharpe, got {sharpe}");
}

#[test]
fn sharpe_scales_with_periods_per_year() {
    let returns = vec![0.001, 0.002, 0.0005, 0.0015];
    let s_hourly = sharpe_from_returns(&returns, 8760.0);
    let s_daily = sharpe_from_returns(&returns, 365.0);
    // Annualization factor √(8760/365) ≈ √24 ≈ 4.9
    let ratio = s_hourly / s_daily;
    let expected = (8760.0_f64 / 365.0).sqrt();
    assert!(
        (ratio - expected).abs() < 1e-9,
        "expected annualization ratio {expected}, got {ratio}"
    );
}

#[test]
fn max_drawdown_pct_zero_for_monotonic_increase() {
    let samples = vec![100.0, 110.0, 121.0, 133.0];
    assert_eq!(max_drawdown_pct(&samples), 0.0);
}

#[test]
fn max_drawdown_pct_known_input() {
    // Peak 120 at index 1, trough 80 at index 3 → drawdown = (120-80)/120 = 33.33%.
    let samples = vec![100.0, 120.0, 100.0, 80.0, 110.0];
    let dd = max_drawdown_pct(&samples);
    let expected = ((120.0 - 80.0) / 120.0) * 100.0;
    assert!((dd - expected).abs() < 1e-9, "expected {expected}%, got {dd}",);
}

#[test]
fn max_drawdown_pct_handles_empty_or_single() {
    assert_eq!(max_drawdown_pct(&[]), 0.0);
    assert_eq!(max_drawdown_pct(&[100.0]), 0.0);
}

#[test]
fn max_drawdown_pct_resets_after_new_high() {
    // Peak1 = 110 (idx 1), trough 90 (idx 2) → dd1 = 18.18%
    // New peak2 = 200 (idx 3), trough 180 (idx 5) → dd2 = 10%
    // Max overall = 18.18%
    let samples = vec![100.0, 110.0, 90.0, 200.0, 190.0, 180.0];
    let dd = max_drawdown_pct(&samples);
    let expected = ((110.0 - 90.0) / 110.0) * 100.0;
    assert!((dd - expected).abs() < 1e-9, "expected {expected}%, got {dd}",);
}

#[test]
fn total_return_pct_basic() {
    assert!((total_return_pct(100.0, 110.0) - 10.0).abs() < 1e-9);
    assert!((total_return_pct(100.0, 90.0) - (-10.0)).abs() < 1e-9);
    assert_eq!(total_return_pct(0.0, 110.0), 0.0); // baseline 0 → 0 by convention
}

#[test]
fn annualization_periods_per_year_for_60min_cadence_is_8760() {
    assert!((annualization_periods_per_year(60) - 8760.0).abs() < 1e-9);
}

#[test]
fn annualization_periods_per_year_for_15min_cadence_is_35040() {
    assert!((annualization_periods_per_year(15) - 35040.0).abs() < 1e-9);
}

#[test]
fn annualization_periods_per_year_for_daily_cadence_is_365() {
    assert!((annualization_periods_per_year(60 * 24) - 365.0).abs() < 1e-9);
}

#[test]
fn annualization_periods_per_year_zero_or_negative_returns_one() {
    assert_eq!(annualization_periods_per_year(0), 1.0);
}

#[test]
fn calendar_annualization_crypto_60min_cadence_is_8760() {
    assert_eq!(
        annualization_calendar_for_asset_class(AssetClass::Crypto),
        "crypto_24_7_365d"
    );
    assert!(
        (annualization_periods_per_year_for_asset_class(AssetClass::Crypto, 60) - 8760.0).abs()
            < 1e-9
    );
}

#[test]
fn calendar_annualization_equity_60min_cadence_uses_regular_session_minutes() {
    let expected = 252.0 * 390.0 / 60.0;
    assert_eq!(
        annualization_calendar_for_asset_class(AssetClass::Equity),
        "us_market_252x390m"
    );
    assert!(
        (annualization_periods_per_year_for_asset_class(AssetClass::Equity, 60) - expected).abs()
            < 1e-9
    );
}

#[test]
fn calendar_annualization_zero_cadence_returns_one() {
    assert_eq!(
        annualization_periods_per_year_for_asset_class(AssetClass::Equity, 0),
        1.0
    );
}
