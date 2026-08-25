//! Nansen on-chain signal tools. Three operator-facing capabilities.
//!
//! ## Mode-aware routing
//!
//! The dispatch chokepoint injects `as_of_date` into tool inputs only when the
//! run mode is Backtest (and strips it in live runs). The tools branch on its
//! *presence*: absent → live `/api/v1/...`; present → historical endpoint or
//! an explicit degrade.
//!
//! # ENDPOINT GROUNDING (audited against https://docs.nansen.ai 2026-08-25)
//!
//! API restructured 2025-08-29; current verified bindings:
//!   - `nansen_smart_money_flow`: live `/api/v1/smart-money/netflow`
//!     (`{chains:[…], filters:{token_address}}`). Netflow has NO history
//!     (rolling 30 days, no backfill) → backtest degrades.
//!   - `nansen_token_screener`: live `/api/v1/token-screener`
//!     (`{chains:[…], timeframe, filters:{token_address}}`; NOTE: no `tgm/`
//!     prefix). Historical `/api/v1beta1/token-screener/historical`
//!     (`{to_date, timeframe_days, chains}`); no server-side token filter →
//!     rows filtered client-side by `token_address`.
//!   - `nansen_flow_intel`: live `/api/v1/tgm/flow-intelligence`
//!     (`{chain, token_address}`). Historical
//!     `/api/v1beta1/tgm/historical-who-bought-sold`
//!     (`{chain, token_address, date_range{from,to}}`).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};
use xvision_data::nansen::{NansenClient, NansenError};

use crate::tools::{Tool, ToolName};

/// Window (days) used by every historical Nansen call, ending at `as_of_date`.
const HIST_WINDOW_DAYS: i64 = 7;

#[derive(Deserialize)]
struct AssetInput {
    asset: String,
    #[serde(default)]
    as_of_date: Option<String>, // injected by the dispatch in backtest only
}

/// A routed Nansen request.
enum NansenRoute {
    Call { path: &'static str, body: serde_json::Value },
    /// POST, then keep only rows whose `token_address` equals the asset's
    /// contract address (the historical screener screens a whole chain and has
    /// no server-side token filter).
    CallFilterTokenAddress { path: &'static str, body: serde_json::Value },
    Unavailable(&'static str),
}

macro_rules! nansen_tool {
    ($ty:ident, $name:literal, $desc:literal) => {
        pub struct $ty {
            client: Arc<NansenClient>,
        }
        impl $ty {
            pub fn new(client: Arc<NansenClient>) -> Self {
                Self { client }
            }
            #[cfg(test)]
            pub fn for_test(base_url: String) -> Self {
                Self {
                    client: Arc::new(NansenClient::new(base_url, "test".into(), 300)),
                }
            }
        }
        #[async_trait]
        impl Tool for $ty {
            fn name(&self) -> ToolName {
                ToolName::new($name)
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    name: $name.to_string(),
                    version: "1".to_string(),
                    description: $desc.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": { "asset": { "type": "string" } },
                        "required": ["asset"],
                        "additionalProperties": true
                    }),
                    output_schema: json!({ "type": "object", "additionalProperties": true }),
                    timeout_ms: 15_000,
                    side_effect_level: SideEffectLevel::ExternalRead,
                    requires_approval: false,
                }
            }
            async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
                Ok($ty::route(&self.client, input).await)
            }
        }
    };
}

nansen_tool!(
    NansenSmartMoneyFlowTool,
    "nansen_smart_money_flow",
    "Smart-money net flow for a token (on-chain). Live only — netflow has no history."
);
nansen_tool!(
    NansenTokenScreenerTool,
    "nansen_token_screener",
    "Token screener metrics over a 7d window. Live + backtest (point-in-time)."
);
nansen_tool!(
    NansenFlowIntelTool,
    "nansen_flow_intel",
    "Flow intelligence by holder segment. Live + backtest (point-in-time window)."
);

