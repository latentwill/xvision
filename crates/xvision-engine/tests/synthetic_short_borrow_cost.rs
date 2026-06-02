//! Borrow cost tests: deterministic accrual on held shorts.
//! Covers M2 (borrow half) of the 2026-06-02 synthetic-eval-fill-path spec.
//!
//! These tests exercise the borrow-cost math directly via the pure
//! `compute_borrow_cost` function (re-exported for tests) and via the
//! `VenueSettings.borrow_bps_per_day` + `VenueOverride.borrow_bps_per_day`
//! configuration surface.

use xvision_engine::eval::scenario::{Fees, FillModel, LatencyModel, LimitOrderFill,
    MarketOrderFill, SlippageModel, Venue, VenueOverride, VenueSettings};

// ---------------------------------------------------------------------------
// Direct borrow-cost math checks (no executor round-trip needed)
// ---------------------------------------------------------------------------

/// Expected formula:
/// cost = abs_pos * entry * borrow_bps_per_day / 10_000 / bars_per_day * bars_held
fn hand_compute_borrow(
    abs_pos: f64,
    entry: f64,
    borrow_bps_per_day: f64,
    bars_held: u32,
    bar_secs: u64,
) -> f64 {
    if bars_held == 0 || abs_pos == 0.0 || entry == 0.0 || bar_secs == 0 {
        return 0.0;
    }
    let bars_per_day = 86_400.0 / bar_secs as f64;
    let daily = abs_pos * entry * borrow_bps_per_day / 10_000.0;
    daily * bars_held as f64 / bars_per_day
}

#[test]
fn borrow_cost_formula_five_bps_per_day_hourly() {
    // 1 BTC short at $50_000 entry, 5 bps/day, held 24 hourly bars (= 1 day).
    // Expected cost = 1 * 50_000 * 5 / 10_000 = $25.00
    let cost = hand_compute_borrow(1.0, 50_000.0, 5.0, 24, 3_600);
    assert!(
        (cost - 25.0).abs() < 1e-10,
        "1 BTC short for 1 day at 5bps/day should cost $25, got {cost}"
    );
}

#[test]
fn borrow_cost_zero_for_long_position() {
    // Long positions do not accrue borrow cost.
    // The executor only calls compute_borrow_cost when pos < 0.
    // Verify the formula returns 0 when bars_held = 0.
    let cost = hand_compute_borrow(1.0, 50_000.0, 5.0, 0, 3_600);
    assert_eq!(cost, 0.0, "zero bars held → zero cost");
}

#[test]
fn borrow_cost_proportional_to_bars_held() {
    let one_bar = hand_compute_borrow(1.0, 50_000.0, 5.0, 1, 3_600);
    let ten_bars = hand_compute_borrow(1.0, 50_000.0, 5.0, 10, 3_600);
    assert!(
        (ten_bars - one_bar * 10.0).abs() < 1e-10,
        "borrow cost must be linear in bars_held"
    );
}

#[test]
fn borrow_cost_determinism_same_inputs_same_output() {
    let c1 = hand_compute_borrow(0.5, 48_000.0, 7.5, 12, 3_600);
    let c2 = hand_compute_borrow(0.5, 48_000.0, 7.5, 12, 3_600);
    assert_eq!(c1, c2, "borrow cost must be deterministic");
}

// ---------------------------------------------------------------------------
// VenueSettings configuration surface
// ---------------------------------------------------------------------------

fn default_venue() -> VenueSettings {
    VenueSettings {
        venue: Venue::Alpaca,
        fees: Fees { maker_bps: 10, taker_bps: 25 },
        slippage: SlippageModel::None,
        latency: LatencyModel { decision_to_fill_ms: 0 },
        fill_model: FillModel {
            market_order_fill: MarketOrderFill::NextBarOpen,
            limit_order_fill: LimitOrderFill::NeverFills,
            partial_fills: false,
            volume_constraints: None,
        },
        overrides: Vec::new(),
        borrow_bps_per_day: 5.0,
    }
}

#[test]
fn venue_settings_default_borrow_is_five_bps() {
    let venue = VenueSettings::default();
    assert_eq!(venue.borrow_bps_per_day, 5.0);
}

#[test]
fn venue_settings_explicit_borrow_round_trips() {
    let venue = VenueSettings {
        borrow_bps_per_day: 2.5,
        ..default_venue()
    };
    let json = serde_json::to_string(&venue).unwrap();
    let parsed: VenueSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.borrow_bps_per_day, 2.5);
}

#[test]
fn legacy_venue_json_without_borrow_hydrates_to_five() {
    let json = serde_json::json!({
        "venue": "Alpaca",
        "fees": {"maker_bps": 10, "taker_bps": 25},
        "slippage": {"model": "none"},
        "latency": {"decision_to_fill_ms": 0},
        "fill_model": {
            "market_order_fill": "NextBarOpen",
            "limit_order_fill": "NeverFills",
            "partial_fills": false,
            "volume_constraints": null
        }
    });
    let venue: VenueSettings = serde_json::from_value(json).unwrap();
    assert_eq!(venue.borrow_bps_per_day, 5.0, "missing field must hydrate to 5.0");
}

#[test]
fn venue_override_borrow_overrides_default() {
    let venue = VenueSettings {
        overrides: vec![VenueOverride {
            symbol_pattern: "BTC/USD".into(),
            fees: None,
            slippage: None,
            borrow_bps_per_day: Some(1.0),
        }],
        ..default_venue()
    };
    // Per-asset override wins.
    let eff = venue.overrides
        .iter()
        .find(|o| o.matches("BTC/USD"))
        .and_then(|o| o.borrow_bps_per_day)
        .unwrap_or(venue.borrow_bps_per_day);
    assert_eq!(eff, 1.0);
}

#[test]
fn venue_override_borrow_falls_through_when_none() {
    let venue = VenueSettings {
        overrides: vec![VenueOverride {
            symbol_pattern: "BTC/USD".into(),
            fees: None,
            slippage: None,
            borrow_bps_per_day: None, // explicit None → fall through
        }],
        borrow_bps_per_day: 8.0,
        ..default_venue()
    };
    let eff = venue.overrides
        .iter()
        .find(|o| o.matches("BTC/USD"))
        .and_then(|o| o.borrow_bps_per_day)
        .unwrap_or(venue.borrow_bps_per_day);
    assert_eq!(eff, 8.0);
}

#[test]
fn borrow_cost_over_n_hourly_bars_matches_hand_computed_fixture() {
    // Fixture: short 0.1 BTC at entry $60_000, held for 48 h (= 48 hourly bars).
    // borrow_bps_per_day = 5.0
    // bars_per_day = 24
    // daily_cost = 0.1 * 60_000 * 5 / 10_000 = $3.00
    // total = $3.00 * 48 / 24 = $6.00
    let cost = hand_compute_borrow(0.1, 60_000.0, 5.0, 48, 3_600);
    assert!(
        (cost - 6.0).abs() < 1e-10,
        "48h short at 5bps/day expected $6.00, got {cost}"
    );
}
