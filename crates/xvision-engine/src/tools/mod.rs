pub mod indicators;
pub mod ohlcv;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use xvision_agent_client::{SideEffectLevel, ToolDescriptor};

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
    /// Full descriptor for agentd's tool registry. Must match `name()` and `description()`.
    fn descriptor(&self) -> ToolDescriptor;
    /// JSON in, JSON out. Schema is documented per-tool.
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}

/// Built-in lifecycle tools handled entirely by agentd (no Rust callback).
/// The model calls these; agentd captures the payload in StepResult fields.
/// `submit_decision` and `submit_risk_verdict` must always be registered so
/// the model sees their schemas. The `decision_schema` in StartRunParams
/// overrides the submit_decision input schema at run time.
pub fn built_in_tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "submit_decision".to_string(),
            version: "1.0.0".to_string(),
            description: "Submit a trading decision for this cycle. Call this exactly once \
                          when you have evaluated the market inputs. The decision schema \
                          is provided at run start and specifies the required fields \
                          (action, conviction, justification, and any strategy-specific fields)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "description": "Trading decision payload. Schema is injected per-run via decision_schema.",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Trading action (e.g. long_open, short_open, flat, hold)"
                    },
                    "conviction": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Confidence in this decision (0–1)"
                    },
                    "justification": {
                        "type": "string",
                        "description": "One-line reasoning for the decision"
                    }
                },
                "required": ["action", "conviction", "justification"],
                "additionalProperties": true
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "accepted": { "type": "boolean" }
                }
            }),
            timeout_ms: 5_000,
            side_effect_level: SideEffectLevel::ExternalWrite,
            requires_approval: false,
        },
        ToolDescriptor {
            name: "submit_risk_verdict".to_string(),
            version: "1.0.0".to_string(),
            description: "Submit a risk-gate verdict on a proposed trading decision. \
                          Call this exactly once after reviewing the proposed action. \
                          Returns approve, modify, or veto."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "verdict": {
                        "type": "string",
                        "enum": ["approve", "modify", "veto"],
                        "description": "Risk gate verdict"
                    },
                    "modified_action": {
                        "type": "string",
                        "description": "Replacement action when verdict is modify (optional)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Explanation for the verdict"
                    }
                },
                "required": ["verdict", "reason"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "accepted": { "type": "boolean" }
                }
            }),
            timeout_ms: 5_000,
            side_effect_level: SideEffectLevel::ExternalWrite,
            requires_approval: false,
        },
    ]
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

    /// Descriptors for Rust callback tools (ohlcv, indicator_panel, etc.).
    pub fn to_descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.values().map(|t| t.descriptor()).collect()
    }

    /// All descriptors: Rust callback tools + built-in lifecycle tools.
    /// Pass this to `AgentClient::register_tools()` before starting any run.
    pub fn all_descriptors(&self) -> Vec<ToolDescriptor> {
        let mut all = self.to_descriptors();
        all.extend(built_in_tool_descriptors());
        all
    }
}
