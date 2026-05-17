use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest<'a, P: Serialize> {
    pub jsonrpc: &'a str,
    pub id: u64,
    pub method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<P>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse<R> {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: Option<R>,
    pub error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeHealthResult {
    pub protocol_version: String,
    pub sidecar_version: String,
    // Protocol field name; must match `cline_sdk_version` in xvision-agentd.
    // Do not rename independently of the JSON-RPC spec.
    pub cline_sdk_version: String,
    pub status: String,
}

pub const SUPPORTED_PROTOCOL_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub timeout_ms: u32,
    pub side_effect_level: SideEffectLevel,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectLevel {
    Pure,
    ReadOnly,
    ExternalRead,
    ExternalWrite,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolRegistrySetParams {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegistrySetResult {
    pub count: usize,
    pub registry_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegistryGetResult {
    pub tools: Vec<ToolDescriptor>,
    pub registry_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetLimits {
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub max_wall_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartRunParams {
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub budget_limits: BudgetLimits,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRunResult {
    pub run_id: String,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepParams {
    pub run_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepResult {
    pub status: String,
    pub output_text: String,
    pub iterations: u32,
    pub usage: RunUsage,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndRunParams {
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndRunResult {
    pub ended: bool,
}
