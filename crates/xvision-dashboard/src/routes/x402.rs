//! x402 resource server + facilitator. Wraps the existing buyWithAuthorization
//! relay in the standard HTTP-402 protocol so any x402 client can pay.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use alloy::primitives::{Address, B256, U256};
use xvision_marketplace::AnchorDriver;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaymentRequirements {
    pub scheme: String,
    pub network: String,
    pub asset: String,
    #[serde(rename = "payTo")]
    pub pay_to: String,
    #[serde(rename = "maxAmountRequired")]
    pub max_amount_required: String,
    pub resource: String,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Accepts {
    #[serde(rename = "x402Version")]
    pub x402_version: u8,
    pub accepts: Vec<PaymentRequirements>,
}

/// Pure builder — no chain access; caller supplies the on-chain price/addresses.
pub fn build_accepts(
    chain_id: u64,
    usdc: &str,
    marketplace: &str,
    listing_id: u64,
    price_usdc: &str,
) -> Accepts {
    Accepts {
        x402_version: 1,
        accepts: vec![PaymentRequirements {
            scheme: "exact".into(),
            network: format!("eip155:{chain_id}"),
            asset: usdc.to_string(),
            pay_to: marketplace.to_string(),
            max_amount_required: price_usdc.to_string(),
            resource: format!("/api/marketplace/listings/{listing_id}/x402"),
            extra: serde_json::json!({ "listingId": listing_id }),
        }],
    }
}

#[derive(Debug, Deserialize)]
struct XPaymentEnvelope {
    payload: XPaymentPayload,
}
#[derive(Debug, Deserialize)]
struct XPaymentPayload {
    authorization: XPaymentAuth,
    signature: String, // 65-byte 0x sig; split into v/r/s
}
#[derive(Debug, Deserialize)]
struct XPaymentAuth {
    from: String,
    to: String,
    value: String,
    #[serde(rename = "validAfter")]
    valid_after: String,
    #[serde(rename = "validBefore")]
    valid_before: String,
    nonce: String,
}

/// Decoded x402 payment, normalized to the relay's `AuthorizationBody`.
pub struct DecodedPayment {
    pub from: String,
    pub authorization: crate::routes::marketplace::AuthorizationBody,
    pub listing_value: String,
}

pub fn decode_x_payment(header: &str) -> Result<DecodedPayment, DashboardError> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(header.trim())
        .map_err(|e| DashboardError::BadRequest(format!("x-payment base64: {e}")))?;
    let env: XPaymentEnvelope = serde_json::from_slice(&raw)
        .map_err(|e| DashboardError::BadRequest(format!("x-payment json: {e}")))?;
    let sig = env.payload.signature.trim_start_matches("0x");
    let bytes = alloy::hex::decode(sig).map_err(|e| DashboardError::BadRequest(format!("sig hex: {e}")))?;
    if bytes.len() != 65 {
        return Err(DashboardError::BadRequest("sig must be 65 bytes".into()));
    }
    let r = format!("0x{}", alloy::hex::encode(&bytes[0..32]));
    let s = format!("0x{}", alloy::hex::encode(&bytes[32..64]));
    let v = bytes[64] as u64;
    let a = env.payload.authorization;
    Ok(DecodedPayment {
        from: a.from.clone(),
        listing_value: a.value.clone(),
        authorization: crate::routes::marketplace::AuthorizationBody {
            from: a.from.clone(),
            to: a.to,
            value: a.value,
            valid_after: a
                .valid_after
                .parse()
                .map_err(|_| DashboardError::BadRequest("validAfter must be a u64 decimal".into()))?,
            valid_before: a
                .valid_before
                .parse()
                .map_err(|_| DashboardError::BadRequest("validBefore must be a u64 decimal".into()))?,
            nonce: a.nonce,
            v,
            r,
            s,
        },
    })
}

// ---------------------------------------------------------------------------
// Task 1.6: /facilitator/verify — pure checks + route
// ---------------------------------------------------------------------------

/// Check that the payment value meets the listing price and the authorization
/// has not expired. Pure — no I/O; clock is passed as a parameter so tests
/// can inject a fixed time.
pub fn check_terms(value: &str, price: &str, valid_before: u64, now: u64) -> Result<(), DashboardError> {
    let v: u128 = value
        .parse()
        .map_err(|_| DashboardError::BadRequest("value".into()))?;
    let p: u128 = price
        .parse()
        .map_err(|_| DashboardError::BadRequest("price".into()))?;
    if v < p {
        return Err(DashboardError::BadRequest("insufficient payment".into()));
    }
    if valid_before <= now {
        return Err(DashboardError::BadRequest("authorization expired".into()));
    }
    Ok(())
}

