//! Public perpetual-futures data feed (funding rate, open interest, mark
//! price) sourced from Hyperliquid's **public, no-auth** info endpoint.
//!
//! This is live-only: it populates [`OnchainPanel`] (funding + OI) for the
//! agent briefing and supplies the mark price the engine threads onto the
//! filter `Bar` perps fields. The backtest path leaves all perps fields
//! `None` (perps backtesting is deferred — see the spec).
//!
//! Hyperliquid returns asset contexts with string-typed numeric fields
//! (`funding`, `openInterest`, `markPx`); we parse them to `f64` and drop
//! any context we can't read rather than failing the whole cycle.

use serde::Deserialize;
use xvision_core::OnchainPanel;

const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";

/// One snapshot of perps state for a single market.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerpSnapshot {
    /// Funding rate as a per-interval fraction (e.g. `0.0000125`).
    pub funding_rate: f64,
    /// Open interest in USD.
    pub open_interest: f64,
    /// Venue mark price.
    pub mark_price: f64,
}

/// Hyperliquid asset-context wire shape (string-typed numerics).
#[derive(Debug, Deserialize)]
struct AssetCtx {
    funding: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    #[serde(rename = "markPx")]
    mark_px: String,
}

/// Parse a single Hyperliquid asset-context JSON object into a
/// [`PerpSnapshot`]. Returns `None` if the body is not the expected shape or
/// any numeric string fails to parse — callers degrade gracefully (perps
/// fields stay `None`/stale rather than erroring the decision cycle).
pub fn parse_perp_snapshot(body: &str) -> Option<PerpSnapshot> {
    let ctx: AssetCtx = serde_json::from_str(body).ok()?;
    Some(PerpSnapshot {
        funding_rate: ctx.funding.parse().ok()?,
        open_interest: ctx.open_interest.parse().ok()?,
        mark_price: ctx.mark_px.parse().ok()?,
    })
}

/// Write the funding + open-interest readings onto an [`OnchainPanel`] for the
/// agent briefing. Mark price is not part of the panel (it rides on the filter
/// `Bar` instead), so it is intentionally not applied here.
pub fn apply_to_onchain(snap: &PerpSnapshot, panel: &mut OnchainPanel) {
    panel.funding_rate_8h = Some(snap.funding_rate);
    panel.open_interest_usd = Some(snap.open_interest);
}

/// Fetch the latest perps snapshot for `symbol` (bare coin, e.g. `"BTC"`)
/// from the Hyperliquid public info endpoint. No auth required.
///
/// Network errors and unexpected response shapes return `None` so the caller
/// can fall back to the last-known reading.
pub async fn fetch_perp_snapshot(client: &reqwest::Client, symbol: &str) -> Option<PerpSnapshot> {
    // Hyperliquid's `metaAndAssetCtxs` returns [universe, [assetCtx...]] where
    // assetCtx[i] aligns with universe.universe[i]. We request it and match the
    // coin by index.
    let body = serde_json::json!({ "type": "metaAndAssetCtxs" });
    let resp = client.post(HYPERLIQUID_INFO_URL).json(&body).send().await.ok()?;
    let value: serde_json::Value = resp.json().await.ok()?;
    let universe = value.get(0)?.get("universe")?.as_array()?;
    let ctxs = value.get(1)?.as_array()?;
    let idx = universe
        .iter()
        .position(|u| u.get("name").and_then(|n| n.as_str()) == Some(symbol))?;
    let ctx = ctxs.get(idx)?;
    parse_perp_snapshot(&ctx.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hyperliquid_funding_oi_mark() {
        let body = r#"{ "funding": "0.0000125", "openInterest": "9000000", "markPx": "60000.5" }"#;
        let snap = parse_perp_snapshot(body).expect("valid body must parse");
        assert!((snap.funding_rate - 0.0000125).abs() < 1e-12);
        assert_eq!(snap.open_interest, 9_000_000.0);
        assert_eq!(snap.mark_price, 60_000.5);
    }

    #[test]
    fn malformed_body_returns_none() {
        assert_eq!(parse_perp_snapshot("not json"), None);
        // Missing fields → None.
        assert_eq!(parse_perp_snapshot(r#"{ "funding": "0.1" }"#), None);
        // Non-numeric string → None.
        assert_eq!(
            parse_perp_snapshot(r#"{ "funding": "x", "openInterest": "1", "markPx": "2" }"#),
            None
        );
    }

    #[test]
    fn apply_writes_funding_and_oi_to_panel() {
        let snap = PerpSnapshot {
            funding_rate: 0.0003,
            open_interest: 12_000_000.0,
            mark_price: 61_000.0,
        };
        let mut panel = OnchainPanel::default();
        apply_to_onchain(&snap, &mut panel);
        assert_eq!(panel.funding_rate_8h, Some(0.0003));
        assert_eq!(panel.open_interest_usd, Some(12_000_000.0));
        // Mark price is not a panel field — must remain untouched elsewhere.
        assert_eq!(panel.long_short_ratio, None);
    }
}
