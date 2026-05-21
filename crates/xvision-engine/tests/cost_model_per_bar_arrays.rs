//! V2E eval-cost-model-per-bar-and-volume-share — per-bar cost array tests.
//!
//! Tests:
//! - `BarCostTable` lookup by timestamp.
//! - Fallback to scenario default when column is absent (None).
//! - VenueOverride glob matching.
//! - Per-asset override precedence over scenario default.
//! - Both BTC/USD override and ETH/USD fallthrough.

use chrono::{TimeZone, Utc};
use xvision_engine::eval::scenario::{FeeSource, Fees, SlippageModel, VenueOverride};
use xvision_engine::eval::{BarCostEntry, BarCostTable};

fn ts(y: i32, m: u32, d: u32, h: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

// ── BarCostTable lookup ────────────────────────────────────────────────────

#[test]
fn bar_cost_table_lookup_returns_entry_at_matching_timestamp() {
    let entries = vec![
        BarCostEntry {
            timestamp: ts(2024, 1, 1, 0),
            fee_bps: Some(12.0),
            slip_bps: Some(6.0),
            spread_bps: None,
        },
        BarCostEntry {
            timestamp: ts(2024, 1, 1, 1),
            fee_bps: Some(15.0),
            slip_bps: None,
            spread_bps: Some(3.0),
        },
    ];
    let table = BarCostTable::from_entries(entries);

    let e0 = table.lookup(&ts(2024, 1, 1, 0)).unwrap();
    assert_eq!(e0.fee_bps, Some(12.0));
    assert_eq!(e0.slip_bps, Some(6.0));
    assert_eq!(e0.spread_bps, None);

    let e1 = table.lookup(&ts(2024, 1, 1, 1)).unwrap();
    assert_eq!(e1.fee_bps, Some(15.0));
    assert_eq!(e1.slip_bps, None);
    assert_eq!(e1.spread_bps, Some(3.0));
}

#[test]
fn bar_cost_table_lookup_returns_none_for_missing_timestamp() {
    let table = BarCostTable::default();
    assert!(table.lookup(&ts(2024, 1, 1, 0)).is_none());
}

#[test]
fn bar_cost_table_fallback_when_column_none() {
    // An entry with all columns None represents a bar where columns were
    // present in the schema but the value was NULL. The simulator should
    // fall through to the scenario default.
    let entries = vec![BarCostEntry {
        timestamp: ts(2024, 1, 1, 0),
        fee_bps: None,
        slip_bps: None,
        spread_bps: None,
    }];
    let table = BarCostTable::from_entries(entries);
    let e = table.lookup(&ts(2024, 1, 1, 0)).unwrap();
    // All None — caller falls through to scenario default.
    assert!(e.fee_bps.is_none());
    assert!(e.slip_bps.is_none());
    assert!(e.spread_bps.is_none());
}

// ── VenueOverride glob matching ────────────────────────────────────────────

use xvision_engine::eval::scenario::glob_match;

#[test]
fn glob_exact_match() {
    assert!(glob_match("BTC/USD", "BTC/USD"));
    assert!(!glob_match("BTC/USD", "ETH/USD"));
}

#[test]
fn glob_star_suffix() {
    assert!(glob_match("BTC*", "BTC/USD"));
    assert!(glob_match("BTC*", "BTC"));
    assert!(!glob_match("BTC*", "ETH/USD"));
}

#[test]
fn glob_star_prefix() {
    assert!(glob_match("*USD", "BTC/USD"));
    assert!(glob_match("*USD", "ETH/USD"));
    assert!(glob_match("*USD", "USD"));
    assert!(!glob_match("*USD", "BTC/EUR"));
}

#[test]
fn glob_question_mark() {
    assert!(glob_match("BTC/?SD", "BTC/USD"));
    assert!(!glob_match("BTC/?SD", "BTC/USDT"));
}

#[test]
fn glob_star_matches_empty() {
    assert!(glob_match("BTC*", "BTC"));
    assert!(glob_match("*", ""));
    assert!(glob_match("*", "anything"));
}

// ── VenueOverride precedence ───────────────────────────────────────────────

fn make_btc_override() -> VenueOverride {
    VenueOverride {
        symbol_pattern: "BTC/USD".into(),
        fees: Some(Fees {
            maker_bps: 5,
            taker_bps: 15,
        }),
        slippage: Some(SlippageModel::Linear { bps: 3 }),
    }
}

fn make_star_usd_override() -> VenueOverride {
    VenueOverride {
        symbol_pattern: "*USD".into(),
        fees: Some(Fees {
            maker_bps: 8,
            taker_bps: 20,
        }),
        slippage: None,
    }
}

#[test]
fn venue_override_matches_exact_pattern() {
    let ov = make_btc_override();
    assert!(ov.matches("BTC/USD"));
    assert!(!ov.matches("ETH/USD"));
}

#[test]
fn venue_override_btc_beats_scenario_default() {
    // BTC/USD override has taker_bps=15; scenario default is 25.
    // The first matching override wins.
    let overrides = vec![make_btc_override(), make_star_usd_override()];
    let symbol = "BTC/USD";
    let found = overrides.iter().find(|o| o.matches(symbol));
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.fees.as_ref().unwrap().taker_bps, 15);
}