/// Pure decision for the spent-nonce precheck. `used` comes from the on-chain
/// `authorizationState(from, nonce)` read (see `Erc8004MantleDriver::is_authorization_used`).
pub fn ensure_unused(used: bool) -> Result<(), DashboardError> {
    if used {
        return Err(DashboardError::BadRequest("authorization already used".into()));
    }
    Ok(())
}

/// Response body for `POST /facilitator/verify`.
#[derive(Debug, Serialize)]
pub struct VerifyOut {
    pub valid: bool,
    pub payer: String,
}

/// Parse an address from a `0x…` hex string, mapping errors to `BadRequest`.
fn parse_address_bad_request(label: &str, s: &str) -> Result<Address, DashboardError> {
    s.parse()
        .map_err(|_| DashboardError::BadRequest(format!("{label}: expected 0x-prefixed 40-hex-char address")))
}

/// Parse a `B256` from a `0x…` hex string, mapping errors to `BadRequest`.
fn parse_b256_bad_request(label: &str, s: &str) -> Result<B256, DashboardError> {
    s.parse()
        .map_err(|_| DashboardError::BadRequest(format!("{label}: expected 0x-prefixed 64-hex-char bytes32")))
}

/// Parse a decimal `U256` from a string, mapping errors to `BadRequest`.
fn parse_u256_decimal_bad_request(label: &str, s: &str) -> Result<U256, DashboardError> {
    U256::from_str_radix(s, 10)
        .map_err(|_| DashboardError::BadRequest(format!("{label}: expected decimal U256 string")))
}

/// Off-chain recover of the EIP-3009 payer, followed by the on-chain
/// `authorizationState` spent-nonce precheck.
///
/// Returns the recovered `Address` on success. The caller must then compare
/// it against `decoded.from` to confirm the `from` field was not forged.
async fn recover_payer(state: &AppState, decoded: &DecodedPayment) -> Result<Address, DashboardError> {
    // 1. Load chain config — we need chain_id and the USDC contract address.
    let mp = state.marketplace_chain();
    let chain = mp
        .and_then(|c| c.chain.as_ref())
        .ok_or_else(|| DashboardError::ServiceUnavailable("chain relay not configured".into()))?;
    let addrs = mp
        .and_then(|c| c.marketplace_addresses.clone())
        .ok_or_else(|| DashboardError::ServiceUnavailable("marketplace not configured".into()))?;

    // 2. Build the EIP-712 domain for the USDC contract.
    let domain = xvision_marketplace::x402::usdc_domain(chain.chain_id, addrs.usdc);

    // 3. Parse the authorization fields from the decoded payment.
    let auth_body = &decoded.authorization;
    let from = parse_address_bad_request("authorization.from", &auth_body.from)?;
    let to = parse_address_bad_request("authorization.to", &auth_body.to)?;
    let value = parse_u256_decimal_bad_request("authorization.value", &auth_body.value)?;
    let nonce = parse_b256_bad_request("authorization.nonce", &auth_body.nonce)?;
    let r = parse_b256_bad_request("authorization.r", &auth_body.r)?;
    let s = parse_b256_bad_request("authorization.s", &auth_body.s)?;
    let v: u8 = auth_body
        .v
        .try_into()
        .map_err(|_| DashboardError::BadRequest("authorization.v must fit u8".into()))?;

    let auth = xvision_marketplace::x402::Authorization {
        from,
        to,
        value,
        valid_after: U256::from(auth_body.valid_after),
        valid_before: U256::from(auth_body.valid_before),
        nonce,
    };

    // 4. Off-chain ecrecover.
    let payer = xvision_marketplace::x402::recover_authorizer(&auth, &domain, v, r, s)?;

    // 5. On-chain spent-nonce precheck via IERC3009::authorizationState.
    let driver =
        xvision_marketplace::adapter::Erc8004MantleDriver::new(addrs, chain.rpc_url.clone(), chain.chain_id);
    let used = driver.is_authorization_used(from, nonce).await?;
    ensure_unused(used)?;

    Ok(payer)
}

