//! Alpaca historical bars fetcher.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarGranularity {
    Minute1,
    Minute5,
    Minute15,
    Hour1,
    Hour4,
    Day1,
}

impl BarGranularity {
    pub fn as_alpaca_str(self) -> &'static str {
        match self {
            Self::Minute1 => "1Min",
            Self::Minute5 => "5Min",
            Self::Minute15 => "15Min",
            Self::Hour1 => "1Hour",
            Self::Hour4 => "4Hour",
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

    pub fn with_rate_limit(
        base_url: String,
        api_key: String,
        api_secret: String,
        rpm: u32,
    ) -> Self {
        let quota = Quota::per_minute(
            std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(200u32)),
        );
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
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            self.rate_limiter.until_ready().await;
            let url = format!("{}/v1beta3/crypto/us/bars", self.base_url);
            let req = self
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
                    ("page_token", page_token.as_deref().unwrap_or("")),
                ]);
            let resp = req.send().await?;
            match resp.status() {
                StatusCode::OK => {}
                StatusCode::UNAUTHORIZED => return Err(FetchError::Unauthorized),
                StatusCode::NOT_FOUND => return Err(FetchError::AssetNotFound(asset_pair.into())),
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    return Err(FetchError::RateLimited {
                        retry_after_secs: retry,
                    });
                }
                other => {
                    warn!(status=?other, "unexpected Alpaca response");
                    return Err(FetchError::Network(resp.error_for_status().unwrap_err()));
                }
            }
            let payload: BarsResponse = resp.json().await?;
            let raw = payload
                .bars
                .into_iter()
                .next()
                .map(|(_, v)| v)
                .unwrap_or_default();
            out.extend(raw.into_iter().map(|b| MarketBar {
                timestamp: b.t,
                open: b.o,
                high: b.h,
                low: b.l,
                close: b.c,
                volume: b.v,
            }));
            match payload.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }
        Ok(out)
    }
}
