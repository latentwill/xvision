pub mod indicators;
pub mod nansen;
pub mod ohlcv;
pub mod signal_policy;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use xvision_agent_client::protocol::{SideEffectLevel, ToolDescriptor};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> ToolName;
    fn description(&self) -> &'static str;
    fn descriptor(&self) -> ToolDescriptor;
    /// JSON in, JSON out. Schema is documented per-tool.
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}

pub fn submit_decision_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: "submit_decision".to_string(),
        version: "1".to_string(),
        description: "Submit the final trading decision for the current cycle".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["long_open", "short_open", "long_close", "short_close", "hold"]
                },
                "size": {"type": "number"},
                "confidence": {"type": "number"},
                "rationale": {"type": "string"}
            },
            "required": ["action"],
            "additionalProperties": true
        }),
        output_schema: json!({
            "type": "object",
            "properties": {"accepted": {"type": "boolean"}},
            "required": ["accepted"],
            "additionalProperties": false
        }),
        timeout_ms: 5_000,
        side_effect_level: SideEffectLevel::Pure,
        requires_approval: false,
    }
}

pub fn submit_risk_verdict_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: "submit_risk_verdict".to_string(),
        version: "1".to_string(),
        description: "Submit a risk review verdict for a proposed trading action".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "verdict": {"type": "string", "enum": ["approve", "reject", "revise"]},
                "reason": {"type": "string"},
                "max_size": {"type": "number"}
            },
            "required": ["verdict"],
            "additionalProperties": true
        }),
        output_schema: json!({
            "type": "object",
            "properties": {"accepted": {"type": "boolean"}},
            "required": ["accepted"],
            "additionalProperties": false
        }),
        timeout_ms: 5_000,
        side_effect_level: SideEffectLevel::Pure,
        requires_approval: false,
    }
}

pub fn built_in_tool_descriptors() -> Vec<ToolDescriptor> {
    let registry = ToolRegistry::default_with_builtins();
    let mut descriptors = registry.all_descriptors();
    descriptors.push(submit_decision_descriptor());
    descriptors.push(submit_risk_verdict_descriptor());
    descriptors.sort_by(|a, b| a.name.cmp(&b.name));
    descriptors
}

pub struct ToolRegistry {
    tools: HashMap<ToolName, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn empty() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn default_with_builtins() -> Self {
        let mut r = Self::empty();
        r.register(Arc::new(ohlcv::OhlcvTool));
        r.register(Arc::new(indicators::IndicatorPanelTool));
        r
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }

    pub fn get(&self, name: &ToolName) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolName> {
        self.tools.keys().cloned().collect()
    }

    pub fn all_descriptors(&self) -> Vec<ToolDescriptor> {
        let mut descriptors: Vec<ToolDescriptor> =
            self.tools.values().map(|tool| tool.descriptor()).collect();
        descriptors.sort_by(|a, b| a.name.cmp(&b.name));
        descriptors
    }

    /// Register the Nansen signal tools when a client is configured. Elfa tools
    /// are added by a later task. No-op when `nansen` is `None`.
    pub fn register_signal_tools(
        &mut self,
        nansen: Option<std::sync::Arc<xvision_data::nansen::NansenClient>>,
    ) {
        if let Some(c) = nansen {
            self.register(std::sync::Arc::new(nansen::NansenSmartMoneyFlowTool::new(c.clone())));
            self.register(std::sync::Arc::new(nansen::NansenTokenScreenerTool::new(c.clone())));
            self.register(std::sync::Arc::new(nansen::NansenFlowIntelTool::new(c)));
        }
    }
}
