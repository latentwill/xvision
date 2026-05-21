//! V2E eval-intra-bar-fill-ordering — FillBranch variant tests.
//!
//! Tests one scenario per FillBranch variant:
//! - GapPast: bar opens past the trigger price → fill at open.
//! - OhlcHighFirst: H closer to O than L → O→H→L→C, buy trigger on H leg.
//! - OhlcLowFirst: L closer to O than H → O→L→H→C, sell trigger on L leg.
//! - NextOpenOnly: trigger not reached; market orders always use this branch.
//!
//! Test function names are prefixed `intra_bar_` so the contract's
//! `cargo test -p xvision-engine intra_bar_` filter selects them.

use xvision_engine::eval::executor::backtest::{corwin_schultz_spread_bps, intra_bar_fill_branch};
use xvision_engine::eval::executor::trace_types::FillBranch;

// ── GapPast ────────────────────────────────────────────────────────────────

/// A buy-stop at 100. Bar opens at 105 (gap up past trigger) → fill at open.
#[test]
fn intra_bar_gap_past_buy_stop() {
    let (branch, fill_price) = intra_bar_fill_branch(
        105.0, // bar_open: already past trigger
        110.0, // bar_high
        104.0, // bar_low
        100.0, // trigger_price
        true,  // is_buy
    );
    assert_eq!(branch, FillBranch::GapPast, "should be GapPast");
    assert_eq!(fill_price, Some(105.0), "gap fill at bar open");
}

/// A sell-stop at 100. Bar opens at 95 (gap down past trigger) → fill at open.
#[test]
fn intra_bar_gap_past_sell_stop() {
    let (branch, fill_price) = intra_bar_fill_branch(
        95.0,  // bar_open: already past trigger
        96.0,  // bar_high
        93.0,  // bar_low
        100.0, // trigger_price
        false, // is_sell
    );
    assert_eq!(branch, FillBranch::GapPast, "should be GapPast");
    assert_eq!(fill_price, Some(95.0), "gap fill at bar open");
}

/// Trigger exactly at open counts as gap-past (open >= trigger for buys).
#[test]
fn intra_bar_gap_past_trigger_at_open_is_gap() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open == trigger
        105.0, 98.0, 100.0, // trigger_price == open
        true,
    );
    assert_eq!(branch, FillBranch::GapPast, "open == trigger counts as gap");
    assert_eq!(fill_price, Some(100.0));
}

// ── OhlcHighFirst ──────────────────────────────────────────────────────────

/// O=100, H=105, L=90 → |H-O|=5, |L-O|=10. H is closer → OhlcHighFirst.
/// Buy trigger at 103: H=105 >= 103 → fill at trigger via high leg.
#[test]
fn intra_bar_ohlc_high_first_buy_trigger_on_high_leg() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high (closer to open, |105-100|=5 vs |90-100|=10)
        90.0,  // bar_low
        103.0, // trigger_price: between open and high
        true,  // is_buy
    );
    assert_eq!(branch, FillBranch::OhlcHighFirst, "H closer to O → OhlcHighFirst");
    assert_eq!(fill_price, Some(103.0), "fill at trigger price on high leg");
}

/// OhlcHighFirst sequence: sell trigger reached on the low leg.
/// O=100, H=105, L=90 → H closer to O.
/// Sell trigger at 92: L=90 <= 92 → hit during the low leg (after high).
#[test]
fn intra_bar_ohlc_high_first_sell_trigger_on_low_leg() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high
        90.0,  // bar_low
        92.0,  // trigger_price: sell stop below low
        false, // is_sell
    );
    assert_eq!(branch, FillBranch::OhlcHighFirst, "H closer to O → OhlcHighFirst");
    assert_eq!(fill_price, Some(92.0), "fill at trigger price on low leg");
}

// ── OhlcLowFirst ───────────────────────────────────────────────────────────

/// O=100, H=120, L=95 → |H-O|=20, |L-O|=5. L is closer → OhlcLowFirst.
/// Sell trigger at 97: L=95 <= 97 → fill on low leg.
#[test]
fn intra_bar_ohlc_low_first_sell_trigger_on_low_leg() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        120.0, // bar_high (farther from open)
        95.0,  // bar_low (closer to open, |95-100|=5 vs |120-100|=20)
        97.0,  // trigger_price: sell stop between open and low
        false, // is_sell
    );
    assert_eq!(branch, FillBranch::OhlcLowFirst, "L closer to O → OhlcLowFirst");
    assert_eq!(fill_price, Some(97.0), "fill at trigger price on low leg");
}

/// OhlcLowFirst sequence: buy trigger reached on the high leg.
/// O=100, H=120, L=95 → L closer to O.
/// Buy trigger at 115: H=120 >= 115 → hit on high leg (after low).
#[test]
fn intra_bar_ohlc_low_first_buy_trigger_on_high_leg() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        120.0, // bar_high (farther from open)
        95.0,  // bar_low (closer to open)
        115.0, // trigger_price: buy stop between open and high
        true,  // is_buy
    );
    assert_eq!(branch, FillBranch::OhlcLowFirst, "L closer to O → OhlcLowFirst");
    assert_eq!(fill_price, Some(115.0), "fill at trigger price on high leg");
}

