//! The [`IpfsStore`] port for manifest / sealed-bundle storage.
//!
//! V2 ships only [`PinataDriver`] (backstop tier). The `iroh` install-mesh
//! driver lands in V3 (direction doc §8.10); keeping storage behind this trait
//! makes that swap mechanical (nav-doc open question C7).

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::MarketplaceError;

/// Default Pinata pinning API base (overridable for tests).
const DEFAULT_PINATA_API: &str = "https://api.pinata.cloud";
/// Default public gateway base (overridable for tests / dedicated gateways).
const DEFAULT_GATEWAY: &str = "https://gateway.pinata.cloud";

/// Content-addressed storage for listing metadata and sealed bundles.
#[async_trait]
pub trait IpfsStore: Send + Sync {
    /// Pin `bytes`, returning the content id (e.g. `bafy…`).
    async fn put(&self, bytes: &[u8]) -> Result<String, MarketplaceError>;

    /// Fetch the bytes behind `cid`.
    async fn get(&self, cid: &str) -> Result<Vec<u8>, MarketplaceError>;
}

/// Shape of a successful `pinFileToIPFS` / `pinJSONToIPFS` response.
/// Pinata returns `IpfsHash` (the CID) plus size/timestamp we ignore.
#[derive(Debug, Deserialize)]
struct PinResponse {
    #[serde(rename = "IpfsHash")]
    ipfs_hash: String,
}

/// Pinata-backed [`IpfsStore`] (V2 backstop tier).
///
/// `put` pins arbitrary bytes through the `pinFileToIPFS` multipart endpoint
/// (works for both JSON manifests and sealed bundles), authenticating with the
/// JWT bearer token. `get` fetches by CID from the configured gateway.
pub struct PinataDriver {
    jwt: String,
    gateway: String,
    /// Pinata API base. The real API in production; a mock server URL in tests.
    api_base: String,
    client: reqwest::Client,
}

impl PinataDriver {
    /// Construct with a JWT and gateway, pointing at the live Pinata API.
    pub fn new(jwt: impl Into<String>, gateway: impl Into<String>) -> Self {
        Self::with_api_base(jwt, gateway, DEFAULT_PINATA_API)
    }

    /// Construct against a custom API base (used by tests to target a mock
    /// HTTP server). `api_base` and `gateway` must NOT have a trailing slash.
    pub fn with_api_base(
        jwt: impl Into<String>,
        gateway: impl Into<String>,
        api_base: impl Into<String>,
    ) -> Self {
        let gateway = gateway.into();
        let gateway = if gateway.is_empty() {
            DEFAULT_GATEWAY.to_string()
        } else {
            gateway
        };
        Self {
            jwt: jwt.into(),
            gateway: gateway.trim_end_matches('/').to_string(),
            api_base: api_base.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    /// Whether credentials are present (does not validate them).
    pub fn is_configured(&self) -> bool {
        !self.jwt.is_empty()
    }
}

#[async_trait]
impl IpfsStore for PinataDriver {
    async fn put(&self, bytes: &[u8]) -> Result<String, MarketplaceError> {
        if !self.is_configured() {
            return Err(MarketplaceError::NotConfigured("Pinata JWT is empty"));
        }

        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|e| MarketplaceError::Ipfs(e.to_string()))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let url = format!("{}/pinning/pinFileToIPFS", self.api_base);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.jwt)
            .multipart(form)
            .send()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("pin request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MarketplaceError::Ipfs(format!(
                "pin failed: HTTP {status}: {body}"
            )));
        }

        let parsed: PinResponse = resp
            .json()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("pin response decode failed: {e}")))?;
        Ok(parsed.ipfs_hash)
    }

    async fn get(&self, cid: &str) -> Result<Vec<u8>, MarketplaceError> {
        let url = format!("{}/ipfs/{cid}", self.gateway);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("gateway request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MarketplaceError::Ipfs(format!(
                "gateway fetch failed: HTTP {status}: {body}"
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("gateway body read failed: {e}")))?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_configured_reflects_jwt_presence() {
        assert!(PinataDriver::new("jwt-token", "https://gw").is_configured());
        assert!(!PinataDriver::new("", "https://gw").is_configured());
    }

    #[tokio::test]
    async fn put_without_jwt_is_not_configured() {
        let d = PinataDriver::new("", "https://gw");
        let err = d.put(b"{}").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn put_pins_and_returns_cid() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/pinning/pinFileToIPFS")
            .match_header("authorization", "Bearer test-jwt")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"IpfsHash":"bafytestcid","PinSize":12,"Timestamp":"now"}"#)
            .create_async()
            .await;

        let d = PinataDriver::with_api_base("test-jwt", "https://gw", server.url());
        let cid = d.put(br#"{"hello":"world"}"#).await.unwrap();
        assert_eq!(cid, "bafytestcid");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn put_propagates_http_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/pinning/pinFileToIPFS")
            .with_status(401)
            .with_body("invalid credentials")
            .create_async()
            .await;

        let d = PinataDriver::with_api_base("bad-jwt", "https://gw", server.url());
        let err = d.put(b"{}").await.unwrap_err();
        match err {
            MarketplaceError::Ipfs(msg) => {
                assert!(msg.contains("401"), "expected 401 in: {msg}");
            }
            other => panic!("expected Ipfs error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn put_errors_on_malformed_response() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/pinning/pinFileToIPFS")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected":"shape"}"#)
            .create_async()
            .await;

        let d = PinataDriver::with_api_base("jwt", "https://gw", server.url());
        let err = d.put(b"{}").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Ipfs(_)), "{err:?}");
    }

    #[tokio::test]
    async fn get_round_trips_bytes() {
        let mut server = mockito::Server::new_async().await;
        let payload = br#"{"sealed":"bundle"}"#;
        let m = server
            .mock("GET", "/ipfs/bafytestcid")
            .with_status(200)
            .with_body(payload)
            .create_async()
            .await;

        // gateway points at the mock server; api_base unused for get.
        let d = PinataDriver::with_api_base("jwt", server.url(), "https://api.unused");
        let got = d.get("bafytestcid").await.unwrap();
        assert_eq!(got, payload);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn get_propagates_gateway_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/ipfs/missing")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;

        let d = PinataDriver::with_api_base("jwt", server.url(), "https://api.unused");
        let err = d.get("missing").await.unwrap_err();
        match err {
            MarketplaceError::Ipfs(msg) => assert!(msg.contains("404"), "{msg}"),
            other => panic!("expected Ipfs error, got {other:?}"),
        }
    }
}
