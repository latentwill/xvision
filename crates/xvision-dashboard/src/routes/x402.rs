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
}
