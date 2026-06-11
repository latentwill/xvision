//! The [`IpfsStore`] port for manifest / sealed-bundle storage.
//!
//! Two drivers ship today: [`KuboStore`] (self-hosted go-ipfs node — the
//! preferred backend; no paid pinning service) and [`PinataDriver`]
//! (alternative hosted backstop tier). The `iroh` install-mesh driver lands
//! in V3 (direction doc §8.10); keeping storage behind this trait makes that
//! swap mechanical (nav-doc open question C7).

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::MarketplaceError;

/// Default pinning API base for the legacy [`PinataDriver`] path only — this
/// is the one place an external vendor (Pinata) is named, and it is only
/// reachable when an operator opts into the Pinata pinning backend by setting
/// `PINATA_JWT`. The default backend is self-hosted Kubo; nothing here is
/// dialed unless the alternative Pinata backend is explicitly configured.
const DEFAULT_PINATA_API: &str = "https://api.pinata.cloud";
/// Default public read gateway base (overridable for tests / dedicated
/// gateways). Vendor-neutral: `dweb.link` is the IPFS-canonical public
/// gateway, not a vendor product — we never bake an external vendor's gateway
/// into a self-hosted, open-source product as the default.
const DEFAULT_GATEWAY: &str = "https://dweb.link";
/// HTTP timeout for Pinata pin/gateway requests. Without it a hung connection
/// to Pinata or the gateway would block a `put`/`get` future forever.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

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
        // A configured timeout keeps a hung Pinata/gateway connection from
        // blocking forever. The builder only fails on TLS/system-resource
        // issues; fall back to the default client so construction stays
        // infallible (the timeout is a robustness bound, not a hard invariant).
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            jwt: jwt.into(),
            gateway: gateway.trim_end_matches('/').to_string(),
            api_base: api_base.into().trim_end_matches('/').to_string(),
            client,
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

/// Default local Kubo gateway when `gateway_url` is empty. Kubo's daemon
/// serves the read gateway on `:8080` next to the `:5001` RPC API by default.
const DEFAULT_KUBO_GATEWAY: &str = "http://127.0.0.1:8080";

/// Shape of a successful Kubo `POST /api/v0/add` response. Kubo returns
/// `Hash` (the CID) plus `Name`/`Size` we ignore.
#[derive(Debug, Deserialize)]
struct KuboAddResponse {
    #[serde(rename = "Hash")]
    hash: String,
}

/// Self-hosted Kubo (go-ipfs) backed [`IpfsStore`] — the preferred pin
/// backend (no paid pinning service).
///
/// `put` adds + pins bytes through the Kubo RPC `POST /api/v0/add` multipart
/// endpoint (`pin=true&cid-version=1`; the RPC API requires POST and is
/// unauthenticated by default). `get` fetches by CID from the configured
/// HTTP gateway (default the node's own `:8080` gateway).
pub struct KuboStore {
    /// Kubo RPC API base, e.g. `http://127.0.0.1:5001`. Empty → unconfigured.
    api_url: String,
    gateway: String,
    client: reqwest::Client,
}

impl KuboStore {
    /// Construct against a Kubo RPC API (`api_url`, e.g.
    /// `http://127.0.0.1:5001`) and HTTP gateway. An empty `gateway_url`
    /// falls back to [`DEFAULT_KUBO_GATEWAY`]. Trailing slashes are stripped.
    pub fn new(api_url: impl Into<String>, gateway_url: impl Into<String>) -> Self {
        let gateway = gateway_url.into();
        let gateway = if gateway.is_empty() {
            DEFAULT_KUBO_GATEWAY.to_string()
        } else {
            gateway
        };
        // Same robustness bound as PinataDriver: a timeout keeps a hung
        // node/gateway connection from blocking forever; fall back to the
        // default client so construction stays infallible.
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_url: api_url.into().trim_end_matches('/').to_string(),
            gateway: gateway.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    /// Whether an API URL is present (does not probe the node).
    pub fn is_configured(&self) -> bool {
        !self.api_url.is_empty()
    }
}

#[async_trait]
impl IpfsStore for KuboStore {
    async fn put(&self, bytes: &[u8]) -> Result<String, MarketplaceError> {
        if !self.is_configured() {
            return Err(MarketplaceError::NotConfigured("Kubo API URL is empty"));
        }

        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|e| MarketplaceError::Ipfs(e.to_string()))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        // `pin=true` keeps the block pinned across GC; `cid-version=1` yields
        // the modern base32 `bafy…` CIDs (matching what Pinata returns).
        let url = format!("{}/api/v0/add?pin=true&cid-version=1", self.api_url);
        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("kubo add request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MarketplaceError::Ipfs(format!(
                "kubo add failed: HTTP {status}: {body}"
            )));
        }

        let parsed: KuboAddResponse = resp
            .json()
            .await
            .map_err(|e| MarketplaceError::Ipfs(format!("kubo add response decode failed: {e}")))?;
        Ok(parsed.hash)
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
mod kubo_tests {
    use super::*;

