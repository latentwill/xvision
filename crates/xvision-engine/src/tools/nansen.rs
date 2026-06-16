//! Nansen on-chain signal tools. Three operator-facing capabilities. Each
//! routes to a `/api/v1/...` (live) endpoint here; the `/api/v1beta1`
//! historical routing for backtest + `as_of_date` is added in a later task.
//! The forward-only/backtest anchor is enforced upstream in
//! `ToolRegistryDispatch`, NOT in these tools — they are pure fetch+shape.

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

/// Shared fetch: parse input, POST via `build_body`, convert transport errors
/// to the degrade shape. `build_body(asset, as_of_date) -> request JSON`.
async fn nansen_invoke(
    client: &NansenClient,
    path: &str,
    input: serde_json::Value,
    build_body: impl FnOnce(&str, Option<&str>) -> serde_json::Value,
) -> serde_json::Value {
    let parsed: AssetInput = match serde_json::from_value(input) {
        Ok(v) => v,
        Err(e) => return degrade(format!("bad input: {e}")),
    };
    // Phase-5 swap point: replace bare symbol with resolved chain/contract identity.
    let body = build_body(&parsed.asset, parsed.as_of_date.as_deref());
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

// Per-tool live-endpoint routing. A later task makes these mode-aware (v1 vs v1beta1).
impl NansenSmartMoneyFlowTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/smart-money/netflow", input, |asset, _as_of| {
            json!({ "symbol": asset })
        })
        .await
    }
}
impl NansenTokenScreenerTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/tgm/token-screener", input, |asset, _as_of| {
            json!({ "symbol": asset })
        })
        .await
    }
}
impl NansenFlowIntelTool {
    async fn route(client: &NansenClient, input: serde_json::Value) -> serde_json::Value {
        nansen_invoke(client, "/api/v1/tgm/flow-intelligence", input, |asset, _as_of| {
            json!({ "symbol": asset })
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
}
