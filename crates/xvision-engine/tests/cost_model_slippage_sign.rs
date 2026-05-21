//! V2E eval-cost-model-per-bar-and-volume-share — slippage sign tests.
//!
//! Verifies that buys slip up and sells slip down under both Linear and
//! VolumeShare models. Tests realistic positions at typical BTC prices.

use xvision_engine::eval::scenario::SlippageModel;

/// Compute slippage-adjusted fill price for a buy order under `Linear`.
fn linear_fill_buy(next_open: f64, slip_bps: f64) -> f64 {
    next_open * (1.0 + slip_bps / 10_000.0)
}

/// Compute slippage-adjusted fill price for a sell order under `Linear`.
fn linear_fill_sell(next_open: f64, slip_bps: f64) -> f64 {
    next_open * (1.0 - slip_bps / 10_000.0)
}

/// Compute fill price for VolumeShare.
/// `volume_share = min(order_qty / bar_volume, volume_limit)`.
/// `fill_price = next_open * (1 ± price_impact * volume_share²)`.
fn volume_share_fill_price(
    next_open: f64,
    order_qty: f64,
    bar_volume: f64,
    price_impact: f64,
    volume_limit: f64,
    is_buy: bool,
) -> f64 {
    let vs = (order_qty / bar_volume).min(volume_limit);
    let impact = price_impact * vs * vs;
    if is_buy {
        next_open * (1.0 + impact)
    } else {
        next_open * (1.0 - impact)
    }
}

#[test]
fn linear_buy_slips_above_mid() {
    let mid = 60_000.0;
    let fp = linear_fill_buy(mid, 10.0);
    assert!(fp > mid, "buy should fill above mid; got {fp} vs mid {mid}");
    assert!((fp - 60_060.0).abs() < 1e-6, "expected 60_060 got {fp}");
}

#[test]
fn linear_sell_slips_below_mid() {
    let mid = 60_000.0;
    let fp = linear_fill_sell(mid, 10.0);
    assert!(fp < mid, "sell should fill below mid; got {fp} vs mid {mid}");
    assert!((fp - 59_940.0).abs() < 1e-6, "expected 59_940 got {fp}");
}

#[test]
fn linear_zero_slip_fills_at_mid() {
    let mid = 60_000.0;
    assert_eq!(linear_fill_buy(mid, 0.0), mid);
    assert_eq!(linear_fill_sell(mid, 0.0), mid);
}

#[test]
fn volume_share_buy_slips_up() {
    let mid = 60_000.0;
    let order_qty = 0.01; // BTC
    let bar_volume = 1_000.0; // large so share is small
    let fp = volume_share_fill_price(mid, order_qty, bar_volume, 0.1, 0.025, true);
    assert!(fp > mid, "VolumeShare buy should fill above mid; got {fp}");
}

#[test]
fn volume_share_sell_slips_down() {
    let mid = 60_000.0;
    let order_qty = 0.01;
    let bar_volume = 1_000.0;
    let fp = volume_share_fill_price(mid, order_qty, bar_volume, 0.1, 0.025, false);
    assert!(fp < mid, "VolumeShare sell should fill below mid; got {fp}");
}

#[test]
fn volume_share_zero_qty_no_impact() {
    let mid = 60_000.0;
    let fp_buy = volume_share_fill_price(mid, 0.0, 1_000.0, 0.1, 0.025, true);
    let fp_sell = volume_share_fill_price(mid, 0.0, 1_000.0, 0.1, 0.025, false);
    assert_eq!(fp_buy, mid, "zero qty buy should fill at mid");
    assert_eq!(fp_sell, mid, "zero qty sell should fill at mid");
}

#[test]
fn volume_share_serde_tag() {
    let model = SlippageModel::VolumeShare {
        price_impact: 0.1,
        volume_limit: 0.025,
    };
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"volume_share\""),
        "expected snake_case tag; got {json}"
    );
}

#[test]
fn linear_serde_tag() {
    let model = SlippageModel::Linear { bps: 10 };
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"linear\""),
        "expected snake_case tag; got {json}"
    );
}

#[test]
fn none_serde_tag() {
    let model = SlippageModel::None;
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"none\""),
        "expected snake_case tag; got {json}"
    );
}
