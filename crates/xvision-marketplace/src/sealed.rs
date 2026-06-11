//! The [`SealedBundleCrypto`] port for sealed-tier bundle encryption.
//!
//! Sealed listings publish their payload encrypted: only a wallet that holds
//! the listing's license NFT can decrypt it. Encryption is delegated to a
//! crypto backend behind this trait so the actual mechanism (Lit Protocol,
//! operator escrow, a future on-chain KMS) is a swap, not a rewrite —
//! mirroring how [`crate::IpfsStore`] abstracts storage.
//!
//! ## Backends (this phase)
//!
//! - [`NoopSealed`] — default / test backend. `encrypt` always errors
//!   ("sealed crypto not configured"); `is_configured` is `false`.
//! - [`EscrowSealed`] — operator-escrow fallback. **Stub this phase:**
//!   no AEAD cipher is in the dependency tree and the deploy guardrails
//!   forbid adding a heavy crypto dep here, so `encrypt` returns
//!   [`MarketplaceError::NotImplemented`]. The trait shape is the
//!   deliverable; a real symmetric cipher lands in a later phase.
//! - [`LitChipotleClient`] — Lit Protocol v3 ("Chipotle") REST backend.
//!   `encrypt` POSTs to the Lit Action endpoint and returns the ciphertext.
//!   Decryption is NOT a Rust concern: it happens inside the custom Lit
//!   gate action (`contracts/lit-actions/sealed-gate.js`) pinned to
//!   [`SealedBundleCrypto::gate_action_cid`].
//!
//! ## Lit Chipotle wire-format caveat
//!
//! Lit v3 "Chipotle" is a REST API (`api.chipotle.litprotocol.com`), not an
//! SDK. The exact request/response JSON shapes are **not fully pinned** as of
//! 2026-06-11. The structs in this module ([`LitActionRequest`],
//! [`LitActionResponse`]) encode a best-effort shape and are marked
//! accordingly — **verify against the live Chipotle OpenAPI before any live
//! wiring.** This phase ships the trait, a compiling+tested HTTP client
//! (against a mock server), and the gate action JS; the live wiring is a
//! later phase.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MarketplaceError;

/// Default Lit Chipotle REST API base (overridable for tests).
const DEFAULT_LIT_API: &str = "https://api.chipotle.litprotocol.com";
/// HTTP timeout for Lit Action requests. A Lit Action may run up to 15min
/// server-side, but the *encrypt* path is a short call; without a timeout a
/// hung connection would block an `encrypt` future forever. Matches the
/// Pinata driver's robustness bound.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Encrypts sealed-bundle payloads for license-gated decryption.
///
/// Object-safe (`async_trait`, like [`crate::IpfsStore`]) so call sites hold a
/// `Box<dyn SealedBundleCrypto>` / `Arc<dyn …>` and never bind to a concrete
/// backend.
#[async_trait]
pub trait SealedBundleCrypto: Send + Sync {
    /// Encrypt `plaintext`, returning the opaque ciphertext blob to publish
    /// (stored on IPFS, decrypted later by the gate action).
    async fn encrypt(&self, plaintext: &[u8]) -> Result<String, MarketplaceError>;

    /// IPFS CID of the immutable gate (decrypt) Lit Action that license
    /// holders invoke. Empty when the backend has no gate (e.g. escrow).
    fn gate_action_cid(&self) -> &str;

    /// Whether the backend is configured (does not validate credentials).
    fn is_configured(&self) -> bool;
}

/// Default backend: never encrypts. Used as the dormant default and in tests
/// so a sealed-publish path is explicit about being unconfigured.
#[derive(Debug, Default, Clone)]
pub struct NoopSealed;

#[async_trait]
impl SealedBundleCrypto for NoopSealed {
    async fn encrypt(&self, _plaintext: &[u8]) -> Result<String, MarketplaceError> {
        Err(MarketplaceError::Sealed(
            "sealed crypto not configured".to_string(),
        ))
    }

    fn gate_action_cid(&self) -> &str {
        ""
    }

    fn is_configured(&self) -> bool {
        false
    }
}

/// Operator-escrow fallback backend.
///
/// **Stub this phase.** The intent is a symmetric AEAD (e.g. AES-256-GCM or
/// ChaCha20-Poly1305) under an operator-held key, so the operator can decrypt
/// on a buyer's behalf without Lit. But no AEAD crate is in the dependency
/// tree and the deploy guardrails forbid adding a heavy crypto dep in this
/// phase — so `encrypt` returns [`MarketplaceError::NotImplemented`]. The
/// trait shape matters more than the cipher here; a real implementation lands
/// in a later phase.
///
/// `is_configured` reports whether an operator key was supplied, so wiring can
/// be exercised even though `encrypt` is not yet live.
#[derive(Clone)]
pub struct EscrowSealed {
    /// Operator key material. Held but unused this phase (stub). Redacted in
    /// `Debug` so it cannot leak into logs.
    operator_key: Vec<u8>,
}

