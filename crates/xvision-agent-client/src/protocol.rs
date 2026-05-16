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