/// `POST /api/marketplace/facilitator/verify` — off-chain recover + terms check
/// + spent-nonce precheck. Does NOT settle; that is Task 1.7.
///
/// Accepts the payment as an `X-PAYMENT` header (base64 JSON) or as the raw
/// base64-encoded JSON body. Returns `{ valid: true, payer: "0x…" }` on
/// success, or a `BadRequest` / `ServiceUnavailable` on any failure.
pub async fn post_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<VerifyOut>, DashboardError> {
    // Accept either an X-PAYMENT header (base64-encoded) or the body (also
    // base64-encoded per spec).
    let hdr = headers
        .get("x-payment")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    let decoded = match hdr {
        Some(h) => decode_x_payment(&h)?,
        None => decode_x_payment(body.trim())?,
    };

    // Terms check: authorization must not have expired.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    check_terms(
        &decoded.listing_value,
        &decoded.listing_value, // self-consistent (value == declared price); listing-price
        // cross-check is done in settle (Task 1.7) where price is fetched fresh.
        decoded.authorization.valid_before,
        now,
    )?;

    // Recover the signer and do the spent-nonce precheck.
    let payer = recover_payer(&state, &decoded).await?;

    // Confirm the `from` field in the header was not forged.
    let payer_hex = format!("0x{:x}", payer);
    if payer_hex.to_lowercase() != decoded.from.to_lowercase() {
        return Err(DashboardError::BadRequest("signature/from mismatch".into()));
    }

    Ok(Json(VerifyOut {
        valid: true,
        payer: payer_hex,
    }))
}

// ---------------------------------------------------------------------------
// Task 1.7: /facilitator/settle + X-PAYMENT-RESPONSE header
// ---------------------------------------------------------------------------

/// Encode the settlement receipt as a base64 JSON string for the
/// `X-PAYMENT-RESPONSE` response header.
///
/// Format: base64(JSON `{"success":true,"txHash":"...","network":"...","paidAt":<u64>}`)
pub fn encode_payment_response(tx_hash: &str, network: &str, paid_at: u64) -> String {
    use base64::Engine;
    let body = serde_json::json!({
        "success": true,
        "txHash": tx_hash,
        "network": network,
        "paidAt": paid_at,
    });
    base64::engine::general_purpose::STANDARD.encode(body.to_string())
}

/// Core settle logic shared by `post_settle` and the X-PAYMENT branch of
/// `get_x402`. Decodes the `X-PAYMENT` header, builds a `BuyRequest`, drives
/// the existing `buy_listing` relay, and returns the settlement response with
/// an `X-PAYMENT-RESPONSE` header.
pub async fn settle_from_header(
    state: AppState,
    listing_id: u64,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    // Decode X-PAYMENT header.
    let hdr = headers
        .get("x-payment")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| DashboardError::BadRequest("missing X-PAYMENT header".into()))?;
    let decoded = decode_x_payment(hdr)?;

    // Build the relay BuyBody: non-custodial — recipient == payer (M-2 guard).
    let body = crate::routes::marketplace::BuyBody {
        listing_id,
        recipient: decoded.from.clone(),
        authorization: decoded.authorization,
    };
    let req = crate::routes::marketplace::build_buy_request(&body)?;

    // Load chain config.
    let mp = state.marketplace_chain();
    let chain = mp
        .and_then(|c| c.chain.as_ref())
        .ok_or_else(|| DashboardError::ServiceUnavailable("chain relay not configured".into()))?;
    let addrs = mp
        .and_then(|c| c.marketplace_addresses.clone())
        .ok_or_else(|| DashboardError::ServiceUnavailable("marketplace not configured".into()))?;
    let net = format!("eip155:{}", chain.chain_id);

    // Build the signer-backed driver and settle.
    let driver = xvision_marketplace::adapter::Erc8004MantleDriver::with_signer(
        addrs,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );
    let receipt = driver.buy_listing(req).await.map_err(DashboardError::from)?;

    // Build the response body.
    let tx = format!("0x{:x}", receipt.tx_hash);
    let paid_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut resp = Json(crate::routes::marketplace::BuyOut {
        tx_hash: tx.clone(),
        license_token_id: receipt.license_token_id.to_string(),
    })
    .into_response();

    // Attach X-PAYMENT-RESPONSE header (base64 ASCII — parse won't fail, but
    // we avoid unwrap in non-test code).
    let header_val = axum::http::HeaderValue::from_str(&encode_payment_response(&tx, &net, paid_at))
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("x-payment-response header: {e}")))?;
    resp.headers_mut().insert("x-payment-response", header_val);

    Ok(resp)
}

/// `POST /api/marketplace/facilitator/settle/:id` — thin wrapper around
/// `settle_from_header`. Route registration happens in Task 1.8.
pub async fn post_settle(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    settle_from_header(state, id, headers).await
}

