use async_trait::async_trait;

use crate::tools::{Tool, ToolName};

pub struct IndicatorPanelTool;

#[async_trait]
impl Tool for IndicatorPanelTool {
    fn name(&self) -> ToolName {
        ToolName::new("indicator_panel")
    }

    fn description(&self) -> &'static str {
        "Computed indicator panel for an asset"
    }

    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({"stub": true, "tool": "indicator_panel"}))
    }
}