/// Shared driver: parse input, resolve on-chain identity, execute the routed
/// request, convert transport errors to the degrade shape. Degrades (D8) if
/// the asset has no mapped identity or the input is malformed — never panics,
/// never hits the network for unmapped assets.
async fn nansen_invoke(
    client: &NansenClient,
    input: serde_json::Value,
    route: impl FnOnce(&xvision_core::asset_registry::SignalAssetIdentity, Option<&str>) -> NansenRoute,
) -> serde_json::Value {
    use crate::tools::signal_policy::signal_unavailable;
    let parsed: AssetInput = match serde_json::from_value(input) {
        Ok(v) => v,
        Err(e) => return signal_unavailable(format!("bad input: {e}")),
    };
    let Some(id) = xvision_core::asset_registry::signal_asset_identity(&parsed.asset) else {
        return signal_unavailable(format!("no on-chain identity mapped for {}", parsed.asset));
    };
    match route(&id, parsed.as_of_date.as_deref()) {
        NansenRoute::Unavailable(reason) => signal_unavailable(reason),
        NansenRoute::Call { path, body } => post_or_degrade(client, path, body).await,
        NansenRoute::CallFilterTokenAddress { path, body } => {
            let out = post_or_degrade(client, path, body).await;
            filter_rows_by_address(out, id.contract_address)
        }
    }
}

async fn post_or_degrade(client: &NansenClient, path: &str, body: serde_json::Value) -> serde_json::Value {
    use crate::tools::signal_policy::signal_unavailable;
    match client.post(path, body).await {
        Ok(v) => v,
        Err(NansenError::RateLimited) => signal_unavailable("nansen rate limited"),
        Err(NansenError::CreditsExhausted) => signal_unavailable("nansen credits exhausted"),
        Err(e) => signal_unavailable(format!("nansen unavailable: {e}")),
    }
}

/// Keep only response rows for the requested token; degrade when absent or
/// when the response shape is not what we asked for. A non-array `data`
/// (vendor error wrapped in HTTP 200, changed envelope) must NEVER pass
/// through unfiltered — that would present chain-wide rows as this asset's
/// signal.
fn filter_rows_by_address(mut out: serde_json::Value, address: &str) -> serde_json::Value {
    use crate::tools::signal_policy::signal_unavailable;
    let Some(rows) = out.get_mut("data").and_then(|d| d.as_array_mut()) else {
        return signal_unavailable("unexpected nansen screener envelope (no data array)");
    };
    let want = address.to_ascii_lowercase();
    rows.retain(|r| {
        r.get("token_address")
            .and_then(|a| a.as_str())
            .is_some_and(|a| a.eq_ignore_ascii_case(&want))
    });
    if rows.is_empty() {
        // Distinguish "the token has no row" from "our single page ended
        // before the token's row" — a truncated fetch is an honest degrade
        // reason, silent truncation is not.
        let more_pages = out
            .get("pagination")
            .and_then(|p| p.get("is_last_page"))
            .and_then(|b| b.as_bool())
            .map(|last| !last)
            .unwrap_or(false);
        return if more_pages {
            signal_unavailable(format!(
                "no screener row for {address} in the fetched page — the historical window spans multiple pages"
            ))
        } else {
            signal_unavailable(format!("no screener row for {address} in the historical window"))
        };
    }
    out
}

impl NansenSmartMoneyFlowTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, input, |id, as_of| match as_of {
            // Verified: netflow retains only a rolling 30-day window and takes
            // no custom date range — there is no historical counterpart.
            Some(_) => NansenRoute::Unavailable("backtest-unavailable for nansen_smart_money_flow"),
            None => NansenRoute::Call {
                path: "/api/v1/smart-money/netflow",
                body: json!({ "chains": [id.chain], "filters": { "token_address": id.contract_address } }),
            },
        })
        .await
    }
}
impl NansenTokenScreenerTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, input, |id, as_of| match as_of {
            None => NansenRoute::Call {
                path: "/api/v1/token-screener", // verified: NO tgm/ prefix
                body: json!({
                    "chains": [id.chain],
                    "timeframe": "7d",
                    "filters": { "token_address": id.contract_address }
                }),
            },
            Some(d) => match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                Err(_) => NansenRoute::Unavailable("bad as_of_date"),
                Ok(to) => NansenRoute::CallFilterTokenAddress {
                    path: "/api/v1beta1/token-screener/historical",
                    body: json!({
                        "to_date": to.to_string(),
                        "timeframe_days": HIST_WINDOW_DAYS,
                        "chains": [id.chain],
                        "pagination": { "page": 1, "per_page": 1000 }
                    }),
                },
            },
        })
        .await
    }
}
impl NansenFlowIntelTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, input, |id, as_of| match as_of {
            None => NansenRoute::Call {
                path: "/api/v1/tgm/flow-intelligence",
                body: json!({ "chain": id.chain, "token_address": id.contract_address }),
            },
            Some(d) => match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                Err(_) => NansenRoute::Unavailable("bad as_of_date"),
                Ok(to) => NansenRoute::Call {
                    path: "/api/v1beta1/tgm/historical-who-bought-sold",
                    body: json!({
                        "chain": id.chain,
                        "token_address": id.contract_address,
                        "date_range": {
                            "from": (to - chrono::Duration::days(HIST_WINDOW_DAYS - 1)).to_string(),
                            "to": to.to_string(),
                        }
                    }),
                },
            },
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[tokio::test]
    async fn smart_money_flow_descriptor_and_name() {
        let t = NansenSmartMoneyFlowTool::for_test("http://unused".into());
        assert_eq!(t.name().as_str(), "nansen_smart_money_flow");
        assert_eq!(
            t.descriptor().side_effect_level,
            xvision_agent_client::protocol::SideEffectLevel::ExternalRead
        );
    }

    #[tokio::test]
    async fn smart_money_flow_live_body_uses_chains_array() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1/smart-money/netflow")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"chains":["ethereum"],"filters":{"token_address":"0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"}}"#.into(),
            ))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = NansenSmartMoneyFlowTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"BTC"})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn smart_money_flow_backtest_degrades_without_http() {
        // Verified: netflow has no historical endpoint — must degrade before
        // any HTTP (unroutable port proves no call is made).
        let t = NansenSmartMoneyFlowTool::for_test("http://127.0.0.1:1".into());
        let out = t
            .invoke(serde_json::json!({"asset":"BTC","as_of_date":"2024-03-14"}))
            .await
            .unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("backtest-unavailable"));
    }

    #[tokio::test]
    async fn screener_live_hits_v1_without_tgm_prefix() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1/token-screener")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"timeframe":"7d","chains":["ethereum"]}"#.into(),
            ))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = NansenTokenScreenerTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"ETH"})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn screener_backtest_filters_rows_by_token_address() {
        const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1beta1/token-screener/historical")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"to_date":"2024-03-14","timeframe_days":7,"chains":["ethereum"]}"#.into(),
            ))
            .with_status(200)
            .with_body(format!(
                r#"{{"data":[{{"token_address":"0xother","netflow":1.0}},{{"token_address":"{WETH}","netflow":2.0}}]}}"#
            ))
            .create_async()
            .await;
        let t = NansenTokenScreenerTool::for_test(server.url());
        let out = t
            .invoke(serde_json::json!({"asset":"ETH","as_of_date":"2024-03-14"}))
            .await
            .unwrap();
        let rows = out["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["token_address"], WETH);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn screener_backtest_degrades_when_row_absent() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/v1beta1/token-screener/historical")
            .with_status(200)
            .with_body(r#"{"data":[{"token_address":"0xother"}]}"#)
            .create_async()
            .await;
        let t = NansenTokenScreenerTool::for_test(server.url());
        let out = t
            .invoke(serde_json::json!({"asset":"ETH","as_of_date":"2024-03-14"}))
            .await
            .unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("no screener row"));
    }

    #[test]
    fn filter_degrades_on_non_array_envelope() {
        // A vendor error wrapped in HTTP 200 (or a changed envelope) must
        // degrade — never pass chain-wide rows through as the asset's signal.
        let out = serde_json::json!({"error": "boom"});
        let filtered = filter_rows_by_address(out, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
        assert_eq!(filtered["available"], false);
        assert!(filtered["reason"].as_str().unwrap().contains("unexpected nansen screener envelope"));
    }

    #[test]
    fn filter_names_truncation_when_more_pages_remain() {
        let out = serde_json::json!({
            "data": [{"token_address": "0xaaa"}],
            "pagination": {"is_last_page": false}
        });
        let filtered = filter_rows_by_address(out, "0xbbb");
        assert_eq!(filtered["available"], false);
        assert!(filtered["reason"]
            .as_str()
            .unwrap()
            .contains("multiple pages"));
    }

    #[tokio::test]
    async fn flow_intel_backtest_routes_to_historical_who_bought_sold() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1beta1/tgm/historical-who-bought-sold")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"chain":"ethereum","date_range":{"from":"2024-03-08","to":"2024-03-14"}}"#.into(),
            ))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = NansenFlowIntelTool::for_test(server.url());
        let out = t
            .invoke(serde_json::json!({"asset":"ETH","as_of_date":"2024-03-14"}))
            .await
            .unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn unmapped_asset_degrades_no_http() {
        // No mock server needed — an unmapped asset must degrade before any HTTP.
        let t = NansenSmartMoneyFlowTool::for_test("http://127.0.0.1:1".into());
        let out = t.invoke(serde_json::json!({"asset":"NOTACOIN"})).await.unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("no on-chain identity"));
    }
}