    #[test]
    fn kubo_is_configured_reflects_api_url_presence() {
        assert!(KuboStore::new("http://127.0.0.1:5001", "").is_configured());
        assert!(!KuboStore::new("", "").is_configured());
    }

    #[test]
    fn kubo_defaults_and_strips_trailing_slashes() {
        let k = KuboStore::new("http://127.0.0.1:5001/", "");
        assert_eq!(k.gateway(), "http://127.0.0.1:8080");
        let k = KuboStore::new("http://127.0.0.1:5001", "http://gw.example/");
        assert_eq!(k.gateway(), "http://gw.example");
    }

    #[tokio::test]
    async fn kubo_put_without_api_url_is_not_configured() {
        let k = KuboStore::new("", "");
        let err = k.put(b"{}").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn kubo_put_adds_and_returns_cid() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v0/add")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("pin".into(), "true".into()),
                mockito::Matcher::UrlEncoded("cid-version".into(), "1".into()),
            ]))
            .match_body(mockito::Matcher::Regex("name=\"file\"".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"Name":"blob","Hash":"bafykubocid","Size":"17"}"#)
            .create_async()
            .await;

        let k = KuboStore::new(server.url(), "http://gw.unused");
        let cid = k.put(br#"{"hello":"world"}"#).await.unwrap();
        assert_eq!(cid, "bafykubocid");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn kubo_put_propagates_http_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/api/v0/add")
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_body("kubo exploded")
            .create_async()
            .await;

        let k = KuboStore::new(server.url(), "http://gw.unused");
        let err = k.put(b"{}").await.unwrap_err();
        match err {
            MarketplaceError::Ipfs(msg) => {
                assert!(msg.contains("500"), "expected 500 in: {msg}");
            }
            other => panic!("expected Ipfs error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn kubo_put_errors_on_malformed_response() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/api/v0/add")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected":"shape"}"#)
            .create_async()
            .await;

        let k = KuboStore::new(server.url(), "http://gw.unused");
        let err = k.put(b"{}").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Ipfs(_)), "{err:?}");
    }

    #[tokio::test]
    async fn kubo_get_round_trips_bytes() {
        let mut server = mockito::Server::new_async().await;
        let payload = br#"{"sealed":"bundle"}"#;
        let m = server
            .mock("GET", "/ipfs/bafykubocid")
            .with_status(200)
            .with_body(payload)
            .create_async()
            .await;

        // gateway points at the mock server; api_url unused for get.
        let k = KuboStore::new("http://api.unused", server.url());
        let got = k.get("bafykubocid").await.unwrap();
        assert_eq!(got, payload);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn kubo_get_propagates_gateway_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/ipfs/missing")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;

        let k = KuboStore::new("http://api.unused", server.url());
        let err = k.get("missing").await.unwrap_err();
        match err {
            MarketplaceError::Ipfs(msg) => assert!(msg.contains("404"), "{msg}"),
            other => panic!("expected Ipfs error, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_gateway_falls_back_to_vendor_neutral_default() {
        // The neutral default must be the IPFS-canonical public gateway, never
        // a vendor product (no pinata.cloud baked into a self-hosted product).
        let d = PinataDriver::new("jwt", "");
        assert_eq!(d.gateway(), "https://dweb.link");
        assert!(!d.gateway().contains("pinata"));
    }

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
