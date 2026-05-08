pub mod indicators;
pub mod ohlcv;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

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
    /// JSON in, JSON out. Schema is documented per-tool.
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value>;
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
}