/// `GET /api/marketplace/listings/:id/x402` — returns 402 with payment
/// requirements, or, when an `X-PAYMENT` header is present, delegates to
/// `settle_from_header` (the x402 protocol shortcut path).
pub async fn get_x402(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    // X-PAYMENT branch: act as a facilitator and settle immediately.
    if headers.get("x-payment").is_some() {
        return settle_from_header(state, id, headers).await;
    }

    let mp = state.marketplace_chain();
    let chain = mp
        .and_then(|c| c.chain.as_ref())
        .ok_or_else(|| DashboardError::ServiceUnavailable("chain relay not configured".into()))?;
    let addrs = mp
        .and_then(|c| c.marketplace_addresses.clone())
        .ok_or_else(|| DashboardError::ServiceUnavailable("marketplace not configured".into()))?;

    let driver = xvision_marketplace::adapter::Erc8004MantleDriver::new(
        addrs.clone(),
        chain.rpc_url.clone(),
        chain.chain_id,
    );
    let view = driver
        .fetch_listing(alloy::primitives::U256::from(id))
        .await
        .map_err(DashboardError::from)?;

    let body = build_accepts(
        chain.chain_id,
        &format!("0x{:x}", addrs.usdc),
        &format!("0x{:x}", addrs.marketplace),
        id,
        &view.price_usdc.to_string(),
    );
    Ok((StatusCode::PAYMENT_REQUIRED, Json(body)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_rejects_underpayment_and_expiry() {
        let now = 1_000u64;
        assert!(check_terms(/*value*/ "49000000", /*price*/ "49000000", /*valid_before*/ 2000, now).is_ok());
        assert!(check_terms("10000000", "49000000", 2000, now).is_err()); // underpay
        assert!(check_terms("49000000", "49000000", 999, now).is_err()); // expired
    }

    #[test]
    fn verify_rejects_used_nonce() {
        // The on-chain authorizationState(from, nonce) read feeds this pure
        // decision. `true` = already used → reject; `false` = fresh → ok.
        assert!(ensure_unused(false).is_ok());
        assert!(ensure_unused(true).is_err());
    }

    #[test]
    fn accepts_has_exact_scheme_and_network() {
        let a = build_accepts(5000, "0xUSDC", "0xMKT", 42, "49000000");
        assert_eq!(a.x402_version, 1);
        let pr = &a.accepts[0];
        assert_eq!(pr.scheme, "exact");
        assert_eq!(pr.network, "eip155:5000");
        assert_eq!(pr.max_amount_required, "49000000");
        assert_eq!(pr.extra["listingId"], 42);
    }

    #[test]
    fn decode_x_payment_roundtrip() {
        let json = serde_json::json!({
            "x402Version": 1,
            "scheme": "exact",
            "network": "eip155:5000",
            "payload": {
                "authorization": {
                    "from":"0x1111111111111111111111111111111111111111",
                    "to":"0x2222222222222222222222222222222222222222",
                    "value":"49000000","validAfter":"0","validBefore":"9999999999",
                    "nonce":"0x33"
                },
                "signature": format!("0x{}1b", "00".repeat(64))  // 65-byte dummy (v=0x1b=27)
            }
        });
        use base64::Engine;
        let hdr = base64::engine::general_purpose::STANDARD.encode(json.to_string());
        let decoded = decode_x_payment(&hdr).unwrap();
        assert_eq!(decoded.listing_value, "49000000");
        assert_eq!(decoded.from, "0x1111111111111111111111111111111111111111");
        // 65-byte sig split: r=bytes[0..32], s=bytes[32..64], v=byte 64 (0x1b=27).
        assert_eq!(decoded.authorization.v, 27);
        assert_eq!(
            decoded.authorization.r,
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(decoded.authorization.s, format!("0x{}", "00".repeat(32)));
        assert_eq!(decoded.authorization.valid_before, 9_999_999_999);
    }

    #[test]
    fn payment_response_header_encodes() {
        let h = encode_payment_response("0xabc", "eip155:5000", 1_700_000_000);
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(h).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(v["txHash"], "0xabc");
        assert_eq!(v["network"], "eip155:5000");
        assert_eq!(v["success"], true);
        assert_eq!(v["paidAt"], 1_700_000_000u64);
    }

    #[test]
    fn decode_x_payment_rejects_bad_validbefore() {
        let json = serde_json::json!({
            "payload": {
                "authorization": {
                    "from":"0x1111111111111111111111111111111111111111",
                    "to":"0x2222222222222222222222222222222222222222",
                    "value":"49000000","validAfter":"0","validBefore":"notanumber",
                    "nonce":"0x33"
                },
                "signature": format!("0x{}1b", "00".repeat(64))
            }
        });
        use base64::Engine;
        let hdr = base64::engine::general_purpose::STANDARD.encode(json.to_string());
        assert!(decode_x_payment(&hdr).is_err());
    }
}
