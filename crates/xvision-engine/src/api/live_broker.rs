//! Live broker (execution venue) status — `venue_account`.
//!
//! Operator-facing snapshot of the Orderly Network account that backs
//! live-trading runs (live-trading hackathon, 2026-06-11). The dashboard's
//! `GET /api/live/venue-account` handler is a thin wrapper over
//! [`venue_account`]; all venue logic lives here so the dashboard never
//! depends on `xvision-execution` directly.
//!
//! Missing `ORDERLY_*` env is NOT an error: the endpoint reports
//! `{ connected: false, reason: "…" }` so the live page can render a
//! "not configured" state instead of a 500. Venue fetch failures are
//! reported the same way — this is a status surface, not a trade path.

use serde::Serialize;

use xvision_execution::OrderlyExecutor;

use super::ApiResult;

/// One open position on the venue, flattened for the dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct VenuePositionDto {
    /// Venue market string, e.g. `"PERP_BTC_USDC"`.
    pub symbol: String,
    /// Signed base-asset quantity (positive = long, negative = short).
    pub qty: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
}

/// Connection + account snapshot for the live execution venue.
#[derive(Debug, Clone, Serialize)]
pub struct VenueAccountDto {
    pub connected: bool,
    /// Always `"orderly"` in the current live scope.
    pub venue: String,
    /// `"testnet"` or `"mainnet"`, derived from `ORDERLY_BASE_URL`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// `ORDERLY_ACCOUNT_ID` (not secret — it is the public account id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equity_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usdc_holding: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl: Option<f64>,
    pub positions: Vec<VenuePositionDto>,
    /// Populated when `connected == false`: why the venue is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl VenueAccountDto {
    fn disconnected(reason: String) -> Self {
        Self {
            connected: false,
            venue: "orderly".into(),
            network: None,
            account_id: None,
            equity_usd: None,
            usdc_holding: None,
            unrealized_pnl: None,
            positions: Vec::new(),
            reason: Some(reason),
        }
    }
}

/// Derive the operator-facing network label from `ORDERLY_BASE_URL`. An
/// unset URL means the executor defaults to the mainnet EVM gateway.
fn network_from_base_url(base_url: Option<&str>) -> &'static str {
    match base_url {
        Some(url) if url.contains("testnet") => "testnet",
        _ => "mainnet",
    }
}

/// Snapshot the live execution venue account. When `venue` is `None` or
/// `Some("orderly")`, queries Orderly via the executor. For any other venue
/// name, returns a `connected: false` stub indicating the account view is
/// not yet wired — credentials may be stored but live ledger snapshots for
/// non-Orderly venues are not implemented at this revision.
///
/// Never errors on a missing/unreachable venue — returns `connected: false`
/// with a reason so the live page can render a "not configured" state instead
/// of an HTTP error.
pub async fn venue_account(venue: Option<&str>) -> ApiResult<VenueAccountDto> {
    // Route non-Orderly venue requests to a stub response.
    match venue {
        Some(v) if v != "orderly" => {
            return Ok(VenueAccountDto {
                connected: false,
                venue: v.to_string(),
                network: None,
                account_id: None,
                equity_usd: None,
                usdc_holding: None,
                unrealized_pnl: None,
                positions: vec![],
                reason: Some(format!(
                    "Live ledger snapshot for {v} is not wired yet — credentials are stored; \
                     account view is Orderly-only at this revision."
                )),
            });
        }
        _ => {}
    }

    // Orderly path (venue == None or "orderly").
    let missing: Vec<&str> = ["ORDERLY_KEY", "ORDERLY_SECRET", "ORDERLY_ACCOUNT_ID"]
        .into_iter()
        .filter(|v| std::env::var(v).map(|s| s.trim().is_empty()).unwrap_or(true))
        .collect();
    if !missing.is_empty() {
        return Ok(VenueAccountDto::disconnected(format!(
            "Orderly credentials not configured: {} unset",
            missing.join(", ")
        )));
    }

    let base_url = std::env::var("ORDERLY_BASE_URL").ok();
    let network = network_from_base_url(base_url.as_deref()).to_string();
    let account_id = std::env::var("ORDERLY_ACCOUNT_ID").unwrap_or_default();

    let executor = match OrderlyExecutor::from_env() {
        Ok(e) => e,
        Err(e) => {
            return Ok(VenueAccountDto::disconnected(format!(
                "Orderly client build failed: {e}"
            )));
        }
    };

    match executor.venue_snapshot().await {
        Ok(snap) => Ok(VenueAccountDto {
            connected: true,
            venue: "orderly".into(),
            network: Some(network),
            account_id: Some(account_id),
            equity_usd: Some(snap.equity_usd),
            usdc_holding: Some(snap.usdc_holding),
            unrealized_pnl: Some(snap.unrealized_pnl),
            positions: snap
                .positions
                .into_iter()
                .map(|p| VenuePositionDto {
                    symbol: p.symbol,
                    qty: p.position_qty,
                    entry_price: p.average_open_price,
                    mark_price: p.mark_price,
                    unrealized_pnl: p.unsettled_pnl,
                })
                .collect(),
            reason: None,
        }),
        Err(e) => Ok(VenueAccountDto::disconnected(format!(
            "Orderly snapshot failed: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_label_derives_from_base_url() {
        assert_eq!(
            network_from_base_url(Some("https://testnet-api-evm.orderly.org")),
            "testnet"
        );
        assert_eq!(
            network_from_base_url(Some("https://api-evm.orderly.org")),
            "mainnet"
        );
        assert_eq!(network_from_base_url(None), "mainnet");
    }

    #[test]
    fn disconnected_dto_serializes_without_account_fields() {
        let dto = VenueAccountDto::disconnected("Orderly credentials not configured".into());
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["connected"], false);
        assert_eq!(json["venue"], "orderly");
        assert!(json.get("equity_usd").is_none(), "no equity when disconnected");
        assert!(json["reason"].as_str().unwrap().contains("not configured"));
        assert!(json["positions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn connected_dto_serializes_positions() {
        let dto = VenueAccountDto {
            connected: true,
            venue: "orderly".into(),
            network: Some("testnet".into()),
            account_id: Some("0xabc".into()),
            equity_usd: Some(1_025.5),
            usdc_holding: Some(1_000.0),
            unrealized_pnl: Some(25.5),
            positions: vec![VenuePositionDto {
                symbol: "PERP_BTC_USDC".into(),
                qty: 0.5,
                entry_price: 70_000.0,
                mark_price: 71_000.0,
                unrealized_pnl: 500.0,
            }],
            reason: None,
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["network"], "testnet");
        assert_eq!(json["positions"][0]["symbol"], "PERP_BTC_USDC");
        assert_eq!(json["positions"][0]["qty"], 0.5);
        assert!(json.get("reason").is_none());
    }
}
