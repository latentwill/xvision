use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LLMSlot {
    pub role: String,               // "regime", "intern", "trader"
    pub prompt: String,             // slot prompt body
    pub model_requirement: String,  // e.g., "anthropic.claude-sonnet-4.6+"
    pub allowed_tools: Vec<String>, // tool names from registry
}