// ── NextOpenOnly — trigger not reached ─────────────────────────────────────

/// Buy trigger at 110, but H=105 — trigger never reached within the bar.
#[test]
fn intra_bar_next_open_only_buy_trigger_not_reached() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high: does NOT reach trigger
        98.0,  // bar_low
        110.0, // trigger_price: above high
        true,
    );
    assert_eq!(
        branch,
        FillBranch::NextOpenOnly,
        "trigger above high → not reached → NextOpenOnly"
    );
    assert_eq!(fill_price, None, "no fill when trigger not crossed");
}

/// Sell trigger at 90, but L=95 — trigger never reached within the bar.
#[test]
fn intra_bar_next_open_only_sell_trigger_not_reached() {
    let (branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high
        95.0,  // bar_low: does NOT reach trigger
        90.0,  // trigger_price: below low
        false,
    );
    assert_eq!(
        branch,
        FillBranch::NextOpenOnly,
        "trigger below low → not reached → NextOpenOnly"
    );
    assert_eq!(fill_price, None, "no fill when trigger not crossed");
}

// ── Limit-doesn't-cross stays Open ─────────────────────────────────────────

/// A limit buy at 110 on a bar that only reaches H=105.
/// The limit price is never crossed, so the order stays Open (no fill).
#[test]
fn intra_bar_limit_buy_that_doesnt_cross_stays_open() {
    // intra_bar_fill_branch returns (NextOpenOnly, None) when the trigger
    // is not reached — the caller is responsible for marking the order Open.
    let (_branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high: does NOT reach limit at 110
        98.0,  // bar_low
        110.0, // limit_price
        true,
    );
    assert!(
        fill_price.is_none(),
        "limit buy price 110 with H=105 must not fill — order stays Open"
    );
}

/// A limit sell at 90 on a bar with L=95. Trigger not reached → no fill.
#[test]
fn intra_bar_limit_sell_that_doesnt_cross_stays_open() {
    let (_branch, fill_price) = intra_bar_fill_branch(
        100.0, // bar_open
        105.0, // bar_high
        95.0,  // bar_low: does NOT reach limit at 90
        90.0,  // limit_price
        false,
    );
    assert!(
        fill_price.is_none(),
        "limit sell price 90 with L=95 must not fill — order stays Open"
    );
}

// ── Corwin-Schultz spread proxy ─────────────────────────────────────────────

/// Corwin-Schultz must return a finite, non-negative value for typical OHLC.
#[test]
fn intra_bar_corwin_schultz_returns_finite_nonnegative_for_typical_bar() {
    // BTC/USD-like bar: H=61_500, L=59_500
    let spread_bps = corwin_schultz_spread_bps(61_500.0, 59_500.0, &[]);
    assert!(spread_bps.is_finite(), "spread must be finite");
    assert!(spread_bps >= 0.0, "spread must be non-negative");
}

/// With a rolling window, the σ² term reduces the spread estimate.
#[test]
fn intra_bar_corwin_schultz_with_rolling_window_is_nonnegative() {
    let window = vec![0.0001, 0.0002, 0.0001, 0.00015, 0.0001];
    let spread_bps = corwin_schultz_spread_bps(61_500.0, 59_500.0, &window);
    assert!(spread_bps.is_finite(), "spread with window must be finite");
    assert!(spread_bps >= 0.0, "spread must be non-negative");
}

/// Degenerate inputs (H == L) should produce 0 spread without panicking.
#[test]
fn intra_bar_corwin_schultz_zero_range_bar_produces_zero() {
    let spread_bps = corwin_schultz_spread_bps(60_000.0, 60_000.0, &[]);
    assert!(spread_bps.is_finite(), "zero-range spread must be finite");
    assert_eq!(spread_bps, 0.0, "H==L must produce zero spread");
}

/// Invalid inputs (H < L, zero, negative) must not panic and must return 0.
#[test]
fn intra_bar_corwin_schultz_invalid_inputs_return_zero() {
    // H < L (inverted)
    assert_eq!(corwin_schultz_spread_bps(50_000.0, 60_000.0, &[]), 0.0);
    // H is zero
    assert_eq!(corwin_schultz_spread_bps(0.0, 1.0, &[]), 0.0);
    // Negative
    assert_eq!(corwin_schultz_spread_bps(-1.0, -2.0, &[]), 0.0);
}

/// Spread is always non-negative even when σ² > log(H/L)² (the max(0,…) guard).
#[test]
fn intra_bar_corwin_schultz_clamps_negative_radicand_to_zero() {
    // Provide a very large σ² window so the radicand goes negative.
    // s² = log_hl² - 2*ln(2)*σ² — if σ² dominates, this would be negative.
    // max(0, …) must clamp and return a non-negative spread.
    let large_sigma_window = vec![1.0; 20]; // very large variance
    let spread_bps = corwin_schultz_spread_bps(60_100.0, 59_900.0, &large_sigma_window);
    assert!(spread_bps >= 0.0, "spread clamped to non-negative");
    assert!(spread_bps.is_finite(), "spread must be finite");
}