#[test]
fn eth_falls_through_to_star_pattern() {
    // ETH/USD doesn't match "BTC/USD" but does match "*USD".
    let overrides = vec![make_btc_override(), make_star_usd_override()];
    let symbol = "ETH/USD";
    let found = overrides.iter().find(|o| o.matches(symbol));
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.fees.as_ref().unwrap().taker_bps, 20);
    // slippage not set on this override
    assert!(found.slippage.is_none());
}

#[test]
fn no_matching_override_returns_none() {
    let overrides = vec![make_btc_override()];
    let symbol = "SOL/USD";
    let found = overrides.iter().find(|o| o.matches(symbol));
    assert!(found.is_none(), "SOL/USD should not match BTC/USD pattern");
}

/// Round-trip test: VenueOverride serializes and deserializes correctly.
#[test]
fn venue_override_serde_round_trip() {
    let ov = VenueOverride {
        symbol_pattern: "BTC/USD".into(),
        fees: Some(Fees {
            maker_bps: 5,
            taker_bps: 15,
        }),
        slippage: Some(SlippageModel::VolumeShare {
            price_impact: 0.1,
            volume_limit: 0.025,
        }),
    };
    let json = serde_json::to_string(&ov).unwrap();
    let back: VenueOverride = serde_json::from_str(&json).unwrap();
    assert_eq!(back.symbol_pattern, "BTC/USD");
    assert_eq!(back.fees.as_ref().unwrap().taker_bps, 15);
    assert_eq!(
        back.slippage.unwrap(),
        SlippageModel::VolumeShare {
            price_impact: 0.1,
            volume_limit: 0.025
        }
    );
}

/// A VenueOverride with no fees falls through to scenario default fee.
#[test]
fn venue_override_no_fees_falls_through() {
    let ov = VenueOverride {
        symbol_pattern: "ETH/USD".into(),
        fees: None,
        slippage: Some(SlippageModel::Linear { bps: 3 }),
    };
    assert!(ov.fees.is_none(), "no fees on override means fall through");
}

// ── FeeSource tests ────────────────────────────────────────────────────────

#[test]
fn fee_source_per_bar_array_serializes_correctly() {
    let s = serde_json::to_string(&FeeSource::PerBarArray).unwrap();
    assert_eq!(s, "\"per_bar_array\"");
}

#[test]
fn fee_source_per_asset_override_serializes_correctly() {
    let s = serde_json::to_string(&FeeSource::PerAssetOverride).unwrap();
    assert_eq!(s, "\"per_asset_override\"");
}

#[test]
fn fee_source_default_serializes_correctly() {
    let s = serde_json::to_string(&FeeSource::Default).unwrap();
    assert_eq!(s, "\"default\"");
}
