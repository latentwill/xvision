//! Parity: `InputsPolicy::Raw` and `InputsPolicy::Oracle` must produce
//! byte-identical JSON from `build_decision_seed`, `ohlcv_to_json` (indirectly
//! via the seed), and `build_bar_history` (indirectly via the seed).
//!
//! Background — pipeline.rs invariant (lines 53-54): the two policies share a
//! single `Raw | Oracle` match arm in all three helpers, so their serialized
//! outputs must be textually equal character-by-character. That comment was
//! never enforced by a test that compares both serialized strings; this file
//! closes that gap.
//!
//! No async runtime, no DB, no mocks required. Only four public types:
//! `Ohlcv`, `InputsPolicy`, `DecisionSeedInput`, `build_decision_seed`.

use chrono::{TimeZone, Utc};
use xvision_core::market::Ohlcv;
use xvision_engine::agents::InputsPolicy;
use xvision_engine::eval::executor::backtest::{build_decision_seed, DecisionSeedInput, PerpsContext};
use xvision_engine::strategies::risk::RiskConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Construct an Ohlcv bar at hour offset `idx` from 2026-01-01T00:00:00Z.
/// Mirrors the helper in `eval_causal_input_sanitization.rs` exactly so both
/// test files exercise the same bar shape.
fn ohlcv(idx: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Ohlcv {
    Ohlcv {
        timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::hours(idx),
        open,
        high,
        low,
        close,
        volume,
    }
}

fn distinctive_risk() -> RiskConfig {
    RiskConfig {
        risk_pct_per_trade: 0.0137,
        max_concurrent_positions: 4,
        max_leverage: 3.5,
        stop_loss_atr_multiple: 7.5,
        daily_loss_kill_pct: 0.066,
        max_position_pct_nav: 17.0,
        max_funding_pay_8h: 0.0,
        min_liq_distance_pct: 0.0,
        max_total_exposure_pct: 0.0,
    }
}

/// Build a fully-populated seed using the given `policy` and a fixed set of
/// deterministic scalars. All non-policy fields are identical across calls so
/// any divergence in the returned `serde_json::Value` is caused solely by the
/// policy branch.
fn make_seed(policy: InputsPolicy) -> serde_json::Value {
    let history = vec![
        ohlcv(0, 100.0, 110.0, 90.0, 105.0, 1_000.0),
        ohlcv(1, 101.0, 111.0, 91.0, 106.0, 1_100.0),
        ohlcv(2, 102.0, 112.0, 92.0, 107.0, 1_200.0),
    ];
    let history_refs: Vec<&Ohlcv> = history.iter().collect();
    let current = ohlcv(3, 103.0, 113.0, 93.0, 108.0, 1_300.0);
    let active_assets = vec!["BTC/USD".to_string()];
    let risk = distinctive_risk();

    build_decision_seed(DecisionSeedInput {
        decision_idx: 7,
        asset: "BTC/USD",
        active_assets: &active_assets,
        bar: &current,
        next_bar_open: 109.0,
        reference_price_source: "eval_bar.close",
        position_size: 0.5,
        equity: 10_000.0,
        mark_price: 108.0,
        history_slice: &history_refs,
        inputs_policy: policy,
        entry_price: 100.0,
        unrealized_pnl_pct: 8.0,
        bars_held: 4,
        stop_loss_price: 95.0,
        take_profit_price: 120.0,
        risk_config: &risk,
        // Spot/backtest path: empty perps context (all `None`). Identical for
        // both policies, so the Raw-vs-Oracle byte-identity invariant holds.
        perps: PerpsContext::default(),
        supported_timeframes: &[],
        last_closed_times: Default::default(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Primary invariant: `Raw` and `Oracle` must serialise to byte-identical
/// strings. If the shared `Raw | Oracle` match arm is ever split into two
/// separate arms this test fails immediately.
#[test]
fn raw_and_oracle_seed_are_byte_identical() {
    let raw_str = serde_json::to_string(&make_seed(InputsPolicy::Raw)).unwrap();
    let oracle_str = serde_json::to_string(&make_seed(InputsPolicy::Oracle)).unwrap();
    assert_eq!(
        raw_str, oracle_str,
        "Raw and Oracle must produce byte-identical seed JSON (pipeline.rs invariant)\n\
         Raw:    {raw_str}\n\
         Oracle: {oracle_str}",
    );
}

/// Targeted guard: drill into `market_data.current_bar` and
/// `market_data.bar_history[0]` so that if the nested bar shape drifts
/// between the two policies the failure message names the exact field.
#[test]
fn raw_and_oracle_per_bar_json_are_byte_identical() {
    let raw = make_seed(InputsPolicy::Raw);
    let oracle = make_seed(InputsPolicy::Oracle);

    // current_bar
    let raw_current = serde_json::to_string(&raw["market_data"]["current_bar"]).unwrap();
    let oracle_current = serde_json::to_string(&oracle["market_data"]["current_bar"]).unwrap();
    assert_eq!(
        raw_current, oracle_current,
        "market_data.current_bar must be byte-identical for Raw vs Oracle\n\
         Raw:    {raw_current}\n\
         Oracle: {oracle_current}",
    );

    // bar_history[0]
    let raw_h0 = serde_json::to_string(&raw["market_data"]["bar_history"][0]).unwrap();
    let oracle_h0 = serde_json::to_string(&oracle["market_data"]["bar_history"][0]).unwrap();
    assert_eq!(
        raw_h0, oracle_h0,
        "market_data.bar_history[0] must be byte-identical for Raw vs Oracle\n\
         Raw:    {raw_h0}\n\
         Oracle: {oracle_h0}",
    );
}
