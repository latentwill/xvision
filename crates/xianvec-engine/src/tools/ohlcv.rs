use async_trait::async_trait;
use serde::Deserialize;

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

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: OhlcvRequest = serde_json::from_value(input)?;
        let fixture = req.fixture.ok_or_else(|| {
            anyhow::anyhow!("MVP requires a fixture name; live Alpaca fetch lands in Plan #2")
        })?;
        let bars = xianvec_data::fixtures::load_ohlcv_fixture(&fixture, &req.asset, req.lookback_bars)?;
        Ok(serde_json::json!({"asset": req.asset, "bars": bars}))
    }
}
