use async_trait::async_trait;
use serde::Deserialize;
use xvision_agent_client::{SideEffectLevel, ToolDescriptor};

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct PanelRequest {
    asset: String,
    fixture: String,
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
                        "description": "Fixture name for backtest data"
                    },
                    "lookback_bars": {
                        "type": "integer",
                        "default": 200,
                        "description": "Number of bars to compute indicators over (default 200)"
                    }
                },
                "required": ["asset", "fixture"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "description": "Latest indicator values keyed by indicator name",
                "additionalProperties": { "type": "number" }
            }),
            timeout_ms: 10_000,
            side_effect_level: SideEffectLevel::ReadOnly,
            requires_approval: false,
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: PanelRequest = serde_json::from_value(input)?;
        let panel = xvision_data::compute_panel_from_fixture(&req.fixture, &req.asset, req.lookback_bars)?;
        Ok(serde_json::to_value(panel)?)
    }
}
