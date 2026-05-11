use chrono::{TimeZone, Utc};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xvision_data::alpaca::{AlpacaBarsFetcher, BarGranularity};

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
