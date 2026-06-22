//! Task 1 — locks the new `Scenario` struct JSON shape.

use chrono::{TimeZone, Utc};
use serde_json::json;
use xvision_core::Capital;
use xvision_engine::eval::scenario::*;
use xvision_engine::safety::VenueLabel;

fn valid_crypto_scenario(symbol: &str) -> Scenario {
    Scenario {
        id: "sc_validation".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: format!("{symbol} 2024"),
        description: "".into(),
        tags: vec!["regression".into(), symbol.to_lowercase()],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
        capital: Capital::default(),
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
            },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy {
            cache_key: "abc".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },

        warmup_bars: xvision_engine::eval::scenario::DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "edkenne@gmail.com".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

#[test]
fn scenario_serde_roundtrip() {
    let s = Scenario {
        id: "sc_test".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: "ETH 2024".into(),
        description: "".into(),
        tags: vec!["regression".into(), "eth".into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 2, 3, 0, 0, 0).unwrap(),
        },
        capital: Capital::default(),
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
            },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy {
            cache_key: "abc".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },

        warmup_bars: xvision_engine::eval::scenario::DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "edkenne@gmail.com".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    };
    let expected = json!({
        "id": "sc_test",
        "parent_scenario_id": null,
        "source": "User",
        "display_name": "ETH 2024",
        "description": "",
        "tags": ["regression", "eth"],
        "notes": null,
        "asset_class": "Crypto",
        "quote_currency": "Usd",
        "time_window": {
            "start": "2024-02-03T00:00:00Z",
            "end": "2025-02-03T00:00:00Z"
        },
        "granularity": "1h",
        "timezone": "UTC",
        "calendar": "Continuous24x7",
        "data_source": {
            "type": "AlpacaHistorical",
            "feed": null,
            "adjustment": "Raw"
        },
        "venue": {
            "venue": "Alpaca",
            "fees": {
                "maker_bps": 10,
                "taker_bps": 25
            },
            "slippage": {
                "model": "linear",
                "bps": 5
            },
            "latency": {
                "decision_to_fill_ms": 500
            },
            "fill_model": {
                "market_order_fill": "FullAtClose",
                "limit_order_fill": "NeverFills",
                "partial_fills": false,
                "volume_constraints": null
            },
            "borrow_bps_per_day": 5.0
        },
        "replay_mode": {
            "mode": "Continuous"
        },
        "capital": {
            "initial": 100000.0,
            "currency": "USD"
        },
        "bar_cache_policy": {
            "cache_key": "abc",
            "refresh_policy": {
                "policy": "NeverRefresh"
            },
            "data_fetched_at": null
        },
        "warmup_bars": 200,
        "regime_label": null,
        "volatility_label": null,
        "trend_direction": null,
        "regime_derived": false,
        "created_at": "2026-05-11T00:00:00Z",
        "created_by": "edkenne@gmail.com",
        "archived_at": null,
        "venue_label": "paper"
    });

    assert_eq!(serde_json::to_value(&s).unwrap(), expected);
    let back: Scenario = serde_json::from_value(expected).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scenario_validation_accepts_valid_crypto_scenario() {
    valid_crypto_scenario("ETH").validate_v1().unwrap();
}

#[test]
fn scenario_validation_requires_crypto_usd_envelope() {
    let mut scenario = valid_crypto_scenario("ETH");
    scenario.asset_class = AssetClass::Equity;
    let err = scenario.validate_v1().unwrap_err();
    assert!(err.to_string().contains("crypto assets only"));

    let mut scenario = valid_crypto_scenario("ETH");
    scenario.quote_currency = QuoteCurrency::Usdc;
    let err = scenario.validate_v1().unwrap_err();
    assert!(err.to_string().contains("USD quote currency only"));
}

#[test]
fn scenario_validation_rejects_future_end_time() {
    let mut scenario = valid_crypto_scenario("ETH");
    scenario.time_window.end = chrono::Utc::now() + chrono::Duration::days(1);

    let err = scenario.validate_v1().unwrap_err();

    assert!(err.to_string().contains("must be in the past"));
}
