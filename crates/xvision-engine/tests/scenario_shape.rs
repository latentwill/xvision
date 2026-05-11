//! Task 1 — locks the new `Scenario` struct shape via a serde round-trip.

use chrono::{TimeZone, Utc};
use xvision_engine::eval::scenario::*;

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
        granularity: BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel { decision_to_fill_ms: 500 },
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
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "edkenne@gmail.com".into(),
        archived_at: None,
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: Scenario = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}
