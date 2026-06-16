//! Nansen on-chain signal tools. Three operator-facing capabilities.
//!
//! ## Mode-aware routing (Task 3.1)
//!
//! The dispatch chokepoint (Task 1.5) injects `as_of_date` into tool inputs
//! only when the run mode is Backtest. So the tools stay pure: they branch on
//! the *presence* of `as_of_date` — present → historical `/api/v1beta1/...`
//! endpoint; absent → live `/api/v1/...` endpoint. No `run_mode` enum leaks
//! into the tool layer.
//!
//! The forward-only/backtest anchor is enforced upstream in
//! `ToolRegistryDispatch`, NOT in these tools.
//!
//! # GROUNDING (verify before mainnet, Task 6.4):
//! The v1beta1 endpoint paths below are taken from the Nansen API v5 spec
//! (draft). Verify each path and response shape against the live Nansen v5
//! docs before any production use:
//!   - `/api/v1beta1/smart-money/historical-token-balances`
//!   - `/api/v1beta1/token-screener/historical`
//!   - `/api/v1beta1/tgm/historical-who-bought-sold`
//! Any metric that lacks a real historical counterpart in the live API MUST
//! fall back to `degrade("backtest-unavailable for <tool>")` rather than
//! silently hitting the live endpoint with an `as_of_date` it ignores.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};
use xvision_data::nansen::{NansenClient, NansenError};

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct AssetInput {
    asset: String,
    #[serde(default)]
    as_of_date: Option<String>, // injected by the dispatch in backtest; unused in v1 live (Phase 3 uses it)
}

/// Map a `NansenError` to a structured degrade value (D8) — a *successful*
/// tool result the Cline loop can read, never an `Err`.
fn degrade(reason: impl Into<String>) -> serde_json::Value {
    json!({ "available": false, "reason": reason.into() })
}

/// Shared fetch: parse input, resolve on-chain identity, POST via `build_body`,
/// convert transport errors to the degrade shape.
/// `build_body(identity, as_of_date) -> request JSON`.
/// Degrades (D8) if the asset has no mapped on-chain identity — never panics,
/// never hits the network for unmapped assets.
async fn nansen_invoke(
    client: &NansenClient,
    path: &str,
    input: serde_json::Value,
    build_body: impl FnOnce(&xvision_core::asset_registry::SignalAssetIdentity, Option<&str>) -> serde_json::Value,
) -> serde_json::Value {
    let parsed: AssetInput = match serde_json::from_value(input) {
        Ok(v) => v,
        Err(e) => return degrade(format!("bad input: {e}")),
    };
    let Some(id) = xvision_core::asset_registry::signal_asset_identity(&parsed.asset) else {
        return degrade(format!("no on-chain identity mapped for {}", parsed.asset));
    };
    let body = build_body(&id, parsed.as_of_date.as_deref());
    match client.post(path, body).await {
        Ok(v) => v,
        Err(NansenError::RateLimited) => degrade("nansen rate limited"),
        Err(NansenError::CreditsExhausted) => degrade("nansen credits exhausted"),
        Err(e) => degrade(format!("nansen unavailable: {e}")),
    }
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
    "Smart-money net flow for a token (on-chain). Live + backtest (point-in-time)."
);
nansen_tool!(
    NansenTokenScreenerTool,
    "nansen_token_screener",
    "Token screener / token-god-mode metrics. Live + backtest (point-in-time)."
);
nansen_tool!(
    NansenFlowIntelTool,
    "nansen_flow_intel",
    "Flow intelligence (who-bought-sold + quant scores). Live + backtest (point-in-time)."
);

// Per-tool mode-aware routing: v1 (live) when no as_of_date; v1beta1
// (historical) when as_of_date is present (injected by the dispatch for
// Backtest runs). The build_body closure forwards as_of_date when present.
impl NansenSmartMoneyFlowTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        let historical = input.get("as_of_date").and_then(|v| v.as_str()).is_some();
        let path = if historical {
            "/api/v1beta1/smart-money/historical-token-balances"
        } else {
            "/api/v1/smart-money/netflow"
        };
        nansen_invoke(client, path, input, |id, as_of| match as_of {
            Some(d) => json!({ "chain": id.chain, "token_address": id.contract_address, "as_of_date": d }),
            None => json!({ "chain": id.chain, "token_address": id.contract_address }),
        })
        .await
    }
}
impl NansenTokenScreenerTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        let historical = input.get("as_of_date").and_then(|v| v.as_str()).is_some();
        let path = if historical {
            "/api/v1beta1/token-screener/historical"
        } else {
            "/api/v1/tgm/token-screener"
        };
        nansen_invoke(client, path, input, |id, as_of| match as_of {
            Some(d) => json!({ "chain": id.chain, "token_address": id.contract_address, "as_of_date": d }),
            None => json!({ "chain": id.chain, "token_address": id.contract_address }),
        })
        .await
    }
}
impl NansenFlowIntelTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        let historical = input.get("as_of_date").and_then(|v| v.as_str()).is_some();
        let path = if historical {
            "/api/v1beta1/tgm/historical-who-bought-sold"
        } else {
            "/api/v1/tgm/flow-intelligence"
        };
        nansen_invoke(client, path, input, |id, as_of| match as_of {
            Some(d) => json!({ "chain": id.chain, "token_address": id.contract_address, "as_of_date": d }),
            None => json!({ "chain": id.chain, "token_address": id.contract_address }),
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
    async fn smart_money_flow_hits_v1_live_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1/smart-money/netflow")
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
    async fn backtest_routes_to_v1beta1_with_as_of() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/api/v1beta1/smart-money/historical-token-balances")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"as_of_date":"2024-03-14"}"#.into(),
            ))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = NansenSmartMoneyFlowTool::for_test(server.url());
        t.invoke(serde_json::json!({"asset":"BTC","as_of_date":"2024-03-14"}))
            .await
            .unwrap();
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
