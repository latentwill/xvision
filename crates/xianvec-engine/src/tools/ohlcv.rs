use async_trait::async_trait;

use crate::tools::{Tool, ToolName};

pub struct OhlcvTool;

#[async_trait]
impl Tool for OhlcvTool {
    fn name(&self) -> ToolName {
        ToolName::new("ohlcv")
    }

    fn description(&self) -> &'static str {
        "OHLCV history for an asset and time range"
    }

    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // Real impl in Task 12 — for now, return a deterministic stub so registry tests pass.
        Ok(serde_json::json!({"stub": true, "tool": "ohlcv"}))
    }
}
