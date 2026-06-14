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
    /// JSON schema for the structured decision the agent submits via the
    /// built-in `submit_decision` lifecycle tool. Required by the sidecar
    /// whenever `allowed_tools` contains `submit_decision`. Additive: omitted
    /// for runs that don't use the lifecycle tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_schema: Option<serde_json::Value>,
    /// When `true`, the sidecar records a `TrajectoryFrame` for every model
    /// request, streamed delta, tool result, usage update, and finish marker
    /// and emits each over the event socket as an `event.trajectory_frame`
    /// notification. The Rust client routes those to a
    /// [`crate::event_sink::TrajectoryFramePersister`] backed by a
    /// `TrajectoryStore`.
    ///
    /// Defaults to `false` (and is skipped on the wire) so existing
    /// non-recording callers behave exactly as before. Set to `true` only
    /// when the caller has spawned the client with a trajectory store +
    /// recording id (see `AgentClient::spawn_with_event_sink`); recording
    /// without a persister produces frames the client drops.
    #[serde(skip_serializing_if = "is_false")]
    pub record: bool,
    /// Slot role stamped on every recorded trajectory frame (`slot_role` in the
    /// frame envelope), so the Rust consumer keys frames to the matching
    /// recording's `TrajectoryKey.slot_role`. Only meaningful when
    /// `record == true`; the sidecar defaults to `"default"` when omitted.
    /// Skipped on the wire when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_role: Option<String>,
    /// Reasoning effort hint forwarded to the provider gateway for CoT
    /// reasoning models (deepseek-r1, qwq, etc.). Accepted values are
    /// provider-specific strings (e.g. `"low"`, `"medium"`, `"high"`); the
    /// sidecar passes the value through verbatim. `None` (omitted on the
    /// wire) for non-CoT models and for cases where the operator wants the
    /// provider default. Set via
    /// `crate::agents::model::default_reasoning_effort(model_id)` at the
    /// Cline dispatch site.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
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
    /// JSON the agent submitted via the `submit_decision` lifecycle tool, if it
    /// called the tool. Additive: omitted when the agent didn't submit a
    /// decision (e.g. budget abort before submission).
    #[serde(default)]
    pub decision_json: Option<String>,
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

/// Params for the `session.replay_load` JSON-RPC method.
///
/// The caller sends a recorded run id and the full ordered list of
/// trajectory frames (serialized as raw JSON values so this crate
/// does not impose a particular deserialization shape on the sidecar).
/// In practice the values are `TrajectoryFrame` variants from
/// `xvision-observability` serialized with their `#[serde(tag = "kind")]`
/// discriminator, but the wire representation is `serde_json::Value` so
/// callers can pass pre-serialized blobs without round-tripping.
///
/// Wire shape:
/// ```json
/// {
///   "run_id": "01J...",
///   "frames": [
///     { "kind": "Request",  "ts_ms": 1, "messages": [], "tools": [], "system_prompt": "..." },
///     { "kind": "TextDelta", "ts_ms": 2, "text": "..." },
///     { "kind": "Usage",    "ts_ms": 3, "input_tokens": 10, ... },
///     { "kind": "Finish",   "ts_ms": 4, "reason": "stop" }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct ReplayLoadParams {
    pub run_id: String,
    /// Ordered trajectory frames.  Each element is a JSON object with a
    /// `"kind"` discriminator field (matching the `TrajectoryFrame` serde tag).
    pub frames: Vec<serde_json::Value>,
}

