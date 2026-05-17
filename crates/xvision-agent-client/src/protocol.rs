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
    /// On `status == "aborted"`, this is a machine-readable reason code.
    /// For budget-enforcement-driven aborts the sidecar emits one of the
    /// `BUDGET_*` constants below (`budget_wall_ms_exceeded`,
    /// `budget_input_tokens_exceeded`, `budget_output_tokens_exceeded`).
    /// For other aborts/failures it is a free-form message from the SDK.
    /// The field is additive: existing happy-path responses omit it.
    #[serde(default)]
    pub error: Option<String>,
}

/// Reason code surfaced on `StepResult.error` when the sidecar aborted a
/// step because the run's `max_wall_ms` budget was exhausted.
pub const BUDGET_WALL_MS_EXCEEDED: &str = "budget_wall_ms_exceeded";

/// Reason code surfaced on `StepResult.error` when the sidecar aborted a
/// step because the run's cumulative `max_input_tokens` was exhausted.
pub const BUDGET_INPUT_TOKENS_EXCEEDED: &str = "budget_input_tokens_exceeded";

/// Reason code surfaced on `StepResult.error` when the sidecar aborted a
/// step because the run's cumulative `max_output_tokens` was exhausted.
pub const BUDGET_OUTPUT_TOKENS_EXCEEDED: &str = "budget_output_tokens_exceeded";

impl StepResult {
    /// True iff the step aborted because a budget cap (wall-clock or
    /// token) was exhausted. Callers can use this to distinguish budget
    /// exhaustion from other terminal statuses without string-matching
    /// at the call site.
    #[must_use]
    pub fn is_budget_aborted(&self) -> bool {
        if self.status != "aborted" {
            return false;
        }
        matches!(
            self.error.as_deref(),
            Some(BUDGET_WALL_MS_EXCEEDED)
                | Some(BUDGET_INPUT_TOKENS_EXCEEDED)
                | Some(BUDGET_OUTPUT_TOKENS_EXCEEDED)
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EndRunParams {
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndRunResult {
    pub ended: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(status: &str, error: Option<&str>) -> StepResult {
        StepResult {
            status: status.to_string(),
            output_text: String::new(),
            iterations: 0,
            usage: RunUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                total_cost: None,
            },
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn step_result_deserializes_without_error_field() {
        // Happy path: existing sidecar responses omit `error` entirely.
        let v: StepResult = serde_json::from_str(
            r#"{
                "status": "completed",
                "output_text": "ok",
                "iterations": 1,
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0
                }
            }"#,
        )
        .expect("happy path step result must deserialize");
        assert_eq!(v.status, "completed");
        assert!(v.error.is_none());
        assert!(!v.is_budget_aborted());
    }

    #[test]
    fn step_result_deserializes_budget_aborted_reasons() {
        for code in [
            BUDGET_WALL_MS_EXCEEDED,
            BUDGET_INPUT_TOKENS_EXCEEDED,
            BUDGET_OUTPUT_TOKENS_EXCEEDED,
        ] {
            let json = format!(
                r#"{{
                    "status": "aborted",
                    "output_text": "",
                    "iterations": 0,
                    "usage": {{
                        "input_tokens": 0,
                        "output_tokens": 0,
                        "cache_read_tokens": 0,
                        "cache_write_tokens": 0
                    }},
                    "error": "{code}"
                }}"#
            );
            let v: StepResult = serde_json::from_str(&json).expect("must deserialize");
            assert_eq!(v.status, "aborted");
            assert_eq!(v.error.as_deref(), Some(code));
            assert!(v.is_budget_aborted(), "code {code} should be budget-aborted");
        }
    }

    #[test]
    fn is_budget_aborted_rejects_non_budget_aborts() {
        // Other abort reasons (SDK-internal errors, user cancels) must
        // not be mistaken for budget exhaustion.
        let r = sample("aborted", Some("user_cancelled"));
        assert!(!r.is_budget_aborted());

        let r = sample("completed", Some(BUDGET_WALL_MS_EXCEEDED));
        assert!(!r.is_budget_aborted());

        let r = sample("failed", None);
        assert!(!r.is_budget_aborted());
    }
}
