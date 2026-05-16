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
    pub cline_sdk_version: String,
    pub status: String,
}

pub const SUPPORTED_PROTOCOL_VERSION: &str = "0.1.0";
