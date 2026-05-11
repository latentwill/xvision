use chrono::{TimeZone, Utc};
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::scenario::{create, CreateScenarioRequest};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::scenario::*;

async fn test_ctx() -> ApiContext {
    use tempfile::tempdir;
    let dir = Box::leak(Box::new(tempdir().unwrap()));
    ApiContext::open(
        dir.path(),
        xvision_engine::api::Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap()
}

fn valid_request() -> CreateScenarioRequest {
    CreateScenarioRequest {
        display_name: "ETH 2024".into(),
        description: "".into(),
        asset_class: AssetClass::Crypto,
        asset: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "ETH".into(),
            venue_symbol: "ETH/USD".into(),
        }],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
        granularity: BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
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
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        replay_mode: ReplayMode::Continuous,
        tags: vec!["regression".into()],
        notes: None,
        parent_scenario_id: None,
        source: ScenarioSource::User,
    }
}

#[tokio::test]
async fn create_succeeds_with_valid_request() {
    let ctx = test_ctx().await;
    let req = valid_request();
    let s = create(&ctx, req).await.unwrap();
    assert_eq!(s.source, ScenarioSource::User);
    assert!(s.id.starts_with("sc_"));
    assert!(!s.bar_cache_policy.cache_key.is_empty());
}

#[tokio::test]
async fn create_rejects_multi_asset_v1() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.asset.push(AssetRef {
        class: AssetClass::Crypto,
        symbol: "BTC".into(),
        venue_symbol: "BTC/USD".into(),
    });
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}

#[tokio::test]
async fn create_rejects_history_floor_violation() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.time_window.start = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let err = create(&ctx, req).await.unwrap_err();
    assert!(format!("{err}").contains("before Alpaca crypto history"));
}

#[tokio::test]
async fn create_rejects_non_crypto_asset_class() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.asset_class = AssetClass::Equity;
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}

#[tokio::test]
async fn create_rejects_unsupported_granularity() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.granularity = BarGranularity::Minute5;
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}

#[tokio::test]
async fn create_rejects_unsupported_replay_mode() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.replay_mode = ReplayMode::Stepped;
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}

#[tokio::test]
async fn create_rejects_unwhitelisted_asset() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.asset[0].symbol = "XRP".into();
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}
