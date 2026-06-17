//! Elfa crypto-social / KOL intelligence tools. Three operator-facing
//! capabilities, all forward-only (live runs only).
//!
//! ## Forward-only enforcement
//!
//! Elfa has no historical API. The dispatch chokepoint (`ToolRegistryDispatch`)
//! enforces forward-only at the call site — these tools are pure fetch+shape
//! and do NOT inspect run_mode. A backtest call to any Elfa tool returns an
//! Err from the dispatch layer, never reaching these impls.
//!
//! ## Degrade shape
//!
//! Network / auth / rate failures return a structured Ok value (`available:
//! false, reason: ...`) so the Cline loop can read the failure reason rather
//! than receiving a hard error.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};
use xvision_data::elfa::{ElfaClient, ElfaError};

use crate::tools::signal_policy::signal_unavailable;
use crate::tools::{Tool, ToolName};

/// Shared GET fetch helper. Network/auth/rate failures all degrade gracefully,
/// delegating to the canonical `signal_unavailable` (xvision-im2r.9).
async fn elfa_invoke(client: &ElfaClient, path: &str, query: &[(&str, &str)]) -> serde_json::Value {
    match client.get(path, query).await {
        Ok(v) => v,
        Err(ElfaError::RateLimited) => signal_unavailable("elfa rate limited"),
        Err(ElfaError::CreditsExhausted) => signal_unavailable("elfa credits exhausted"),
        Err(e) => signal_unavailable(format!("elfa unavailable: {e}")),
    }
}

/// Emit the struct, constructor, and full `impl Tool` for an Elfa tool.
///
/// Two forms (xvision-im2r.9):
///   * `elfa_tool!($ty, $name, $desc, global: $endpoint)` — no input; calls
///     `$endpoint` with no query params (TrendingTokens, TrendingNarratives).
///   * `elfa_tool!($ty, $name, $desc, asset_ticker: $endpoint)` — expects an
///     `asset` JSON field; strips the pair-suffix, uppercases to a bare ticker,
///     calls `$endpoint?ticker=<TICKER>` (SmartMentions).
macro_rules! elfa_tool {
    // ── Global endpoint (no input) ──────────────────────────────────────────
    ($ty:ident, $name:literal, $desc:literal, global: $endpoint:literal) => {
        pub struct $ty {
            client: Arc<ElfaClient>,
        }
        impl $ty {
            pub fn new(client: Arc<ElfaClient>) -> Self {
                Self { client }
            }
            #[cfg(test)]
            pub fn for_test(base_url: String) -> Self {
                Self {
                    client: Arc::new(ElfaClient::new(base_url, "test".into(), 60)),
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
                        "properties": {},
                        "additionalProperties": true
                    }),
                    output_schema: json!({ "type": "object", "additionalProperties": true }),
                    timeout_ms: 15_000,
                    side_effect_level: SideEffectLevel::ExternalRead,
                    requires_approval: false,
                }
            }
            async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
                Ok(elfa_invoke(&self.client, $endpoint, &[]).await)
            }
        }
    };

    // ── Asset-ticker endpoint (expects `asset` field → bare ticker) ─────────
    ($ty:ident, $name:literal, $desc:literal, asset_ticker: $endpoint:literal) => {
        pub struct $ty {
            client: Arc<ElfaClient>,
        }
        impl $ty {
            pub fn new(client: Arc<ElfaClient>) -> Self {
                Self { client }
            }
            #[cfg(test)]
            pub fn for_test(base_url: String) -> Self {
                Self {
                    client: Arc::new(ElfaClient::new(base_url, "test".into(), 60)),
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
                #[derive(Deserialize)]
                struct AssetInput {
                    asset: String,
                }
                let parsed: AssetInput = match serde_json::from_value(input) {
                    Ok(v) => v,
                    Err(e) => return Ok(signal_unavailable(format!("bad input: {e}"))),
                };
                // Derive bare ticker: strip /USD or /USDT suffix, take part before '/',
                // uppercase.
                let ticker = parsed
                    .asset
                    .trim()
                    .split('/')
                    .next()
                    .unwrap_or(&parsed.asset)
                    .to_ascii_uppercase();
                Ok(elfa_invoke(&self.client, $endpoint, &[("ticker", &ticker)]).await)
            }
        }
    };
}

// ── ElfaSmartMentionsTool ─────────────────────────────────────────────────

elfa_tool!(
    ElfaSmartMentionsTool,
    "elfa_smart_mentions",
    "Top social mentions for a token ticker (KOL/smart money). Live only.",
    asset_ticker: "/v2/data/top-mentions"
);

// ── ElfaTrendingTokensTool ────────────────────────────────────────────────

elfa_tool!(
    ElfaTrendingTokensTool,
    "elfa_trending_tokens",
    "Global trending tokens across social/KOL channels. Live only.",
    global: "/v2/aggregations/trending-tokens"
);

// ── ElfaTrendingNarrativesTool ────────────────────────────────────────────

elfa_tool!(
    ElfaTrendingNarrativesTool,
    "elfa_trending_narratives",
    "Trending narratives and themes across crypto social channels. Live only.",
    global: "/v2/data/trending-narratives"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[tokio::test]
    async fn smart_mentions_name_and_descriptor() {
        let t = ElfaSmartMentionsTool::for_test("http://unused".into());
        assert_eq!(t.name().as_str(), "elfa_smart_mentions");
        assert_eq!(
            t.descriptor().side_effect_level,
            xvision_agent_client::protocol::SideEffectLevel::ExternalRead
        );
    }

    #[tokio::test]
    async fn smart_mentions_hits_live_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v2/data/top-mentions")
            .match_query(mockito::Matcher::UrlEncoded("ticker".into(), "BTC".into()))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = ElfaSmartMentionsTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"BTC"})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn trending_tokens_hits_global_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v2/aggregations/trending-tokens")
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = ElfaTrendingTokensTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn trending_narratives_hits_global_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v2/data/trending-narratives")
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = ElfaTrendingNarrativesTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn smart_mentions_strips_usd_suffix() {
        let mut server = mockito::Server::new_async().await;
        // Expect "ETH" not "ETH/USD"
        let m = server
            .mock("GET", "/v2/data/top-mentions")
            .match_query(mockito::Matcher::UrlEncoded("ticker".into(), "ETH".into()))
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let t = ElfaSmartMentionsTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"eth/USD"})).await.unwrap();
        assert!(out.get("data").is_some());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn rate_limit_degrades_gracefully() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v2/data/top-mentions")
            .match_query(mockito::Matcher::Any)
            .with_status(429)
            .create_async()
            .await;
        let t = ElfaSmartMentionsTool::for_test(server.url());
        let out = t.invoke(serde_json::json!({"asset":"BTC"})).await.unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("rate limited"));
        m.assert_async().await;
    }
}
