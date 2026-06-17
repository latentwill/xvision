//! Non-custodial x402 client: loads the agent's OWN key locally (never sent to
//! the platform), signs EIP-3009 authorizations, and drives the dashboard's
//! public x402 endpoint. The handshake (browse/buy/import) is added in Task 2.2.

use alloy::primitives::{Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use serde::Deserialize;
use xvision_marketplace::x402::{self, Authorization};

/// Resolve the buyer signer from the local environment only (`XVN_AGENT_PK`,
/// 0x-hex). Errors if unset — non-custodial: the operator provides the key
/// locally; the platform never holds it.
pub fn load_agent_signer() -> Result<PrivateKeySigner, String> {
    let pk = std::env::var("XVN_AGENT_PK")
        .map_err(|_| "XVN_AGENT_PK not set (non-custodial: provide the buyer key locally)".to_string())?;
    pk.trim()
        .parse::<PrivateKeySigner>()
        .map_err(|e| format!("XVN_AGENT_PK invalid: {e}"))
}

/// Dashboard base URL the MCP client talks to. Default localhost dev server.
pub fn api_base() -> String {
    std::env::var("XVN_MARKETPLACE_API").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
}

#[derive(Debug, Deserialize)]
pub struct AcceptsResp {
    pub accepts: Vec<PaymentReq>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentReq {
    pub network: String,
    pub asset: String,
    #[serde(rename = "payTo")]
    pub pay_to: String,
    #[serde(rename = "maxAmountRequired")]
    pub max_amount_required: String,
}

/// Build an EIP-3009 Authorization with a random nonce and validity window.
/// `from`/`to` = buyer/payTo addresses, `value` = USDC in 6-dp units,
/// `ttl_secs` = how long the authorization is valid for (e.g. 600s = 10 min).
pub fn build_authorization(
    from: Address,
    to: Address,
    value: U256,
    ttl_secs: u64,
) -> Result<Authorization, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // random 32-byte nonce
    let mut n = [0u8; 32];
    getrandom::getrandom(&mut n).map_err(|e| format!("rng: {e}"))?;
    Ok(Authorization {
        from,
        to,
        value,
        valid_after: U256::ZERO,
        valid_before: U256::from(now + ttl_secs),
        nonce: B256::from(n),
    })
}

/// Full non-custodial buy: GET 402 → sign locally → POST settle with X-PAYMENT.
///
/// Signature wire format: 65 bytes as `r(32) || s(32) || v(1)`, hex-encoded
/// with a `0x` prefix, matching the server's `decode_x_payment` in Phase 1.
pub async fn buy(listing_id: u64) -> Result<serde_json::Value, String> {
    let signer = load_agent_signer()?;
    let base = api_base();
    let http = reqwest::Client::new();

    // 1. discover requirements (server returns HTTP 402 with the accepts JSON body;
    //    reqwest does not error on 402, so .json() parses the body directly)
    let r = http
        .get(format!("{base}/api/marketplace/listings/{listing_id}/x402"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status();
    if status.as_u16() != 402 {
        let body = r.text().await.unwrap_or_default();
        return Err(format!(
            "expected HTTP 402 with payment requirements, got {status}: {body}"
        ));
    }
    let reqs: AcceptsResp = r.json().await.map_err(|e| e.to_string())?;
    let pr = reqs.accepts.into_iter().next().ok_or("no payment requirements")?;

    let chain_id: u64 = pr
        .network
        .strip_prefix("eip155:")
        .and_then(|s| s.parse().ok())
        .ok_or("bad network")?;
    let usdc: Address = pr.asset.parse().map_err(|_| "bad asset address")?;
    let pay_to: Address = pr.pay_to.parse().map_err(|_| "bad payTo address")?;
    let value: U256 = pr
        .max_amount_required
        .parse()
        .map_err(|_| "bad maxAmountRequired")?;

    // 2. sign locally — key never leaves this process
    let auth = build_authorization(signer.address(), pay_to, value, 600)?;
    let domain = x402::usdc_domain(chain_id, usdc);
    let parts = x402::sign_authorization(&signer, &auth, &domain).map_err(|e| e.to_string())?;

    // 3. assemble X-PAYMENT envelope
    //    Signature wire format: r(32) || s(32) || v(1), hex-encoded as 0x{r}{s}{v:02x}
    //    This matches the server's decode_x_payment: bytes[0..32]=r, bytes[32..64]=s, byte 64=v
    let sig_hex = format!(
        "0x{}{}{:02x}",
        alloy::hex::encode(parts.r.as_slice()),
        alloy::hex::encode(parts.s.as_slice()),
        parts.v
    );
    let envelope = serde_json::json!({
        "x402Version": 1,
        "scheme": "exact",
        "network": pr.network,
        "payload": {
            "authorization": {
                "from": format!("0x{:x}", auth.from),
                "to": format!("0x{:x}", auth.to),
                "value": auth.value.to_string(),
                "validAfter": auth.valid_after.to_string(),
                "validBefore": auth.valid_before.to_string(),
                "nonce": format!("0x{:x}", auth.nonce)
            },
            "signature": sig_hex
        }
    });
    use base64::Engine;
    let xpay = base64::engine::general_purpose::STANDARD.encode(envelope.to_string());

    // 4. POST settle
    let resp = http
        .post(format!("{base}/api/marketplace/facilitator/settle/{listing_id}"))
        .header("x-payment", xpay)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("settle {status}: {body}"));
    }
    Ok(body)
}

/// Browse all marketplace listings (read-only, no auth).
pub async fn browse() -> Result<serde_json::Value, String> {
    let http = reqwest::Client::new();
    http.get(format!("{}/api/marketplace/listings", api_base()))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Get a single marketplace listing by numeric id.
pub async fn get_listing(id: u64) -> Result<serde_json::Value, String> {
    let http = reqwest::Client::new();
    http.get(format!("{}/api/marketplace/listings/{id}", api_base()))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Import a purchased listing: verifies the on-chain license then installs
/// the strategy locally. POSTs `{ address: 0x<signer.address()> }`.
pub async fn import(id: u64) -> Result<serde_json::Value, String> {
    let signer = load_agent_signer()?;
    let http = reqwest::Client::new();
    http.post(format!("{}/api/marketplace/listings/{id}/import", api_base()))
        .json(&serde_json::json!({ "address": format!("0x{:x}", signer.address()) }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_signer_errors_without_env() {
        if std::env::var("XVN_AGENT_PK").is_err() {
            assert!(load_agent_signer().is_err());
        }
    }

    #[test]
    fn api_base_has_default() {
        std::env::remove_var("XVN_MARKETPLACE_API");
        assert!(api_base().starts_with("http"));
    }

    #[test]
    fn build_authorization_sets_value_and_expiry() {
        let from = Address::ZERO;
        let to = Address::ZERO;
        let auth = build_authorization(from, to, U256::from(49_000_000u64), 600).unwrap();
        assert_eq!(auth.value, U256::from(49_000_000u64));
        assert!(auth.valid_before > auth.valid_after);
    }
}
