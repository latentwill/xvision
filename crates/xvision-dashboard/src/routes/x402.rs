//! x402 resource server + facilitator. Wraps the existing buyWithAuthorization
//! relay in the standard HTTP-402 protocol so any x402 client can pay.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

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
            valid_after: a.valid_after.parse().unwrap_or(0),
            valid_before: a.valid_before.parse().unwrap_or(0),
            nonce: a.nonce,
            v,
            r,
            s,
        },
    })
}

/// `GET /api/marketplace/listings/:id/x402` — returns 402 with payment
/// requirements. (The X-PAYMENT settle branch is added in Task 1.7.)
pub async fn get_x402(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Response, DashboardError> {
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
    }
}
