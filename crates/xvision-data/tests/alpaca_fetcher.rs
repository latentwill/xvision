use chrono::{TimeZone, Utc};
use std::str::FromStr;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xvision_data::alpaca::{AlpacaBarsFetcher, BarGranularity, FetchError};

#[test]
fn bar_granularity_parses_supported_alpaca_timeframes() {
    assert_eq!(
        BarGranularity::from_str("1m").unwrap().as_alpaca_str(),
        "1Min"
    );
    assert_eq!(
        BarGranularity::from_str("59Min").unwrap().as_alpaca_str(),
        "59Min"
    );
    assert_eq!(
        BarGranularity::from_str("23h").unwrap().as_alpaca_str(),
        "23Hour"
    );
    assert_eq!(
        BarGranularity::from_str("1w").unwrap().as_alpaca_str(),
        "1Week"
    );
    assert_eq!(
        BarGranularity::from_str("12mo").unwrap().as_alpaca_str(),
        "12Month"
    );
    assert_eq!(
        BarGranularity::from_str("12M").unwrap().as_alpaca_str(),
        "12Month"
    );
}

#[test]
fn bar_granularity_rejects_unsupported_alpaca_timeframes() {
    assert!(BarGranularity::from_str("60m").is_err());
    assert!(BarGranularity::from_str("24h").is_err());
    assert!(BarGranularity::from_str("2d").is_err());
    assert!(BarGranularity::from_str("2w").is_err());
    assert!(BarGranularity::from_str("5mo").is_err());
}

#[test]
fn bar_granularity_deserializes_legacy_variant_names() {
    let g: BarGranularity = serde_json::from_str("\"Hour4\"").unwrap();

    assert_eq!(g, BarGranularity::Hour4);
    assert_eq!(serde_json::to_string(&g).unwrap(), "\"4h\"");
}

#[tokio::test]
async fn fetch_crypto_bars_single_page() {
    let server = MockServer::start().await;

    let body = serde_json::json!({
        "bars": {
            "ETH/USD": [
                {"t": "2024-02-03T00:00:00Z", "o": 2300.0, "h": 2320.0, "l": 2290.0, "c": 2310.0, "v": 1500.0, "n": 42, "vw": 2305.0},
                {"t": "2024-02-03T01:00:00Z", "o": 2310.0, "h": 2330.0, "l": 2305.0, "c": 2325.0, "v": 1700.0, "n": 51, "vw": 2317.0}
            ]
        },
        "next_page_token": null
    });

    Mock::given(method("GET"))
        .and(path("/v1beta3/crypto/us/bars"))
        .and(query_param("symbols", "ETH/USD"))
        .and(query_param("timeframe", "1Hour"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let fetcher = AlpacaBarsFetcher::new(
        server.uri(),
        "key".into(),
        "secret".into(),
    );
    let bars = fetcher
        .fetch_crypto_bars(
            "ETH/USD",
            BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 2, 3, 2, 0, 0).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].open, 2300.0);
    assert_eq!(bars[1].close, 2325.0);
}

#[tokio::test]
async fn fetch_crypto_bars_paginated() {
    let server = MockServer::start().await;

    let page1 = serde_json::json!({
        "bars": {"ETH/USD": [{"t": "2024-02-03T00:00:00Z", "o": 1.0, "h": 1.0, "l": 1.0, "c": 1.0, "v": 1.0}]},
        "next_page_token": "TOKEN_2"
    });
    let page2 = serde_json::json!({
        "bars": {"ETH/USD": [{"t": "2024-02-03T01:00:00Z", "o": 2.0, "h": 2.0, "l": 2.0, "c": 2.0, "v": 2.0}]},
        "next_page_token": null
    });

    Mock::given(method("GET")).and(path("/v1beta3/crypto/us/bars")).and(query_param("page_token", ""))
        .respond_with(ResponseTemplate::new(200).set_body_json(page1))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/v1beta3/crypto/us/bars")).and(query_param("page_token", "TOKEN_2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page2))
        .mount(&server).await;

    let bars = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars("ETH/USD", BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024,2,3,0,0,0).unwrap(),
            Utc.with_ymd_and_hms(2024,2,3,2,0,0).unwrap()).await.unwrap();

    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].open, 1.0);
    assert_eq!(bars[1].open, 2.0);
}

#[tokio::test]
async fn fetch_returns_unauthorized_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1beta3/crypto/us/bars"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server).await;
    let err = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars("ETH/USD", BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024,2,3,0,0,0).unwrap(),
            Utc.with_ymd_and_hms(2024,2,3,1,0,0).unwrap()).await.unwrap_err();
    assert!(matches!(err, FetchError::Unauthorized));
}

#[tokio::test]
async fn fetch_returns_asset_not_found_on_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1beta3/crypto/us/bars"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;
    let err = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars(
            "ETH/USD",
            BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 2, 3, 1, 0, 0).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::AssetNotFound(_)));
}

#[tokio::test]
async fn fetch_rejects_unknown_asset_before_http() {
    let server = MockServer::start().await;
    let err = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars(
            "XRP/USD",
            BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 2, 3, 1, 0, 0).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::AssetNotFound(_)));
}

#[tokio::test]
async fn fetch_rejects_pre_history_window_before_http() {
    let server = MockServer::start().await;
    let err = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars(
            "ETH/USD",
            BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2020, 1, 1, 1, 0, 0).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::RangeOutsideHistory { .. }));
}

#[tokio::test]
async fn fetch_returns_rate_limited_on_429() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1beta3/crypto/us/bars"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
        .mount(&server).await;
    let err = AlpacaBarsFetcher::new(server.uri(), "k".into(), "s".into())
        .fetch_crypto_bars("ETH/USD", BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024,2,3,0,0,0).unwrap(),
            Utc.with_ymd_and_hms(2024,2,3,1,0,0).unwrap()).await.unwrap_err();
    assert!(matches!(err, FetchError::RateLimited { retry_after_secs: 30 }));
}