/// Result from `session.replay_load`.
///
/// `loaded` is the count of frames the sidecar accepted and stored for
/// subsequent `session.step` replay.  A mismatch with the number of
/// frames sent should be treated as a protocol error by the caller.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplayLoadResult {
    /// Number of frames loaded by the sidecar.  Defaults to 0 if the
    /// field is absent (forward-compat: older sidecars may omit it).
    #[serde(default)]
    pub loaded: usize,
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
            decision_json: None,
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
    fn step_result_carries_decision_json_when_present() {
        let v: StepResult = serde_json::from_str(
            r#"{
                "status": "completed",
                "output_text": "",
                "iterations": 1,
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0
                },
                "decision_json": "{\"action\":\"buy\"}"
            }"#,
        )
        .expect("step result with decision_json must deserialize");
        assert_eq!(v.decision_json.as_deref(), Some("{\"action\":\"buy\"}"));
    }

    #[test]
    fn start_run_params_skips_decision_schema_when_none() {
        let base = StartRunParams {
            run_id: "r".into(),
            provider_id: "anthropic".into(),
            model_id: "m".into(),
            api_key: None,
            base_url: None,
            system_prompt: "s".into(),
            allowed_tools: vec!["submit_decision".into()],
            budget_limits: BudgetLimits {
                max_input_tokens: 1,
                max_output_tokens: 1,
                max_wall_ms: 1,
            },
            decision_schema: Some(serde_json::json!({"type": "object"})),
            record: false,
            slot_role: None,
            reasoning_effort: None,
        };
        let with = serde_json::to_value(&base).unwrap();
        assert!(with.get("decision_schema").is_some());

        let without = StartRunParams {
            decision_schema: None,
            ..base
        };
        let v = serde_json::to_value(&without).unwrap();
        assert!(
            v.get("decision_schema").is_none(),
            "None must be skipped on the wire"
        );
    }

    #[test]
    fn start_run_params_skips_record_when_false_emits_when_true() {
        let base = StartRunParams {
            run_id: "r".into(),
            provider_id: "anthropic".into(),
            model_id: "m".into(),
            api_key: None,
            base_url: None,
            system_prompt: "s".into(),
            allowed_tools: vec!["echo".into()],
            budget_limits: BudgetLimits {
                max_input_tokens: 1,
                max_output_tokens: 1,
                max_wall_ms: 1,
            },
            decision_schema: None,
            record: false,
            slot_role: None,
            reasoning_effort: None,
        };
        // Default false: omitted from the wire so existing sidecars/tests
        // see exactly the pre-record shape.
        let off = serde_json::to_value(&base).unwrap();
        assert!(
            off.get("record").is_none(),
            "record=false must be skipped on the wire"
        );
        assert!(
            off.get("slot_role").is_none(),
            "slot_role=None must be skipped on the wire"
        );

        // Recording on: the field is present and true; slot_role rides along
        // exactly as the engine sets it.
        let on = serde_json::to_value(&StartRunParams {
            record: true,
            slot_role: Some("trader".into()),
            ..base
        })
        .unwrap();
        assert_eq!(on.get("record"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(
            on.get("slot_role"),
            Some(&serde_json::Value::String("trader".into()))
        );
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

    // ------------------------------------------------------------------
    // ReplayLoadParams / ReplayLoadResult
    // ------------------------------------------------------------------

    #[test]
    fn replay_load_params_serializes_to_expected_shape() {
        let params = ReplayLoadParams {
            run_id: "01HZ".into(),
            frames: vec![
                serde_json::json!({ "kind": "Request", "ts_ms": 1, "messages": [], "tools": [], "system_prompt": "sp" }),
                serde_json::json!({ "kind": "TextDelta", "ts_ms": 2, "text": "hello" }),
                serde_json::json!({ "kind": "Usage", "ts_ms": 3, "input_tokens": 10, "output_tokens": 2,
                                    "cache_read_tokens": 0, "cache_write_tokens": 0, "total_cost": 0.0 }),
                serde_json::json!({ "kind": "Finish", "ts_ms": 4, "reason": "stop" }),
            ],
        };
        let v = serde_json::to_value(&params).unwrap();

        // run_id field present
        assert_eq!(v["run_id"], "01HZ");

        // frames array with correct length and kind tags
        let frames = v["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0]["kind"], "Request");
        assert_eq!(frames[1]["kind"], "TextDelta");
        assert_eq!(frames[2]["kind"], "Usage");
        assert_eq!(frames[3]["kind"], "Finish");
    }

    #[test]
    fn replay_load_result_deserializes_with_loaded_field() {
        let json = r#"{ "loaded": 4 }"#;
        let r: ReplayLoadResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.loaded, 4);
    }

    #[test]
    fn replay_load_result_defaults_loaded_to_zero_when_absent() {
        // Forward-compat: a sidecar that omits the field should parse cleanly.
        let json = r#"{}"#;
        let r: ReplayLoadResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.loaded, 0);
    }

    #[test]
    fn replay_load_params_round_trip_preserves_frame_values() {
        // The frames Vec<Value> must pass through serialization without mutation.
        let original_frame = serde_json::json!({
            "kind": "ToolCallDelta",
            "ts_ms": 99,
            "tool_call_id": "c1",
            "tool_name": "submit_decision",
            "input": { "action": "buy", "qty": 1.0 }
        });
        let params = ReplayLoadParams {
            run_id: "r2".into(),
            frames: vec![original_frame.clone()],
        };
        let serialized = serde_json::to_value(&params).unwrap();
        let frame_back = &serialized["frames"][0];
        assert_eq!(frame_back, &original_frame);
    }

    #[test]
    fn replay_load_params_with_empty_frames_is_valid() {
        // An empty frame list is technically valid on the wire (corrupt recording
        // detection is the sidecar's job).
        let params = ReplayLoadParams {
            run_id: "empty".into(),
            frames: vec![],
        };
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["frames"].as_array().unwrap().len(), 0);
    }
}
