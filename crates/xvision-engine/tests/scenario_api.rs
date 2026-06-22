use chrono::{TimeZone, Utc};
use xvision_data::asset_whitelist::ALPACA_CRYPTO_WHITELIST;
use xvision_engine::api::scenario::{archive, create, validate_request, CreateScenarioRequest};
use xvision_engine::api::{ApiContext, ApiError};
use xvision_engine::eval::scenario::*;

struct TestCtx {
    ctx: ApiContext,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for TestCtx {
    type Target = ApiContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

async fn test_ctx() -> TestCtx {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        xvision_engine::api::Actor::Cli { user: "test".into() },
    )
    .await
    .unwrap();
    TestCtx { ctx, _dir: dir }
}

fn valid_request() -> CreateScenarioRequest {
    CreateScenarioRequest {
        display_name: "ETH 2024".into(),
        description: "".into(),
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2024, 2, 10, 0, 0, 0).unwrap(),
        },
        capital: xvision_core::Capital::default(),
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
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
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
        warmup_bars: None,
    }
}

#[tokio::test]
async fn test_ctx_removes_temp_dir_on_drop() {
    let dir_path;
    {
        let ctx = test_ctx().await;
        dir_path = ctx._dir.path().to_path_buf();
        assert!(dir_path.exists());
    }
    assert!(!dir_path.exists());
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
async fn create_keeps_scenarios_asset_free() {
    let ctx = test_ctx().await;

    let mut req = valid_request();
    req.display_name = "asset-free 2024".into();

    let scenario = create(&ctx, req).await.unwrap();
    assert_eq!(scenario.asset_class, AssetClass::Crypto);
}

#[tokio::test]
async fn create_rejects_blank_display_name() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.display_name = "   ".into();

    let err = create(&ctx, req).await.unwrap_err();

    assert!(matches!(err, ApiError::Validation(_)));
    assert!(format!("{err}").contains("display_name is required"));
}

#[tokio::test]
async fn missing_display_name_reaches_actionable_validation() {
    let ctx = test_ctx().await;
    let mut body = serde_json::to_value(valid_request()).unwrap();
    body.as_object_mut().unwrap().remove("display_name");
    let req: CreateScenarioRequest = serde_json::from_value(body).unwrap();

    let err = validate_request(&req, &ctx).await.unwrap_err();

    assert!(matches!(err, ApiError::Validation(_)));
    assert!(format!("{err}").contains("display_name is required"));
}

#[tokio::test]
async fn create_trims_display_name_before_persisting() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.display_name = "  ETH 2024 named window  ".into();

    let s = create(&ctx, req).await.unwrap();

    assert_eq!(s.display_name, "ETH 2024 named window");
}

#[tokio::test]
async fn create_rejects_active_duplicate_display_name() {
    let ctx = test_ctx().await;
    let req = valid_request();
    let created = create(&ctx, req.clone()).await.unwrap();

    let err = create(&ctx, req.clone()).await.unwrap_err();

    assert!(matches!(err, ApiError::Validation(_)));
    assert!(format!("{err}").contains("display_name already exists"));

    archive(&ctx, &created.id).await.unwrap();
    let replacement = create(&ctx, req).await.unwrap();
    assert_eq!(replacement.display_name, "ETH 2024");
}

#[tokio::test]
async fn create_succeeds_with_hour4_granularity() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    let s = create(&ctx, req).await.unwrap();

    assert!(!s.bar_cache_policy.cache_key.is_empty());
}

#[tokio::test]
async fn create_succeeds_with_hour6_granularity() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    let s = create(&ctx, req).await.unwrap();

    assert!(!s.bar_cache_policy.cache_key.is_empty());
}

#[tokio::test]
async fn create_succeeds_with_minute_and_week_granularities() {
    let ctx = test_ctx().await;

    let mut minute_req = valid_request();
    let minute_scenario = create(&ctx, minute_req).await.unwrap();

    let mut week_req = valid_request();
    week_req.display_name = "ETH weekly 2024".into();
    let week_scenario = create(&ctx, week_req).await.unwrap();
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
async fn create_rejects_unsupported_replay_mode() {
    let ctx = test_ctx().await;
    let mut req = valid_request();
    req.replay_mode = ReplayMode::Stepped;
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}
