use async_trait::async_trait;
use serde::Deserialize;

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

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: PanelRequest = serde_json::from_value(input)?;
        let panel = xvision_data::compute_panel_from_fixture(&req.fixture, &req.asset, req.lookback_bars)?;
        Ok(serde_json::to_value(panel)?)
    }
}
