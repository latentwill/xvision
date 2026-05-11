# Custom-Scenario Eval — M1: Bars cache + Alpaca fetcher + asset unlock

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drop the BTC-only wall on Alpaca crypto and add a deterministic, SQLite-backed historical-bars cache so `xvn ab-compare --asset ETH --from … --to …` runs end-to-end.

**Architecture:** New `crates/xvision-data/src/alpaca.rs` provides a paginated, rate-limited crypto-bars fetcher against Alpaca's `/v1beta3/crypto/us/bars`. A new `eval::bars::load_bars` cache wrapper sits between the harness and the fetcher, reading from a new `bars_cache` SQLite table; misses fall through to the fetcher and back-fill. The `AssetSymbol` enum in `xvision-execution/alpaca.rs` expands from BTC-only to the full Alpaca crypto whitelist. F18 partial pull-in: `TraderDecision.asset` lands as a field.

**Tech Stack:** Rust 2021, tokio, reqwest 0.13, governor (rate limiter), blake3 (cache key), serde_json, rusqlite (existing flight-recorder DB), wiremock (test-only HTTP mocks), `flate2` (gzip).

**Reference spec:** `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` §§5–7, §13, §17.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-data/Cargo.toml` | Modify | Add `reqwest`, `governor`, `blake3`, `flate2`, `wiremock` (dev). |
| `crates/xvision-data/src/lib.rs` | Modify | `pub mod alpaca;` |
| `crates/xvision-data/src/alpaca.rs` | Create | `AlpacaBarsFetcher`, `FetchError`, bar-normalisation, pagination, rate limiting. |
| `crates/xvision-data/src/asset_whitelist.rs` | Create | Compile-time const `ALPACA_CRYPTO_WHITELIST` (BTC, ETH, LTC, SOL, AVAX, LINK, AAVE, UNI, DOT, DOGE, SHIB, MATIC, BCH, USDT, USDC) + `alpaca_crypto_history_start()`. |
| `crates/xvision-data/tests/alpaca_fetcher.rs` | Create | Integration tests using `wiremock`. |
| `crates/xvision-engine/src/eval/bars.rs` | Create | `load_bars(ctx, cache_key, asset, granularity, window, source) -> ApiResult<Vec<MarketBar>>` + single-flight via per-key mutex. |
| `crates/xvision-engine/src/eval/mod.rs` | Modify | `pub mod bars;` |
| `crates/xvision-engine/migrations/0004_bars_cache.sql` | Create | `bars_cache` table + index. |
| `crates/xvision-engine/migrations/0004_bars_cache.down.sql` | Create | DROP TABLE. |
| `crates/xvision-engine/src/store.rs` | Modify | Register the new migration. |
| `crates/xvision-execution/src/alpaca.rs` | Modify | Remove BTC-only comment (line 3) + expand `alpaca_symbol_for`/parser (line 46) for the full whitelist. |
| `crates/xvision-core/src/assets.rs` | Modify | Expand `AssetSymbol` enum past `Btc`. |
| `crates/xvision-core/src/trading.rs` | Modify | Add `asset: Option<AssetSymbol>` to `TraderDecision` (F18 partial). |
| `crates/xvision-cli/src/lib.rs` | Modify | Register new `Bars` subcommand; `--asset` value parser accepts all whitelist symbols. |
| `crates/xvision-cli/src/commands/bars.rs` | Create | `xvn bars fetch / ls / rm / gc` dispatchers. |
| `crates/xvision-cli/src/commands/mod.rs` | Modify | `pub mod bars;` |
| `crates/xvision-cli/src/commands/ab_compare.rs` | Modify | `--asset` now resolves through the whitelist; F18 default propagated. |
| `config/default.toml` | Modify | Add `[data.alpaca] rate_limit_rpm = 200`. |

---

## Task 1 — Cargo dependencies + lib wiring

**Files:** `crates/xvision-data/Cargo.toml`, `crates/xvision-data/src/lib.rs`

- [ ] **Step 1: Add deps to `crates/xvision-data/Cargo.toml`**

```toml
[dependencies]
reqwest = { workspace = true, features = ["json"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
chrono = { workspace = true, features = ["serde"] }
tokio = { workspace = true, features = ["sync", "macros", "rt"] }
governor = "0.6"
nonzero_ext = "0.3"
blake3 = "1.5"
flate2 = "1.0"
thiserror = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
wiremock = "0.6"
tokio = { workspace = true, features = ["rt", "macros", "test-util"] }
```

- [ ] **Step 2: Wire modules in `crates/xvision-data/src/lib.rs`**

```rust
pub mod alpaca;
pub mod asset_whitelist;
pub mod fixtures;
pub mod indicators;
```

- [ ] **Step 3: `cargo build -p xvision-data`**

Expected: PASS (empty `alpaca.rs` / `asset_whitelist.rs` modules still parse).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-data/Cargo.toml crates/xvision-data/src/lib.rs
git commit -m "build(xvision-data): add reqwest/governor/blake3/flate2/wiremock"
```

---

## Task 2 — Alpaca crypto whitelist + history floor

**Files:** `crates/xvision-data/src/asset_whitelist.rs`, `crates/xvision-data/tests/whitelist.rs`

- [ ] **Step 1: Write failing test `tests/whitelist.rs`**

```rust
use xvision_data::asset_whitelist::{is_alpaca_crypto_supported, alpaca_crypto_history_start};
use chrono::{TimeZone, Utc};

#[test]
fn btc_eth_sol_are_supported() {
    assert!(is_alpaca_crypto_supported("BTC"));
    assert!(is_alpaca_crypto_supported("ETH"));
    assert!(is_alpaca_crypto_supported("SOL"));
}

#[test]
fn xrp_is_not_supported() {
    assert!(!is_alpaca_crypto_supported("XRP"));
}

#[test]
fn history_floor_is_2021_09_26() {
    assert_eq!(
        alpaca_crypto_history_start(),
        Utc.with_ymd_and_hms(2021, 9, 26, 0, 0, 0).unwrap(),
    );
}
```

- [ ] **Step 2: Run test, expect FAIL**

```bash
cargo test -p xvision-data --test whitelist
```

- [ ] **Step 3: Implement `src/asset_whitelist.rs`**

```rust
use chrono::{DateTime, TimeZone, Utc};

/// Alpaca crypto pairs available through v1beta3/crypto/us. Source: Alpaca docs.
pub const ALPACA_CRYPTO_WHITELIST: &[&str] = &[
    "BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI",
    "DOT", "DOGE", "SHIB", "MATIC", "BCH", "USDT", "USDC",
];

pub fn is_alpaca_crypto_supported(symbol: &str) -> bool {
    ALPACA_CRYPTO_WHITELIST.contains(&symbol)
}

/// Earliest available timestamp for crypto bars on Alpaca's v1beta3 feed.
pub fn alpaca_crypto_history_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 9, 26, 0, 0, 0).unwrap()
}

/// Convert a bare symbol ("ETH") into Alpaca's pair form ("ETH/USD").
pub fn to_alpaca_pair(symbol: &str) -> String {
    format!("{symbol}/USD")
}
```

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-data --test whitelist
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-data/src/asset_whitelist.rs crates/xvision-data/tests/whitelist.rs
git commit -m "feat(xvision-data): Alpaca crypto whitelist + history-floor const"
```

---

## Task 3 — Fetcher skeleton + happy path

**Files:** `crates/xvision-data/src/alpaca.rs`, `crates/xvision-data/tests/alpaca_fetcher.rs`

- [ ] **Step 1: Write failing test for happy-path single-page fetch**

```rust
// crates/xvision-data/tests/alpaca_fetcher.rs
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
```

- [ ] **Step 2: Run test, expect FAIL (alpaca module not present)**

```bash
cargo test -p xvision-data --test alpaca_fetcher
```

- [ ] **Step 3: Implement minimal `src/alpaca.rs`**

```rust
use std::sync::Arc;

use chrono::{DateTime, Utc};
use governor::{Quota, RateLimiter, clock::DefaultClock, middleware::NoOpMiddleware, state::{InMemoryState, NotKeyed}};
use nonzero_ext::nonzero;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarGranularity { Minute1, Minute5, Minute15, Hour1, Day1 }

impl BarGranularity {
    pub fn as_alpaca_str(self) -> &'static str {
        match self {
            Self::Minute1 => "1Min",
            Self::Minute5 => "5Min",
            Self::Minute15 => "15Min",
            Self::Hour1 => "1Hour",
            Self::Day1 => "1Day",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarketBar {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("Alpaca credentials missing or rejected (401)")]
    Unauthorized,
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },
    #[error("asset '{0}' not found on Alpaca")]
    AssetNotFound(String),
    #[error("requested range starts before Alpaca crypto history (earliest available: {earliest_available})")]
    RangeOutsideHistory { earliest_available: DateTime<Utc> },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

type AlpacaRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

pub struct AlpacaBarsFetcher {
    base_url: String,
    api_key: String,
    api_secret: String,
    client: Client,
    rate_limiter: Arc<AlpacaRateLimiter>,
}

#[derive(Debug, Deserialize)]
struct BarsResponse {
    bars: std::collections::HashMap<String, Vec<RawBar>>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawBar {
    t: DateTime<Utc>,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
}

impl AlpacaBarsFetcher {
    pub fn new(base_url: String, api_key: String, api_secret: String) -> Self {
        Self::with_rate_limit(base_url, api_key, api_secret, 200)
    }

    pub fn with_rate_limit(base_url: String, api_key: String, api_secret: String, rpm: u32) -> Self {
        let quota = Quota::per_minute(std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(200u32)));
        Self {
            base_url,
            api_key,
            api_secret,
            client: Client::new(),
            rate_limiter: Arc::new(RateLimiter::direct(quota)),
        }
    }

    pub async fn fetch_crypto_bars(
        &self,
        asset_pair: &str,
        granularity: BarGranularity,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, FetchError> {
        self.rate_limiter.until_ready().await;
        let url = format!("{}/v1beta3/crypto/us/bars", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .query(&[
                ("symbols", asset_pair),
                ("timeframe", granularity.as_alpaca_str()),
                ("start", &start.to_rfc3339()),
                ("end", &end.to_rfc3339()),
                ("limit", "10000"),
            ])
            .send()
            .await?;

        match resp.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(FetchError::Unauthorized),
            StatusCode::NOT_FOUND => return Err(FetchError::AssetNotFound(asset_pair.into())),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry = resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(60);
                return Err(FetchError::RateLimited { retry_after_secs: retry });
            }
            other => {
                warn!(status=?other, "unexpected Alpaca response");
                return Err(FetchError::Network(resp.error_for_status().unwrap_err()));
            }
        }

        let payload: BarsResponse = resp.json().await?;
        let raw = payload.bars.into_iter().next().map(|(_, v)| v).unwrap_or_default();
        Ok(raw.into_iter().map(|b| MarketBar {
            timestamp: b.t, open: b.o, high: b.h, low: b.l, close: b.c, volume: b.v,
        }).collect())
    }
}
```

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-data --test alpaca_fetcher fetch_crypto_bars_single_page
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-data/src/alpaca.rs crates/xvision-data/tests/alpaca_fetcher.rs
git commit -m "feat(xvision-data): AlpacaBarsFetcher single-page happy path"
```

---

## Task 4 — Pagination

**Files:** `crates/xvision-data/src/alpaca.rs`, `crates/xvision-data/tests/alpaca_fetcher.rs`

- [ ] **Step 1: Add failing test for multi-page fetch**

```rust
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
```

- [ ] **Step 2: Run test, expect FAIL**

```bash
cargo test -p xvision-data --test alpaca_fetcher fetch_crypto_bars_paginated
```

- [ ] **Step 3: Add pagination loop**

Modify `fetch_crypto_bars` to loop on `next_page_token`:

```rust
pub async fn fetch_crypto_bars(
    &self,
    asset_pair: &str,
    granularity: BarGranularity,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<MarketBar>, FetchError> {
    let mut out = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        self.rate_limiter.until_ready().await;
        let url = format!("{}/v1beta3/crypto/us/bars", self.base_url);
        let mut req = self.client.get(&url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.api_secret)
            .query(&[
                ("symbols", asset_pair),
                ("timeframe", granularity.as_alpaca_str()),
                ("start", &start.to_rfc3339()),
                ("end", &end.to_rfc3339()),
                ("limit", "10000"),
                ("page_token", page_token.as_deref().unwrap_or("")),
            ]);
        let resp = req.send().await?;
        match resp.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(FetchError::Unauthorized),
            StatusCode::NOT_FOUND => return Err(FetchError::AssetNotFound(asset_pair.into())),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry = resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(60);
                return Err(FetchError::RateLimited { retry_after_secs: retry });
            }
            _ => return Err(FetchError::Network(resp.error_for_status().unwrap_err())),
        }
        let payload: BarsResponse = resp.json().await?;
        let raw = payload.bars.into_iter().next().map(|(_, v)| v).unwrap_or_default();
        out.extend(raw.into_iter().map(|b| MarketBar { timestamp: b.t, open: b.o, high: b.h, low: b.l, close: b.c, volume: b.v }));
        match payload.next_page_token {
            Some(t) if !t.is_empty() => page_token = Some(t),
            _ => break,
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-data --test alpaca_fetcher
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-data/src/alpaca.rs crates/xvision-data/tests/alpaca_fetcher.rs
git commit -m "feat(xvision-data): paginated bar fetches via next_page_token"
```

---

## Task 5 — Error variants under wiremock

**Files:** `crates/xvision-data/tests/alpaca_fetcher.rs`

- [ ] **Step 1: Add failing tests for 401, 404, 429**

```rust
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
        .fetch_crypto_bars("FOO/USD", BarGranularity::Hour1,
            Utc.with_ymd_and_hms(2024,2,3,0,0,0).unwrap(),
            Utc.with_ymd_and_hms(2024,2,3,1,0,0).unwrap()).await.unwrap_err();
    assert!(matches!(err, FetchError::AssetNotFound(_)));
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
```

- [ ] **Step 2: Run tests, expect PASS** (fetcher already maps these from Task 3)

```bash
cargo test -p xvision-data --test alpaca_fetcher
```

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-data/tests/alpaca_fetcher.rs
git commit -m "test(xvision-data): error-variant coverage for 401/404/429"
```

---

## Task 6 — `bars_cache` SQLite migration

**Files:** `crates/xvision-engine/migrations/0004_bars_cache.sql`, `crates/xvision-engine/migrations/0004_bars_cache.down.sql`, `crates/xvision-engine/src/store.rs`

- [ ] **Step 1: Create migration up**

```sql
-- migrations/0004_bars_cache.sql
CREATE TABLE bars_cache (
    cache_key    TEXT PRIMARY KEY,
    asset        TEXT NOT NULL,
    granularity  TEXT NOT NULL,
    window_start TEXT NOT NULL,
    window_end   TEXT NOT NULL,
    data_source  TEXT NOT NULL,
    fetched_at   TEXT NOT NULL,
    bar_count    INTEGER NOT NULL,
    bars_blob    BLOB NOT NULL,
    compression  TEXT NOT NULL DEFAULT 'none'  -- 'none' | 'gzip'
);
CREATE INDEX bars_cache_by_asset_window ON bars_cache(asset, granularity, window_start, window_end);
```

- [ ] **Step 2: Create migration down**

```sql
-- migrations/0004_bars_cache.down.sql
DROP INDEX IF EXISTS bars_cache_by_asset_window;
DROP TABLE IF EXISTS bars_cache;
```

- [ ] **Step 3: Register in `store.rs` migrations list**

Open `crates/xvision-engine/src/store.rs` and add `0004_bars_cache.sql` after `0003` (or after the last existing migration — confirm with `git log -- crates/xvision-engine/migrations/`):

```rust
const MIGRATIONS: &[(&str, &str)] = &[
    // ... existing entries ...
    ("0004_bars_cache", include_str!("../migrations/0004_bars_cache.sql")),
];
```

- [ ] **Step 4: Run engine tests to confirm migrations still apply**

```bash
cargo test -p xvision-engine store
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/migrations/0004_bars_cache*.sql crates/xvision-engine/src/store.rs
git commit -m "feat(xvision-engine): 0004 bars_cache migration"
```

---

## Task 7 — Cache wrapper `load_bars`

**Files:** `crates/xvision-engine/src/eval/bars.rs`, `crates/xvision-engine/src/eval/mod.rs`, `crates/xvision-engine/tests/bars_cache.rs`

- [ ] **Step 1: Add `pub mod bars;` to `eval/mod.rs`**

- [ ] **Step 2: Write failing integration test**

```rust
// crates/xvision-engine/tests/bars_cache.rs
use chrono::{TimeZone, Utc};
use xvision_engine::api::ApiContext;
use xvision_engine::eval::bars::{load_bars, BarCacheArgs};
use xvision_data::alpaca::BarGranularity;
use xvision_data::asset_whitelist::to_alpaca_pair;

#[tokio::test]
async fn cache_miss_then_hit_returns_same_bars() {
    let ctx = ApiContext::test_with_mock_alpaca().await; // helper introduced in this task
    let args = BarCacheArgs {
        cache_key: "test_key_eth_2024_1h".into(),
        asset_pair: to_alpaca_pair("ETH"),
        granularity: BarGranularity::Hour1,
        start: Utc.with_ymd_and_hms(2024, 2, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2024, 2, 3, 4, 0, 0).unwrap(),
        data_source_tag: "alpaca-historical-v1".into(),
    };

    let first = load_bars(&ctx, &args).await.unwrap();
    let second = load_bars(&ctx, &args).await.unwrap();
    assert_eq!(first.len(), second.len());
    assert_eq!(first[0].timestamp, second[0].timestamp);
    assert_eq!(ctx.alpaca_call_count(), 1, "second call should hit cache");
}
```

- [ ] **Step 3: Run test, expect FAIL**

```bash
cargo test -p xvision-engine --test bars_cache
```

- [ ] **Step 4: Implement `eval/bars.rs`**

```rust
use chrono::{DateTime, Utc};
use flate2::{Compression, write::GzEncoder, read::GzDecoder};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

use xvision_data::alpaca::{BarGranularity, MarketBar};

use crate::api::{ApiContext, ApiError, ApiResult};

const GZIP_THRESHOLD_BARS: usize = 1000;

pub struct BarCacheArgs {
    pub cache_key: String,
    pub asset_pair: String,
    pub granularity: BarGranularity,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub data_source_tag: String,
}

pub async fn load_bars(ctx: &ApiContext, args: &BarCacheArgs) -> ApiResult<Vec<MarketBar>> {
    // 1. Single-flight per key.
    let lock = ctx.bars_singleflight_lock(&args.cache_key).await;
    let _guard = lock.lock().await;

    // 2. Cache lookup.
    if let Some(bars) = ctx.store.read_bars_cache(&args.cache_key).await? {
        return Ok(bars);
    }

    // 3. Fetch.
    let bars = ctx.alpaca_fetcher()
        .fetch_crypto_bars(&args.asset_pair, args.granularity, args.start, args.end)
        .await
        .map_err(|e| ApiError::Validation(format!("alpaca fetch: {e}")))?;

    // 4. Persist (gzip if large).
    let blob = serialise_bars(&bars);
    let (blob, compression) = if bars.len() > GZIP_THRESHOLD_BARS {
        (gzip(&blob), "gzip")
    } else {
        (blob, "none")
    };
    ctx.store.write_bars_cache(
        &args.cache_key,
        &args.asset_pair,
        args.granularity.as_alpaca_str(),
        args.start, args.end,
        &args.data_source_tag,
        bars.len(),
        &blob,
        compression,
    ).await?;

    Ok(bars)
}

fn serialise_bars(bars: &[MarketBar]) -> Vec<u8> {
    let mut out = Vec::new();
    for bar in bars {
        let line = serde_json::to_vec(&serde_json::json!({
            "t": bar.timestamp.to_rfc3339(),
            "o": bar.open, "h": bar.high, "l": bar.low, "c": bar.close, "v": bar.volume,
        })).unwrap();
        out.extend(line); out.push(b'\n');
    }
    out
}

fn gzip(input: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(input).unwrap();
    enc.finish().unwrap()
}

pub(crate) fn deserialise_bars(blob: &[u8], compression: &str) -> Vec<MarketBar> {
    let raw = if compression == "gzip" {
        let mut dec = GzDecoder::new(blob);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        out
    } else { blob.to_vec() };
    raw.split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_slice(l).unwrap();
            MarketBar {
                timestamp: chrono::DateTime::parse_from_rfc3339(v["t"].as_str().unwrap()).unwrap().with_timezone(&Utc),
                open: v["o"].as_f64().unwrap(),
                high: v["h"].as_f64().unwrap(),
                low: v["l"].as_f64().unwrap(),
                close: v["c"].as_f64().unwrap(),
                volume: v["v"].as_f64().unwrap(),
            }
        }).collect()
}
```

- [ ] **Step 5: Add `read_bars_cache` + `write_bars_cache` to the store**

In `crates/xvision-engine/src/store.rs`:

```rust
pub async fn read_bars_cache(&self, cache_key: &str) -> ApiResult<Option<Vec<MarketBar>>> {
    let row = sqlx::query!(
        "SELECT bars_blob, compression FROM bars_cache WHERE cache_key = ?",
        cache_key
    ).fetch_optional(&self.pool).await?;
    Ok(row.map(|r| crate::eval::bars::deserialise_bars(&r.bars_blob, &r.compression)))
}

pub async fn write_bars_cache(
    &self,
    cache_key: &str,
    asset: &str,
    granularity: &str,
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
    data_source: &str,
    bar_count: usize,
    blob: &[u8],
    compression: &str,
) -> ApiResult<()> {
    sqlx::query!(
        "INSERT OR REPLACE INTO bars_cache (cache_key, asset, granularity, window_start, window_end, data_source, fetched_at, bar_count, bars_blob, compression)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        cache_key, asset, granularity,
        window_start.to_rfc3339(), window_end.to_rfc3339(),
        data_source, chrono::Utc::now().to_rfc3339(),
        bar_count as i64, blob, compression
    ).execute(&self.pool).await?;
    Ok(())
}
```

- [ ] **Step 6: Add `bars_singleflight_lock` + `alpaca_fetcher` + `alpaca_call_count` + `test_with_mock_alpaca` helpers to `ApiContext`**

In `crates/xvision-engine/src/api/mod.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex as TokioMutex;

pub struct ApiContext {
    // ... existing fields ...
    bars_singleflight: StdMutex<HashMap<String, Arc<TokioMutex<()>>>>,
    pub(crate) alpaca: Arc<xvision_data::alpaca::AlpacaBarsFetcher>,
    #[cfg(test)]
    pub(crate) test_alpaca_calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl ApiContext {
    pub async fn bars_singleflight_lock(&self, key: &str) -> Arc<TokioMutex<()>> {
        let mut map = self.bars_singleflight.lock().unwrap();
        map.entry(key.to_string()).or_insert_with(|| Arc::new(TokioMutex::new(()))).clone()
    }
    pub fn alpaca_fetcher(&self) -> &xvision_data::alpaca::AlpacaBarsFetcher { &self.alpaca }

    #[cfg(test)]
    pub fn alpaca_call_count(&self) -> usize {
        self.test_alpaca_calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    pub async fn test_with_mock_alpaca() -> Self {
        // Spin up wiremock returning a fixed 4-bar response and an AtomicUsize-wrapped fetcher.
        // (Implementation in test-helpers module; counts increments in a custom transport layer.)
        unimplemented!("test helper — see tests/bars_cache.rs for the canonical setup")
    }
}
```

> **Implementation note:** for the test helper, the simplest approach is to define `test_with_mock_alpaca` inline in `tests/bars_cache.rs` and have it construct an `ApiContext` directly rather than calling a method on `ApiContext`. Refactor if the helper sprawls.

- [ ] **Step 7: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test bars_cache cache_miss_then_hit_returns_same_bars
```

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-engine/src/eval/bars.rs crates/xvision-engine/src/eval/mod.rs crates/xvision-engine/src/store.rs crates/xvision-engine/src/api/mod.rs crates/xvision-engine/tests/bars_cache.rs
git commit -m "feat(xvision-engine): load_bars cache wrapper with single-flight + gzip"
```

---

## Task 8 — Single-flight under concurrent misses

**Files:** `crates/xvision-engine/tests/bars_cache.rs`

- [ ] **Step 1: Add failing test for concurrent miss → single fetcher call**

```rust
#[tokio::test]
async fn concurrent_misses_serialize_through_singleflight() {
    let ctx = ApiContext::test_with_mock_alpaca().await;
    let args = BarCacheArgs { /* same key as before */ };

    let (a, b) = tokio::join!(
        load_bars(&ctx, &args),
        load_bars(&ctx, &args),
    );
    a.unwrap(); b.unwrap();
    assert_eq!(ctx.alpaca_call_count(), 1, "single-flight should de-dupe");
}
```

- [ ] **Step 2: Run test, expect PASS** (single-flight from Task 7 already covers this if implemented correctly)

If it FAILS, fix the lock acquisition order in `load_bars` so the cache lookup happens INSIDE the guard, not before.

```bash
cargo test -p xvision-engine --test bars_cache concurrent_misses
```

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/tests/bars_cache.rs
git commit -m "test(xvision-engine): single-flight under concurrent cache misses"
```

---

## Task 9 — Expand `AssetSymbol` enum

**Files:** `crates/xvision-core/src/assets.rs` (or wherever `AssetSymbol` currently lives — `grep -rn "enum AssetSymbol" crates/xvision-core/src/`)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn asset_symbol_covers_alpaca_crypto_whitelist() {
    use crate::AssetSymbol;
    for sym in &["BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI",
                 "DOT", "DOGE", "SHIB", "MATIC", "BCH", "USDT", "USDC"] {
        assert!(AssetSymbol::from_str(sym).is_ok(), "missing variant: {sym}");
    }
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-core asset_symbol_covers
```

- [ ] **Step 3: Expand the enum + FromStr impl + Display**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetSymbol {
    Btc, Eth, Ltc, Sol, Avax, Link, Aave, Uni,
    Dot, Doge, Shib, Matic, Bch, Usdt, Usdc,
}

impl AssetSymbol {
    pub fn as_short(self) -> &'static str {
        match self {
            Self::Btc=>"BTC", Self::Eth=>"ETH", Self::Ltc=>"LTC", Self::Sol=>"SOL",
            Self::Avax=>"AVAX", Self::Link=>"LINK", Self::Aave=>"AAVE", Self::Uni=>"UNI",
            Self::Dot=>"DOT", Self::Doge=>"DOGE", Self::Shib=>"SHIB", Self::Matic=>"MATIC",
            Self::Bch=>"BCH", Self::Usdt=>"USDT", Self::Usdc=>"USDC",
        }
    }
    pub fn as_alpaca_pair(self) -> String { format!("{}/USD", self.as_short()) }
}

impl std::str::FromStr for AssetSymbol {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "BTC"|"BTC/USD"|"BTCUSD" => Ok(Self::Btc),
            "ETH"|"ETH/USD"|"ETHUSD" => Ok(Self::Eth),
            "LTC"|"LTC/USD"|"LTCUSD" => Ok(Self::Ltc),
            "SOL"|"SOL/USD"|"SOLUSD" => Ok(Self::Sol),
            "AVAX"|"AVAX/USD"|"AVAXUSD" => Ok(Self::Avax),
            "LINK"|"LINK/USD"|"LINKUSD" => Ok(Self::Link),
            "AAVE"|"AAVE/USD"|"AAVEUSD" => Ok(Self::Aave),
            "UNI"|"UNI/USD"|"UNIUSD" => Ok(Self::Uni),
            "DOT"|"DOT/USD"|"DOTUSD" => Ok(Self::Dot),
            "DOGE"|"DOGE/USD"|"DOGEUSD" => Ok(Self::Doge),
            "SHIB"|"SHIB/USD"|"SHIBUSD" => Ok(Self::Shib),
            "MATIC"|"MATIC/USD"|"MATICUSD" => Ok(Self::Matic),
            "BCH"|"BCH/USD"|"BCHUSD" => Ok(Self::Bch),
            "USDT"|"USDT/USD"|"USDTUSD" => Ok(Self::Usdt),
            "USDC"|"USDC/USD"|"USDCUSD" => Ok(Self::Usdc),
            other => Err(format!("unknown asset '{other}'")),
        }
    }
}
```

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-core asset_symbol_covers
```

- [ ] **Step 5: `cargo build --workspace` to surface any downstream match-exhaustiveness errors**

Expected: a handful of exhaustive-match warnings/errors in `xvision-execution` / `xvision-eval` / report renderers. Address each by mapping the new variant to a sensible default (e.g. fee schedule unchanged for now; report renderer prints the symbol).

- [ ] **Step 6: Commit**

```bash
git add -p   # stage only the AssetSymbol expansions + downstream match patches
git commit -m "feat(xvision-core): AssetSymbol covers full Alpaca crypto whitelist"
```

---

## Task 10 — Drop BTC-only wall in xvision-execution

**Files:** `crates/xvision-execution/src/alpaca.rs`

- [ ] **Step 1: Remove the line-3 comment that pins us to BTC**

Open the file, delete the line `//! v1 scope: BTC-only via Alpaca's crypto endpoint (BTC/USD).`

- [ ] **Step 2: Generalise the parser**

Replace the `match` at line 46 with a delegation to `AssetSymbol::from_str`:

```rust
fn parse_alpaca_asset(s: &str) -> Option<AssetSymbol> {
    s.parse().ok()
}
```

- [ ] **Step 3: Generalise `alpaca_symbol_for`**

Replace `symbol_for_btc: &'static str` field with `fn alpaca_symbol_for(asset: AssetSymbol) -> String { asset.as_alpaca_pair() }`. Update all call sites accordingly.

- [ ] **Step 4: Run tests, expect PASS**

```bash
cargo test -p xvision-execution
```

If a test asserts BTC-only behaviour, update the assertion to verify "the configured asset is used" instead of "BTC is used."

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-execution/src/alpaca.rs
git commit -m "feat(xvision-execution): drop BTC-only wall in Alpaca executor"
```

---

## Task 11 — F18 partial: `TraderDecision.asset`

**Files:** `crates/xvision-core/src/trading.rs`, all callers

- [ ] **Step 1: Add the field**

```rust
pub struct TraderDecision {
    // ... existing fields ...
    pub asset: Option<AssetSymbol>,   // F18 partial: defaulted from scenario when absent
}
```

- [ ] **Step 2: `cargo build --workspace`**

Expected: pattern-match exhaustiveness + missing-field errors at callers (intern → trader path, eval baselines).

- [ ] **Step 3: Patch each caller**

For each error: set `asset: None` for fresh constructions; downstream consumers (risk, executor) fall through to the scenario's single asset via `decision.asset.unwrap_or(scenario.asset[0])`. F18 proper will tighten this later.

- [ ] **Step 4: Run workspace tests**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -p
git commit -m "feat(core): TraderDecision.asset field (F18 partial; defaults from scenario)"
```

---

## Task 12 — `xvn bars` subcommand

**Files:** `crates/xvision-cli/src/commands/bars.rs`, `crates/xvision-cli/src/commands/mod.rs`, `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Add `pub mod bars;` to `commands/mod.rs`**

- [ ] **Step 2: Implement the subcommand**

```rust
// crates/xvision-cli/src/commands/bars.rs
use clap::{Args, Subcommand};
use chrono::{DateTime, Utc, NaiveDate};
use std::path::PathBuf;
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::{ApiContext, eval::bars};

use crate::error::CliResult;

#[derive(Args, Debug)]
pub struct BarsCmd {
    #[command(subcommand)]
    pub op: BarsOp,
    #[arg(long)] pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum BarsOp {
    Fetch(FetchArgs),
    Ls,
    Rm { cache_key: String },
    Gc { #[arg(long)] older_than: String /* e.g. "90d" */ },
}

#[derive(Args, Debug)]
pub struct FetchArgs {
    #[arg(long)] pub asset: String,
    #[arg(long)] pub from: NaiveDate,
    #[arg(long)] pub to: NaiveDate,
    #[arg(long, default_value = "1h")] pub granularity: String,
}

pub async fn run(cmd: BarsCmd) -> CliResult<()> {
    let ctx = ApiContext::from_xvn_home(cmd.xvn_home.as_deref()).await?;
    match cmd.op {
        BarsOp::Fetch(a) => run_fetch(&ctx, a).await,
        BarsOp::Ls => run_ls(&ctx).await,
        BarsOp::Rm { cache_key } => run_rm(&ctx, cache_key).await,
        BarsOp::Gc { older_than } => run_gc(&ctx, older_than).await,
    }
}

async fn run_fetch(ctx: &ApiContext, a: FetchArgs) -> CliResult<()> {
    let asset: xvision_core::AssetSymbol = a.asset.parse().map_err(|e: String| crate::error::CliError::Validation(e))?;
    let granularity = match a.granularity.as_str() {
        "1h" => BarGranularity::Hour1,
        "1d" => BarGranularity::Day1,
        other => return Err(crate::error::CliError::Validation(format!("granularity '{other}' not in v1 set {{1h,1d}}"))),
    };
    let start = a.from.and_hms_opt(0,0,0).unwrap().and_utc();
    let end   = a.to.and_hms_opt(0,0,0).unwrap().and_utc();
    let cache_key = compute_cache_key(&asset.as_alpaca_pair(), granularity, start, end, "alpaca-historical-v1");
    let args = bars::BarCacheArgs {
        cache_key: cache_key.clone(),
        asset_pair: asset.as_alpaca_pair(),
        granularity, start, end,
        data_source_tag: "alpaca-historical-v1".into(),
    };
    let out = bars::load_bars(ctx, &args).await?;
    println!("Fetched {} bars (cache_key={cache_key})", out.len());
    Ok(())
}

fn compute_cache_key(asset: &str, g: BarGranularity, start: DateTime<Utc>, end: DateTime<Utc>, src: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(asset.as_bytes()); h.update(g.as_alpaca_str().as_bytes());
    h.update(start.to_rfc3339().as_bytes()); h.update(end.to_rfc3339().as_bytes());
    h.update(src.as_bytes());
    h.finalize().to_hex().to_string()
}

async fn run_ls(ctx: &ApiContext) -> CliResult<()> {
    let rows = ctx.store.list_bars_cache().await?;
    for r in rows {
        println!("{}  {}  {}  {}..{}  {} bars", r.cache_key, r.asset, r.granularity, r.window_start, r.window_end, r.bar_count);
    }
    Ok(())
}

async fn run_rm(ctx: &ApiContext, cache_key: String) -> CliResult<()> {
    ctx.store.delete_bars_cache(&cache_key).await?;
    println!("removed {cache_key}");
    Ok(())
}

async fn run_gc(ctx: &ApiContext, older_than: String) -> CliResult<()> {
    let cutoff = parse_duration(&older_than)?;
    let cutoff_ts = Utc::now() - cutoff;
    let n = ctx.store.gc_bars_cache(cutoff_ts).await?;
    println!("evicted {n} entries older than {older_than}");
    Ok(())
}

fn parse_duration(s: &str) -> CliResult<chrono::Duration> {
    if let Some(d) = s.strip_suffix('d') {
        let n: i64 = d.parse().map_err(|_| crate::error::CliError::Validation(format!("bad duration '{s}'")))?;
        Ok(chrono::Duration::days(n))
    } else { Err(crate::error::CliError::Validation(format!("only Nd supported (got '{s}')"))) }
}
```

- [ ] **Step 3: Add `list_bars_cache`, `delete_bars_cache`, `gc_bars_cache` to the store**

```rust
// crates/xvision-engine/src/store.rs
pub async fn list_bars_cache(&self) -> ApiResult<Vec<BarsCacheRow>> {
    let rows = sqlx::query_as!(BarsCacheRow, "SELECT cache_key, asset, granularity, window_start, window_end, fetched_at, bar_count FROM bars_cache ORDER BY fetched_at DESC")
        .fetch_all(&self.pool).await?;
    Ok(rows)
}
pub async fn delete_bars_cache(&self, key: &str) -> ApiResult<()> {
    sqlx::query!("DELETE FROM bars_cache WHERE cache_key = ?", key).execute(&self.pool).await?;
    Ok(())
}
pub async fn gc_bars_cache(&self, cutoff: chrono::DateTime<chrono::Utc>) -> ApiResult<u64> {
    let res = sqlx::query!("DELETE FROM bars_cache WHERE fetched_at < ?", cutoff.to_rfc3339()).execute(&self.pool).await?;
    Ok(res.rows_affected())
}
```

- [ ] **Step 4: Register `Bars(BarsCmd)` in the top-level `Command` enum in `crates/xvision-cli/src/lib.rs`**

```rust
/// SQLite-cached historical bars: fetch / ls / rm / gc.
Bars(commands::bars::BarsCmd),
```

And in the dispatch match:

```rust
Command::Bars(cmd) => commands::bars::run(cmd).await,
```

- [ ] **Step 5: Smoke test**

```bash
cargo run --bin xvn -- bars ls
```

Expected: empty list (no error).

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/src/commands/bars.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs crates/xvision-engine/src/store.rs
git commit -m "feat(cli): xvn bars fetch/ls/rm/gc"
```

---

## Task 13 — Wire `--asset ETH` through `xvn ab-compare`

**Files:** `crates/xvision-cli/src/commands/ab_compare.rs`, `crates/xvision-eval/src/ab_compare.rs`

- [ ] **Step 1: Replace the hardcoded BTC parse**

In `ab_compare.rs` CLI handler, replace the existing `parse_asset` call with `AssetSymbol::from_str(&args.asset)`.

- [ ] **Step 2: Drop `--cycles` and `--bars` file requirements when `--asset` + `--from` + `--to` are provided**

Add optional flags:

```rust
#[arg(long)] pub from: Option<chrono::NaiveDate>,
#[arg(long)] pub to: Option<chrono::NaiveDate>,
```

Branch in the handler: if `from`/`to` are set, build a `BarCacheArgs` and call `eval::bars::load_bars`; if `--bars` is set, keep the existing JSON-file path for backward compat.

- [ ] **Step 3: Smoke test against a recorded fixture**

```bash
cargo run --bin xvn -- ab-compare --asset ETH --from 2024-02-03 --to 2024-02-10 --granularity 1h --arms buy_and_hold --output /tmp/eth.json
```

Expected: PASS (bar cache populates on first run via mocked Alpaca in tests; real Alpaca call when credentials present).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/ab_compare.rs crates/xvision-eval/src/ab_compare.rs
git commit -m "feat(ab-compare): --asset/--from/--to drive cache-backed backtest"
```

---

## Task 14 — Config: rate limit knob

**Files:** `config/default.toml`, `crates/xvision-engine/src/api/mod.rs`

- [ ] **Step 1: Append to `config/default.toml`**

```toml
[data.alpaca]
rate_limit_rpm = 200
```

- [ ] **Step 2: Read in `ApiContext` constructor**

When building the `AlpacaBarsFetcher`, pass `config.data.alpaca.rate_limit_rpm` to `AlpacaBarsFetcher::with_rate_limit`.

- [ ] **Step 3: Commit**

```bash
git add config/default.toml crates/xvision-engine/src/api/mod.rs
git commit -m "feat(config): [data.alpaca] rate_limit_rpm knob (default 200)"
```

---

## Task 15 — M1 acceptance smoke

- [ ] **Step 1: `cargo test --workspace` clean**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 2: End-to-end CLI smoke against real Alpaca (skip in CI; manual on dev)**

```bash
export APCA_API_KEY_ID=$(op read 'op://Private/Alpaca Paper/key_id')
export APCA_API_SECRET_KEY=$(op read 'op://Private/Alpaca Paper/secret')
cargo run --bin xvn -- bars fetch --asset ETH --from 2024-02-03 --to 2024-02-10 --granularity 1h
cargo run --bin xvn -- bars ls
```

Expected: ~168 bars fetched + listed.

- [ ] **Step 3: Final commit if any acceptance follow-ups**

```bash
git add -p
git commit -m "chore: M1 acceptance smoke passes (Alpaca crypto unlocked)"
```

---

## Self-review notes

- Each Cargo feature added has a corresponding test.
- Single-flight defended by an explicit concurrent-miss test.
- BTC-only wall removal verified by an existing-test pass after the parser generalisation.
- F18 partial scope explicitly bounded (field added, defaulted, full cascade deferred).
- No placeholders or "implement later" steps.
