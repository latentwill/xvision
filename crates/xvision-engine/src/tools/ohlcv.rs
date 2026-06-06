use async_trait::async_trait;
use serde::Deserialize;
use xvision_agent_client::{SideEffectLevel, ToolDescriptor};

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct OhlcvRequest {
    asset: String,
    #[serde(default)]
    fixture: Option<String>,
    #[serde(default = "default_lookback")]
    lookback_bars: usize,
}

fn default_lookback() -> usize {
    200
}

pub struct OhlcvTool;

#[async_trait]
impl Tool for OhlcvTool {
    fn name(&self) -> ToolName {
        ToolName::new("ohlcv")
    }

    fn description(&self) -> &'static str {
        "OHLCV history for an asset and time range"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.name().as_str().to_string(),
            version: "1.0.0".to_string(),
            description: self.description().to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "asset": {
                        "type": "string",
                        "description": "Asset symbol, e.g. BTC/USD"
                    },
                    "fixture": {
                        "type": "string",
                        "description": "Fixture name for backtest data (required in backtest mode)"
                    },
                    "lookback_bars": {
                        "type": "integer",
                        "default": 200,
                        "description": "Number of bars to return (default 200)"
                    }
                },
                "required": ["asset"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "asset": { "type": "string" },
                    "bars": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "ts": { "type": "string" },
                                "open": { "type": "number" },
                                "high": { "type": "number" },
                                "low": { "type": "number" },
                                "close": { "type": "number" },
                                "volume": { "type": "number" }
                            }
                        }
                    }
                }
            }),
            timeout_ms: 10_000,
            side_effect_level: SideEffectLevel::ReadOnly,
            requires_approval: false,
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: OhlcvRequest = serde_json::from_value(input)?;
        let fixture = req.fixture.ok_or_else(|| {
            anyhow::anyhow!("MVP requires a fixture name; live Alpaca fetch lands in Plan #2")
        })?;
        let bars = xvision_data::fixtures::load_ohlcv_fixture(&fixture, &req.asset, req.lookback_bars)?;
        Ok(serde_json::json!({"asset": req.asset, "bars": bars}))
    }
}
