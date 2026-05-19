//! Task 1 — locks the new `Scenario` struct shape via a serde round-trip.

use chrono::{TimeZone, Utc};
use xvision_core::Capital;
use xvision_engine::eval::scenario::*;

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
        asset: vec![asset_ref(symbol)],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
        capital: Capital::default(),
        granularity: BarGranularity::Hour1,
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
    }
}

fn asset_ref(symbol: &str) -> AssetRef {
    AssetRef {
        class: AssetClass::Crypto,
        symbol: symbol.into(),
        venue_symbol: format!("{symbol}/USD"),
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
        asset: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "ETH".into(),
            venue_symbol: "ETH/USD".into(),
        }],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 2, 3, 0, 0, 0).unwrap(),
        },
        capital: Capital::default(),
        granularity: BarGranularity::Hour1,
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
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: Scenario = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scenario_validation_accepts_valid_crypto_scenario() {
    valid_crypto_scenario("ETH").validate_v1().unwrap();
}

#[test]
fn scenario_validation_rejects_unsupported_asset() {
    let mut scenario = valid_crypto_scenario("ETH");
    scenario.asset[0].symbol = "XRP".into();

    let err = scenario.validate_v1().unwrap_err();

    assert!(err.to_string().contains("Alpaca crypto whitelist"));
}

#[test]
fn scenario_validation_requires_single_crypto_usd_asset() {
    let mut scenario = valid_crypto_scenario("ETH");
    scenario.asset.push(asset_ref("SOL"));
    let err = scenario.validate_v1().unwrap_err();
    assert!(err.to_string().contains("single asset"));

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
