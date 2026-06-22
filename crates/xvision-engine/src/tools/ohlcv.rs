use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct OhlcvRequest {
    asset: String,
    #[serde(default)]
    fixture: Option<String>,
    #[serde(default)]
    timeframe: Option<String>,
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
            version: "1".to_string(),
            description: self.description().to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "asset": {"type": "string"},
                    "fixture": {"type": "string"},
                    "timeframe": {"type": "string"},
                    "lookback_bars": {"type": "integer", "minimum": 1, "default": 200}
                },
                "required": ["asset"],
                "additionalProperties": false
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "asset": {"type": "string"},
                    "bars": {"type": "array"}
                },
                "required": ["asset", "bars"],
                "additionalProperties": true
            }),
            timeout_ms: 10_000,
            side_effect_level: SideEffectLevel::ReadOnly,
            requires_approval: false,
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: OhlcvRequest = serde_json::from_value(input)?;
        if req.timeframe.is_some() {
            anyhow::bail!(
                "timeframe-specific OHLCV requests require run-scoped market data; fixture-backed ohlcv only serves the fixture's native bars"
            );
        }
        let fixture = req.fixture.ok_or_else(|| {
            anyhow::anyhow!("MVP requires a fixture name; live Alpaca fetch lands in Plan #2")
        })?;
        let bars = xvision_data::fixtures::load_ohlcv_fixture(&fixture, &req.asset, req.lookback_bars)?;
        let mut out = serde_json::json!({"asset": req.asset, "bars": bars});
        if let Some(timeframe) = req.timeframe {
            out["timeframe"] = serde_json::Value::String(timeframe);
        }
        Ok(out)
    }
}
