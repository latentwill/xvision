//! Task 7 integration test: `eval::bars::load_bars` cache wrapper.
//!
//! Verifies that a cache miss fetches from the upstream Alpaca fetcher and
//! persists the result, and that the immediate next call for the same
//! `cache_key` reads from `bars_cache` without re-hitting the upstream.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xvision_data::alpaca::{AlpacaBarsFetcher, BarGranularity};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::bars::{compute_cache_key, load_bars, BarCacheArgs};

struct TestCtx {
    ctx: ApiContext,
    temp_dir: tempfile::TempDir,
}

impl std::ops::Deref for TestCtx {
    type Target = ApiContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

impl TestCtx {
    fn xvn_home(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }
}

fn utc(ts: &str) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .unwrap()
        .with_timezone(&Utc)
}

/// Build an in-memory `ApiContext` whose Alpaca fetcher points at a wiremock
/// server returning four hourly bars for the test window. Wiremock counts
/// requests for us, so we don't need a separate counter on `ApiContext`.
async fn test_ctx_with_mock_alpaca() -> (TestCtx, MockServer) {
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
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/010_bars_cache.sql"))
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
    (TestCtx { ctx, temp_dir: dir }, server)
}

#[tokio::test]
async fn bars_cache_schema_has_expected_columns_and_index() {
    let (ctx, _server) = test_ctx_with_mock_alpaca().await;

    let columns: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('bars_cache')")
        .fetch_all(&ctx.db)
        .await
        .unwrap();
    let names: Vec<String> = columns.into_iter().map(|(name,)| name).collect();

    for expected in [
        "cache_key",
        "asset",
        "granularity",
        "window_start",
        "window_end",
        "data_source",
        "fetched_at",
        "bar_count",
        "bars_blob",
        "compression",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "bars_cache missing column {expected}; got {names:?}"
        );
    }

    let indexes: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_index_list('bars_cache')")
        .fetch_all(&ctx.db)
        .await
        .unwrap();
    assert!(
        indexes.iter().any(|(name,)| name == "bars_cache_by_asset_window"),
        "bars_cache missing asset/window index; got {indexes:?}"
    );
}

#[tokio::test]
async fn test_context_tempdir_is_removed_on_drop() {
    let (ctx, server) = test_ctx_with_mock_alpaca().await;
    let xvn_home = ctx.xvn_home();
    assert!(xvn_home.exists());

    drop(ctx);
    drop(server);

    assert!(
        !xvn_home.exists(),
        "test context must own and drop its temporary XVN_HOME"
    );
}

#[test]
fn cache_key_is_stable_for_same_window() {
    let start = utc("2025-01-01T00:00:00Z");
    let end = utc("2025-01-02T00:00:00Z");

    let first = compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        start,
        end,
        "alpaca-historical-v1",
    );
    let second = compute_cache_key(
        "ETH/USD",
        xvision_data::alpaca::BarGranularity::Hour1,
        start,
        end,
        "alpaca-historical-v1",
    );

    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}

#[tokio::test]
async fn cache_miss_then_hit_returns_same_bars() {
    let (ctx, server) = test_ctx_with_mock_alpaca().await;
    let args = BarCacheArgs {
        cache_key: "test_key_eth_2024_1h".into(),
        asset_pair: "ETH/USD".into(),
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

#[tokio::test]
async fn concurrent_misses_serialize_through_singleflight() {
    let (ctx, server) = test_ctx_with_mock_alpaca().await;
    let args = std::sync::Arc::new(BarCacheArgs {
        cache_key: "singleflight_key".into(),
        asset_pair: "ETH/USD".into(),
        start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2024, 2, 3, 4, 0, 0).unwrap(),
        data_source_tag: "alpaca-historical-v1".into(),
    });

    let ctx = std::sync::Arc::new(ctx);
    let ctx_a = ctx.clone();
    let ctx_b = ctx.clone();
    let args_a = args.clone();
    let args_b = args.clone();

    let (a, b) = tokio::join!(async move { load_bars(&ctx_a, &args_a).await }, async move {
        load_bars(&ctx_b, &args_b).await
    },);
    a.unwrap();
    b.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "single-flight should de-dupe concurrent fetches"
    );
}