impl fmt::Debug for EscrowSealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EscrowSealed")
            .field("operator_key", &"<redacted>")
            .field("configured", &self.is_configured())
            .finish()
    }
}

impl EscrowSealed {
    /// Construct with operator key material (unused this phase).
    pub fn new(operator_key: impl Into<Vec<u8>>) -> Self {
        Self {
            operator_key: operator_key.into(),
        }
    }
}

#[async_trait]
impl SealedBundleCrypto for EscrowSealed {
    async fn encrypt(&self, _plaintext: &[u8]) -> Result<String, MarketplaceError> {
        // STUB: a real AEAD encrypt under `operator_key` lands in a later
        // phase once a reviewed cipher dep is approved for this crate.
        Err(MarketplaceError::NotImplemented(
            "EscrowSealed::encrypt (operator-escrow cipher is a later phase)",
        ))
    }

    fn gate_action_cid(&self) -> &str {
        // Escrow has no Lit gate action — the operator decrypts directly.
        ""
    }

    fn is_configured(&self) -> bool {
        !self.operator_key.is_empty()
    }
}

/// Request body POSTed to the Lit Chipotle `lit_action` endpoint to encrypt.
///
/// VERIFY AGAINST THE LIVE CHIPOTLE OPENAPI BEFORE LIVE USE — the exact field
/// names/shape are not pinned as of 2026-06-11. Best-effort: the encrypt Lit
/// Action takes the PKP id and the message to seal.
#[derive(Debug, Serialize)]
struct LitActionRequest<'a> {
    #[serde(rename = "pkpId")]
    pkp_id: &'a str,
    message: &'a str,
}

/// Successful response from the Lit Chipotle `lit_action` endpoint.
///
/// VERIFY AGAINST THE LIVE CHIPOTLE OPENAPI BEFORE LIVE USE — best-effort:
/// the action returns the ciphertext blob under `ciphertext`.
#[derive(Debug, Deserialize)]
struct LitActionResponse {
    ciphertext: String,
}

/// Lit Protocol v3 ("Chipotle") REST backend.
///
/// `encrypt` POSTs to `{api_base}/core/v1/lit_action` with an `X-Api-Key`
/// header and parses the returned `ciphertext`. The matching decrypt path is
/// the custom gate Lit Action (`contracts/lit-actions/sealed-gate.js`) pinned
/// to `gate_action_cid`, which verifies a SIWE-ish signature + an ERC-1155
/// `balanceOf` before calling `Lit.Actions.Decrypt` — it is not a Rust path.
pub struct LitChipotleClient {
    /// Lit REST API base (production by default; a mock URL in tests).
    api_base: String,
    /// `X-Api-Key` value. Redacted in `Debug`.
    api_key: String,
    /// PKP id whose key wraps the sealed payload.
    pkp_id: String,
    /// IPFS CID of the immutable decrypt gate action.
    gate_action_cid: String,
    client: reqwest::Client,
}

impl fmt::Debug for LitChipotleClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LitChipotleClient")
            .field("api_base", &self.api_base)
            .field("api_key", &"<redacted>")
            .field("pkp_id", &self.pkp_id)
            .field("gate_action_cid", &self.gate_action_cid)
            .finish()
    }
}

impl LitChipotleClient {
    /// Construct against the live Lit Chipotle API.
    pub fn new(
        api_key: impl Into<String>,
        pkp_id: impl Into<String>,
        gate_action_cid: impl Into<String>,
    ) -> Self {
        Self::with_api_base(DEFAULT_LIT_API, api_key, pkp_id, gate_action_cid)
    }

    /// Construct against a custom API base (tests target a mock HTTP server).
    /// `api_base` must NOT have a trailing slash.
    pub fn with_api_base(
        api_base: impl Into<String>,
        api_key: impl Into<String>,
        pkp_id: impl Into<String>,
        gate_action_cid: impl Into<String>,
    ) -> Self {
        let api_base = api_base.into();
        let api_base = if api_base.is_empty() {
            DEFAULT_LIT_API.to_string()
        } else {
            api_base
        };
        // A configured timeout keeps a hung Lit connection from blocking
        // forever. Fall back to the default client if the builder fails so
        // construction stays infallible (mirrors PinataDriver).
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_base: api_base.trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            pkp_id: pkp_id.into(),
            gate_action_cid: gate_action_cid.into(),
            client,
        }
    }
}

#[async_trait]
impl SealedBundleCrypto for LitChipotleClient {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<String, MarketplaceError> {
        if !self.is_configured() {
            return Err(MarketplaceError::Sealed(
                "sealed crypto not configured".to_string(),
            ));
        }

        // Lit encrypt takes a string message. Sealed payloads are arbitrary
        // bytes, so we send the UTF-8-lossy form; the gate action returns the
        // same string on decrypt. (Callers should hand UTF-8 payloads, e.g.
        // JSON manifests; binary sealing is a later-phase concern alongside
        // the pinned wire format.)
        let message = String::from_utf8_lossy(plaintext);
        let body = LitActionRequest {
            pkp_id: &self.pkp_id,
            message: &message,
        };

