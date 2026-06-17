pub mod elfa;
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

/// Build the descriptor list to advertise to the sidecar for a given run,
/// using the PASSED registry (which includes any configured Nansen/Elfa
/// signal tools) rather than the static builtins-only list.  Used by
/// `spawn_cline_ctx` so the sidecar knows about every tool the dispatch
/// can actually serve for this run.
pub fn sidecar_descriptors(registry: &ToolRegistry) -> Vec<ToolDescriptor> {
    let mut descriptors = registry.all_descriptors();
    descriptors.push(submit_decision_descriptor());
    descriptors.push(submit_risk_verdict_descriptor());
    descriptors.sort_by(|a, b| a.name.cmp(&b.name));
    descriptors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_descriptors_includes_nansen_tool_when_registered() {
        let nansen_client = std::sync::Arc::new(xvision_data::nansen::NansenClient::new(
            "http://x".into(),
            "t".into(),
            300,
        ));
        let mut reg = ToolRegistry::default_with_builtins();
        reg.register_signal_tools(Some(nansen_client), None);

        let descs = sidecar_descriptors(&reg);
        let names: Vec<&str> = descs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"nansen_smart_money_flow"),
            "sidecar_descriptors must include nansen_smart_money_flow, got: {names:?}"
        );
        // Also has the two lifecycle tools.
        assert!(names.contains(&"submit_decision"));
        assert!(names.contains(&"submit_risk_verdict"));
    }

    #[test]
    fn built_in_tool_descriptors_does_not_include_nansen() {
        let names: Vec<String> = built_in_tool_descriptors().into_iter().map(|d| d.name).collect();
        assert!(
            !names.iter().any(|n| n.starts_with("nansen_")),
            "built_in_tool_descriptors must not include nansen tools, got: {names:?}"
        );
        // Still has ohlcv + submit_decision.
        assert!(names.contains(&"ohlcv".to_string()));
        assert!(names.contains(&"submit_decision".to_string()));
    }
}

pub struct ToolRegistry {
    tools: HashMap<ToolName, Arc<dyn Tool>>,
    /// Resolved signal-tool configuration, populated by `build_tool_registry`
    /// once per run start and read by `spawn_cline_ctx` so that `xvn.toml` is
    /// parsed exactly once per run (xvision-im2r.6). `None` in test registries
    /// and the builtins-only registry.
    pub signal_cfg: Option<Arc<signal_policy::SignalToolConfig>>,
}

impl ToolRegistry {
    pub fn empty() -> Self {
        Self {
            tools: HashMap::new(),
            signal_cfg: None,
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

    /// Register Nansen and/or Elfa signal tools when clients are configured.
    /// No-op for either when the corresponding `Option` is `None`.
    pub fn register_signal_tools(
        &mut self,
        nansen: Option<std::sync::Arc<xvision_data::nansen::NansenClient>>,
        elfa_client: Option<std::sync::Arc<xvision_data::elfa::ElfaClient>>,
    ) {
        if let Some(c) = nansen {
            self.register(std::sync::Arc::new(nansen::NansenSmartMoneyFlowTool::new(
                c.clone(),
            )));
            self.register(std::sync::Arc::new(nansen::NansenTokenScreenerTool::new(
                c.clone(),
            )));
            self.register(std::sync::Arc::new(nansen::NansenFlowIntelTool::new(c)));
        }
        if let Some(c) = elfa_client {
            self.register(std::sync::Arc::new(elfa::ElfaSmartMentionsTool::new(c.clone())));
            self.register(std::sync::Arc::new(elfa::ElfaTrendingTokensTool::new(c.clone())));
            self.register(std::sync::Arc::new(elfa::ElfaTrendingNarrativesTool::new(c)));
        }
    }
}
