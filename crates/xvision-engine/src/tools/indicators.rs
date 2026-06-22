use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct PanelRequest {
    asset: String,
    fixture: String,
    #[serde(default)]
    timeframe: Option<String>,
    #[serde(default = "default_lookback")]
    lookback_bars: usize,
}

fn default_lookback() -> usize {
    200
}

pub struct IndicatorPanelTool;

#[async_trait]
impl Tool for IndicatorPanelTool {
    fn name(&self) -> ToolName {
        ToolName::new("indicator_panel")
    }

    fn description(&self) -> &'static str {
        "Computed indicator panel (RSI, MACD, BB, ATR, MA, EMA)"
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
                "required": ["asset", "fixture"],
                "additionalProperties": false
            }),
            output_schema: json!({
                "type": "object",
                "additionalProperties": true
            }),
            timeout_ms: 10_000,
            side_effect_level: SideEffectLevel::ReadOnly,
            requires_approval: false,
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: PanelRequest = serde_json::from_value(input)?;
        let panel = xvision_data::compute_panel_from_fixture(&req.fixture, &req.asset, req.lookback_bars)?;
        let mut out = serde_json::to_value(panel)?;
        if let Some(timeframe) = req.timeframe {
            if let Some(obj) = out.as_object_mut() {
                obj.insert("timeframe".to_string(), serde_json::Value::String(timeframe));
            }
        }
        Ok(out)
    }
}
