//! Nansen on-chain analytics HTTP client. POST + JSON body; auth header
//! `apikey`. Modeled on `alpaca.rs` (reqwest + governor rate limiting + typed
//! errors). Endpoint selection (v1 live vs v1beta1 historical) is the caller's
//! responsibility — this client is a thin signed-POST transport.

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
pub enum NansenError {
    #[error("nansen auth rejected (401/403)")]
    Unauthorized,
    #[error("nansen rate limited (429)")]
    RateLimited,
    #[error("nansen credits exhausted (402)")]
    CreditsExhausted,
    #[error("nansen http {0}")]
    Http(StatusCode),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(serde_json::Error),
    #[error("request timed out after {0}s")]
    Timeout(u64),
}

pub struct NansenClient {
    base_url: String,
    api_key: String,
    client: Client,
    rate_limiter: Arc<Limiter>,
}

impl NansenClient {
    /// `rpm` default 300 (Nansen). `base_url` like `https://api.nansen.ai`.
    pub fn new(base_url: String, api_key: String, rpm: u32) -> Self {
        let quota = Quota::per_minute(std::num::NonZeroU32::new(rpm.max(1)).unwrap_or(nonzero!(300u32)));
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

    /// Signed POST returning parsed JSON. `path` includes the API version
    /// segment, e.g. `/api/v1/smart-money/netflow`.
    pub async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, NansenError> {
        self.rate_limiter.until_ready().await;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(map_err)?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json().await.map_err(map_err)?),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(NansenError::Unauthorized),
            StatusCode::TOO_MANY_REQUESTS => Err(NansenError::RateLimited),
            StatusCode::PAYMENT_REQUIRED => Err(NansenError::CreditsExhausted),
            other => Err(NansenError::Http(other)),
        }
    }
}

fn map_err(err: reqwest::Error) -> NansenError {
    if err.is_timeout() {
        NansenError::Timeout(REQUEST_TIMEOUT_SECS)
    } else {
        NansenError::Network(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn netflow_sends_apikey_header_and_parses() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1/smart-money/netflow")
            .match_header("apikey", "secret-key")
            .with_status(200)
            .with_body(r#"{"data":[{"symbol":"BTC","netflow_usd":1234567.0}]}"#)
            .create_async()
            .await;

        let client = NansenClient::new(server.url(), "secret-key".into(), 300);
        let body = serde_json::json!({"chain":"ethereum","token_address":"0xabc"});
        let resp = client.post("/api/v1/smart-money/netflow", body).await.unwrap();

        assert_eq!(resp["data"][0]["symbol"], "BTC");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn http_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _m = server.mock("POST", "/x").with_status(429).create_async().await;
        let client = NansenClient::new(server.url(), "k".into(), 300);
        let err = client.post("/x", serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, NansenError::RateLimited));
    }
}
