//! Elfa crypto-social / KOL intelligence HTTP client. GET + query params; auth
//! header `x-elfa-api-key`. Modeled on `nansen.rs`. Forward-only (live runs
//! only) — the dispatch enforces that; this client is a thin GET transport.

use std::sync::Arc;
use std::time::Duration;

use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use reqwest::{Client, StatusCode};
use thiserror::Error;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

const REQUEST_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Error)]
pub enum ElfaError {
    #[error("elfa auth rejected (401/403)")]
    Unauthorized,
    #[error("elfa rate limited (429)")]
    RateLimited,
    #[error("elfa credits exhausted (402)")]
    CreditsExhausted,
    #[error("elfa http {0}")]
    Http(StatusCode),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(serde_json::Error),
    #[error("request timed out after {0}s")]
    Timeout(u64),
}

pub struct ElfaClient {
    base_url: String,
    api_key: String,
    client: Client,
    rate_limiter: Arc<Limiter>,
}

impl ElfaClient {
    /// `rpm` default 60 (Elfa). `base_url` like `https://api.elfa.ai`.
    pub fn new(base_url: String, api_key: String, rpm: u32) -> Self {
        let quota = Quota::per_minute(std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(60u32)));
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url,
            api_key,
            client,
            rate_limiter: Arc::new(RateLimiter::direct(quota)),
        }
    }

    /// Signed GET returning parsed JSON. `path` includes the API version
    /// segment, e.g. `/v2/data/top-mentions`. `query` is a slice of
    /// key-value pairs appended as URL query parameters.
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value, ElfaError> {
        self.rate_limiter.until_ready().await;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("x-elfa-api-key", &self.api_key)
            .query(query)
            .send()
            .await
            .map_err(map_err)?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json().await.map_err(map_err)?),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ElfaError::Unauthorized),
            StatusCode::TOO_MANY_REQUESTS => Err(ElfaError::RateLimited),
            StatusCode::PAYMENT_REQUIRED => Err(ElfaError::CreditsExhausted),
            other => Err(ElfaError::Http(other)),
        }
    }
}

fn map_err(err: reqwest::Error) -> ElfaError {
    if err.is_timeout() {
        ElfaError::Timeout(REQUEST_TIMEOUT_SECS)
    } else {
        ElfaError::Network(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn top_mentions_sends_header_and_query_and_parses() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v2/data/top-mentions")
            .match_header("x-elfa-api-key", "secret-key")
            .match_query(mockito::Matcher::UrlEncoded("ticker".into(), "BTC".into()))
            .with_status(200)
            .with_body(r#"{"data":[{"ticker":"BTC"}]}"#)
            .create_async()
            .await;

        let client = ElfaClient::new(server.url(), "secret-key".into(), 60);
        let resp = client
            .get("/v2/data/top-mentions", &[("ticker", "BTC")])
            .await
            .unwrap();
        assert_eq!(resp["data"][0]["ticker"], "BTC");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn http_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _m = server.mock("GET", "/x").with_status(429).create_async().await;
        let client = ElfaClient::new(server.url(), "k".into(), 60);
        let err = client.get("/x", &[]).await.unwrap_err();
        assert!(matches!(err, ElfaError::RateLimited));
    }
}
