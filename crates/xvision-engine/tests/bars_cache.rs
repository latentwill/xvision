//! Task 7 integration test: `eval::bars::load_bars` cache wrapper.
//!
//! Verifies that a cache miss fetches from the upstream Alpaca fetcher and
//! persists the result, and that the immediate next call for the same
//! `cache_key` reads from `bars_cache` without re-hitting the upstream.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xvision_data::alpaca::{AlpacaBarsFetcher, BarGranularity};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::bars::{load_bars, BarCacheArgs};

/// Build an in-memory `ApiContext` whose Alpaca fetcher points at a wiremock
/// server returning four hourly bars for the test window. Wiremock counts
/// requests for us, so we don't need a separate counter on `ApiContext`.
async fn test_ctx_with_mock_alpaca() -> (ApiContext, MockServer) {
    let server = MockServer::start().await;

    let body = serde_json::json!({
        "bars": {
            "ETH/USD": [
                {"t": "2024-02-03T00:00:00Z", "o": 2300.0, "h": 2320.0, "l": 2290.0, "c": 2310.0, "v": 1500.0},
                {"t": "2024-02-03T01:00:00Z", "o": 2310.0, "h": 2330.0, "l": 2305.0, "c": 2325.0, "v": 1700.0},
                {"t": "2024-02-03T02:00:00Z", "o": 2325.0, "h": 2340.0, "l": 2320.0, "c": 2335.0, "v": 1600.0},
                {"t": "2024-02-03T03:00:00Z", "o": 2335.0, "h": 2350.0, "l": 2330.0, "c": 2345.0, "v": 1550.0}
            ]
        },
        "next_page_token": null
    });
    Mock::given(method("GET"))
        .and(path("/v1beta3/crypto/us/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    // In-memory pool + apply migration 005 (and friends) so `bars_cache`
    // exists. We mirror the layout of api_context tests rather than going
    // through `ApiContext::open` (no real filesystem).
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/005_bars_cache.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(AlpacaBarsFetcher::new(
        server.uri(),
        "test-key".into(),
        "test-secret".into(),
    ));
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "tester".into(),
        },
        dir.path().to_path_buf(),
    )
    .with_alpaca_fetcher(fetcher);
    // Hold the tempdir for the lifetime of the test via Box::leak —
    // keeps it on disk for any code that reads xvn_home.
    Box::leak(Box::new(dir));
    (ctx, server)
}

#[tokio::test]
async fn cache_miss_then_hit_returns_same_bars() {
    let (ctx, server) = test_ctx_with_mock_alpaca().await;
    let args = BarCacheArgs {
        cache_key: "test_key_eth_2024_1h".into(),
        asset_pair: "ETH/USD".into(),
        granularity: BarGranularity::Hour1,
        start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2024, 2, 3, 4, 0, 0).unwrap(),
        data_source_tag: "alpaca-historical-v1".into(),
    };

    let first = load_bars(&ctx, &args).await.unwrap();
    let second = load_bars(&ctx, &args).await.unwrap();
    assert_eq!(first.len(), 4);
    assert_eq!(first.len(), second.len());
    assert_eq!(first[0].timestamp, second[0].timestamp);
    assert_eq!(first[0].open, second[0].open);
    assert_eq!(first[3].close, second[3].close);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "second call should hit the cache (no second upstream fetch)"
    );
}

#[tokio::test]
async fn corrupted_cache_blob_treated_as_miss_and_self_heals() {
    let (ctx, server) = test_ctx_with_mock_alpaca().await;
    let args = BarCacheArgs {
        cache_key: "corrupt_key".into(),
        asset_pair: "ETH/USD".into(),
        granularity: BarGranularity::Hour1,
        start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2024, 2, 3, 4, 0, 0).unwrap(),
        data_source_tag: "alpaca-historical-v1".into(),
    };

    // Plant a garbage row — `compression='none'` so the deserialiser
    // tries to parse `DEADBEEF` as ndjson and trips its error path.
    sqlx::query(
        "INSERT INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES ('corrupt_key', 'ETH/USD', '1Hour', \
         '2024-02-03T00:00:00+00:00', '2024-02-03T04:00:00+00:00', \
         'alpaca-historical-v1', '2024-02-03T00:00:00+00:00', 4, \
         X'DEADBEEF', 'none')",
    )
    .execute(&ctx.db)
    .await
    .unwrap();

    // First call: hits corrupt row -> falls through to fetcher -> repopulates.
    let bars = load_bars(&ctx, &args).await.unwrap();
    assert!(!bars.is_empty());

    // The bad row should have been evicted and replaced; fetcher hit once.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "fetcher invoked once after corrupt-row eviction"
    );

    // Second call: hits the newly-cached good row, no extra fetch.
    let _ = load_bars(&ctx, &args).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "second call hits cleaned cache");
}
