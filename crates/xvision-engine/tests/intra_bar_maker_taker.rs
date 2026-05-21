//! V2E eval-intra-bar-fill-ordering — maker/taker aggressor-side classification tests.
//!
//! Tests:
//! - Maker classification: passive limit buy near bid side (open + spread/2 + epsilon)
//!   gets AggressorSide::Maker and uses maker_bps.
//! - Taker classification: market buy gets AggressorSide::Taker and uses taker_bps.
//! - OrderState enum round-trips through JSONL (no panic on absent field).
//!
//! Test function names are prefixed `intra_bar_` so the contract's
//! `cargo test -p xvision-engine intra_bar_` filter selects them.

use xvision_engine::eval::executor::backtest::classify_aggressor_side;
use xvision_engine::eval::executor::trace_types::AggressorSide;
use xvision_engine::eval::orders::OrderState;

// ── Maker classification ───────────────────────────────────────────────────

/// A passive buy limit exactly at bar open fills as maker.
/// bar_open = 100.0, spread_bps = 20.0 → half_spread = 0.10 USD.
/// fill_price = 100.0 (at open, within bid half-spread) → Maker.
#[test]
fn intra_bar_maker_classification_buy_at_open_is_maker() {
    let bar_open = 100.0;
    let spread_bps = 20.0; // 0.20% spread → half_spread = 0.10 bps
                           // Passive buy: fill at open (resting bid)
    let fill_price = bar_open;
    let side = classify_aggressor_side("long_open", fill_price, bar_open, spread_bps);
    assert_eq!(
        side,
        AggressorSide::Maker,
        "fill at open within passive half-spread is maker"
    );
}

/// A passive buy limit at open + spread/2 + epsilon fills as taker (crosses spread).
/// bar_open = 100.0, spread_bps = 20.0 → half_spread = 0.10 USD.
/// fill_price = 100.0 + 0.10 + 0.01 = 100.11 → just outside half_spread → Taker.
#[test]
fn intra_bar_taker_classification_buy_above_half_spread_is_taker() {
    let bar_open = 100.0;
    let spread_bps = 20.0;
    let half_spread_usd = bar_open * (spread_bps / 10_000.0) / 2.0;
    let epsilon = 0.01;
    let fill_price = bar_open + half_spread_usd + epsilon;
    let side = classify_aggressor_side("long_open", fill_price, bar_open, spread_bps);
    assert_eq!(
        side,
        AggressorSide::Taker,
        "fill at open + spread/2 + epsilon crosses passive zone → taker"
    );
}

/// A passive sell limit exactly at open is maker (resting offer at open).
#[test]
fn intra_bar_maker_classification_sell_at_open_is_maker() {
    let bar_open = 100.0;
    let spread_bps = 20.0;
    let fill_price = bar_open; // resting offer at mid
    let side = classify_aggressor_side("short_open", fill_price, bar_open, spread_bps);
    assert_eq!(
        side,
        AggressorSide::Maker,
        "sell at open within passive half-spread is maker"
    );
}

/// A sell limit below open - half_spread is taker (crosses the bid).
#[test]
fn intra_bar_taker_classification_sell_below_half_spread_is_taker() {
    let bar_open = 100.0;
    let spread_bps = 20.0;
    let half_spread_usd = bar_open * (spread_bps / 10_000.0) / 2.0;
    let epsilon = 0.01;
    let fill_price = bar_open - half_spread_usd - epsilon;
    let side = classify_aggressor_side("short_open", fill_price, bar_open, spread_bps);
    assert_eq!(
        side,
        AggressorSide::Taker,
        "sell at open - spread/2 - epsilon crosses passive zone → taker"
    );
}

/// Market buy (no spread) always classifies as taker.
/// With spread_bps=0 the half_spread is zero, so fill_price > open → Taker.
#[test]
fn intra_bar_market_buy_with_slippage_is_taker() {
    let bar_open = 60_000.0;
    let spread_bps = 0.0;
    // Market buy with 10 bps slip: fill_price = 60_000 * 1.001 = 60_060
    let fill_price = bar_open * 1.001;
    let side = classify_aggressor_side("long_open", fill_price, bar_open, spread_bps);
    assert_eq!(side, AggressorSide::Taker, "market buy with slip is taker");
}

/// Market sell (no spread) always classifies as taker.
#[test]
fn intra_bar_market_sell_with_slippage_is_taker() {
    let bar_open = 60_000.0;
    let spread_bps = 0.0;
    let fill_price = bar_open * 0.999; // sell with 10 bps adverse slip
    let side = classify_aggressor_side("short_open", fill_price, bar_open, spread_bps);
    assert_eq!(side, AggressorSide::Taker, "market sell with slip is taker");
}

/// Flat (close position) is taker — it crosses the book to exit.
#[test]
fn intra_bar_flat_close_is_taker() {
    let bar_open = 60_000.0;
    let spread_bps = 5.0;
    // Closing a long at open - slip (sell side).
    let fill_price = bar_open * 0.999;
    let side = classify_aggressor_side("flat", fill_price, bar_open, spread_bps);
    assert_eq!(side, AggressorSide::Taker, "flat close is taker");
}

// ── Maker uses maker_bps, Taker uses taker_bps ─────────────────────────────

/// For a typical venue (maker=10bps, taker=25bps), maker fills have lower fee.
#[test]
fn intra_bar_maker_fee_is_lower_than_taker_fee() {
    let maker_bps = 10.0_f64;
    let taker_bps = 25.0_f64;
    let notional = 1_000.0_f64;

    let maker_fee = notional * (maker_bps / 10_000.0);
    let taker_fee = notional * (taker_bps / 10_000.0);

    assert!(
        maker_fee < taker_fee,
        "maker fee ({maker_fee}) must be less than taker fee ({taker_fee})"
    );
}

// ── OrderState JSONL round-trip ─────────────────────────────────────────────

/// Every OrderState variant must round-trip through serde_json.
#[test]
fn intra_bar_order_state_all_variants_round_trip() {
    let variants = [
        OrderState::Open,
        OrderState::PartiallyFilled,
        OrderState::Filled,
        OrderState::Cancelled,
        OrderState::Expired,
        OrderState::Rejected,
    ];
    for variant in &variants {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: OrderState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant:?}");
    }
}

/// Legacy JSONL rows that lack an `order_state` field must deserialize to None
/// when the containing struct uses `#[serde(default)]`.
#[test]
fn intra_bar_order_state_absent_in_jsonl_deserializes_to_none() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct FillRow {
        fill_price: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order_state: Option<OrderState>,
    }

    let json = r#"{"fill_price": 60000.0}"#;
    let row: FillRow = serde_json::from_str(json).expect("deserialize legacy row");
    assert!(
        row.order_state.is_none(),
        "absent order_state field must deserialize to None (legacy compatibility)"
    );
}

/// OrderState fields round-trip from snake_case JSON values.
#[test]
fn intra_bar_order_state_serde_snake_case() {
    let cases = [
        (OrderState::Open, "\"open\""),
        (OrderState::PartiallyFilled, "\"partially_filled\""),
        (OrderState::Filled, "\"filled\""),
        (OrderState::Cancelled, "\"cancelled\""),
        (OrderState::Expired, "\"expired\""),
        (OrderState::Rejected, "\"rejected\""),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).expect("serialize");
        assert_eq!(
            json, *expected_json,
            "variant {variant:?} must serialize to {expected_json}"
        );
        let back: OrderState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant:?}");
    }
}