        let url = format!("{}/core/v1/lit_action", self.api_base);
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| MarketplaceError::Sealed(format!("lit_action request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MarketplaceError::Sealed(format!(
                "lit_action failed: HTTP {status}: {body}"
            )));
        }

        let parsed: LitActionResponse = resp
            .json()
            .await
            .map_err(|e| MarketplaceError::Sealed(format!("lit_action response decode failed: {e}")))?;
        Ok(parsed.ciphertext)
    }

    fn gate_action_cid(&self) -> &str {
        &self.gate_action_cid
    }

    fn is_configured(&self) -> bool {
        !self.api_key.is_empty() && !self.pkp_id.is_empty() && !self.gate_action_cid.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- NoopSealed ---------------------------------------------------------

    #[test]
    fn noop_is_not_configured() {
        assert!(!NoopSealed.is_configured());
        assert_eq!(NoopSealed.gate_action_cid(), "");
    }

    #[tokio::test]
    async fn noop_encrypt_errors_not_configured() {
        let err = NoopSealed.encrypt(b"secret").await.unwrap_err();
        match err {
            MarketplaceError::Sealed(msg) => {
                assert!(msg.contains("not configured"), "{msg}");
            }
            other => panic!("expected Sealed error, got {other:?}"),
        }
    }

    // --- EscrowSealed -------------------------------------------------------

    #[test]
    fn escrow_is_configured_reflects_key_presence() {
        assert!(EscrowSealed::new(vec![1, 2, 3]).is_configured());
        assert!(!EscrowSealed::new(Vec::new()).is_configured());
    }

    #[test]
    fn escrow_debug_redacts_key() {
        let dbg = format!("{:?}", EscrowSealed::new(b"super-secret-key".to_vec()));
        assert!(dbg.contains("<redacted>"), "{dbg}");
        assert!(!dbg.contains("super-secret-key"), "{dbg}");
    }

    #[tokio::test]
    async fn escrow_encrypt_is_unimplemented_stub() {
        let err = EscrowSealed::new(vec![1, 2, 3])
            .encrypt(b"secret")
            .await
            .unwrap_err();
        assert!(
            matches!(err, MarketplaceError::NotImplemented(_)),
            "escrow encrypt is a documented stub this phase: {err:?}"
        );
    }

    // --- LitChipotleClient --------------------------------------------------

    #[test]
    fn lit_is_configured_requires_key_pkp_and_gate_cid() {
        assert!(LitChipotleClient::new("key", "pkp", "cid").is_configured());
        assert!(!LitChipotleClient::new("", "pkp", "cid").is_configured());
        assert!(!LitChipotleClient::new("key", "", "cid").is_configured());
        assert!(!LitChipotleClient::new("key", "pkp", "").is_configured());
    }

    #[test]
    fn lit_gate_action_cid_is_returned() {
        let c = LitChipotleClient::new("key", "pkp", "bafygatecid");
        assert_eq!(c.gate_action_cid(), "bafygatecid");
    }

    #[test]
    fn lit_debug_redacts_api_key() {
        let dbg = format!(
            "{:?}",
            LitChipotleClient::new("super-secret-api-key", "pkp", "cid")
        );
        assert!(dbg.contains("<redacted>"), "{dbg}");
        assert!(!dbg.contains("super-secret-api-key"), "{dbg}");
    }

    #[tokio::test]
    async fn lit_encrypt_unconfigured_errors() {
        let c = LitChipotleClient::new("", "pkp", "cid");
        let err = c.encrypt(b"x").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Sealed(_)), "{err:?}");
    }

    #[tokio::test]
    async fn lit_encrypt_posts_and_returns_ciphertext() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/core/v1/lit_action")
            .match_header("x-api-key", "test-api-key")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"pkpId":"pkp-123","message":"plain-secret"}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ciphertext":"sealed-blob-abc"}"#)
            .create_async()
            .await;

        let c = LitChipotleClient::with_api_base(server.url(), "test-api-key", "pkp-123", "cid");
        let ct = c.encrypt(b"plain-secret").await.unwrap();
        assert_eq!(ct, "sealed-blob-abc");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn lit_encrypt_propagates_http_error() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/core/v1/lit_action")
            .with_status(403)
            .with_body("invalid api key")
            .create_async()
            .await;

        let c = LitChipotleClient::with_api_base(server.url(), "bad-key", "pkp", "cid");
        let err = c.encrypt(b"x").await.unwrap_err();
        match err {
            MarketplaceError::Sealed(msg) => assert!(msg.contains("403"), "{msg}"),
            other => panic!("expected Sealed error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn lit_encrypt_errors_on_malformed_response() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/core/v1/lit_action")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected":"shape"}"#)
            .create_async()
            .await;

        let c = LitChipotleClient::with_api_base(server.url(), "key", "pkp", "cid");
        let err = c.encrypt(b"x").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Sealed(_)), "{err:?}");
    }
}
