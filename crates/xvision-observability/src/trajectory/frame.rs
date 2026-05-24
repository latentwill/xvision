//! Versioned trajectory frame schema (items 1 + 4 schema).
//!
//! `TrajectoryFrame` is the Rust mirror of the TypeScript `AgentModelEvent`
//! union from `xvision-agentd`, extended with the request frame, tool-result
//! frame, retry/cancel decisions, and the finish marker.
//!
//! ## Versioning
//!
//! The schema version travels with the *recording* (Task 1 —
//! `TRAJECTORY_SCHEMA_VERSION`).  Bump that constant on any change to this
//! enum or to the `TrajectoryKey` fingerprint fields.
//!
//! ## Serde tag
//!
//! All variants use `#[serde(tag = "kind")]` so the wire format has a
//! human-readable discriminator field instead of a single-key object.
//! This matches the TypeScript `frame-types.ts` shape exactly.

/// One frame in a recorded agent trajectory.
///
/// Variants cover the full life of one model call inside a slot:
/// `Request` → streaming deltas → `ToolCallDelta`/`ToolResult` cycles
/// → `Usage` accounting → `RetryOrCancel`? → `Finish`.
///
/// All variants carry `ts_ms` (milliseconds since Unix epoch) for ordering
/// across slots and steps.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum TrajectoryFrame {
    /// The full request sent to the model (messages, tools, system prompt).
    Request {
        ts_ms: u64,
        messages: serde_json::Value,
        tools: serde_json::Value,
        system_prompt: Option<String>,
    },
    /// A streamed text token from the model.
    TextDelta {
        ts_ms: u64,
        text: String,
    },
    /// A streamed reasoning/thinking token (extended thinking models).
    ReasoningDelta {
        ts_ms: u64,
        text: String,
    },
    /// A streamed tool-call delta (may arrive in multiple chunks).
    ToolCallDelta {
        ts_ms: u64,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        input: Option<serde_json::Value>,
    },
    /// The result (or error) returned to the model for a tool call.
    ToolResult {
        ts_ms: u64,
        tool_call_id: String,
        output: serde_json::Value,
        error: Option<String>,
    },
    /// Token + cost accounting at the end of the model call.
    Usage {
        ts_ms: u64,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_write_tokens: u32,
        total_cost: f64,
    },
    /// A retry or cancel decision made by the harness (item 1 — required for
    /// replay determinism: the replayer must know that a retry happened so it
    /// can re-issue rather than using the cached response).
    RetryOrCancel {
        ts_ms: u64,
        reason: String,
    },
    /// The model call is done (either normally or with an error).
    Finish {
        ts_ms: u64,
        reason: String,
        error: Option<String>,
    },
}

impl TrajectoryFrame {
    /// Millisecond timestamp — used for ordering checks.
    pub fn ts_ms(&self) -> u64 {
        match self {
            Self::Request { ts_ms, .. } => *ts_ms,
            Self::TextDelta { ts_ms, .. } => *ts_ms,
            Self::ReasoningDelta { ts_ms, .. } => *ts_ms,
            Self::ToolCallDelta { ts_ms, .. } => *ts_ms,
            Self::ToolResult { ts_ms, .. } => *ts_ms,
            Self::Usage { ts_ms, .. } => *ts_ms,
            Self::RetryOrCancel { ts_ms, .. } => *ts_ms,
            Self::Finish { ts_ms, .. } => *ts_ms,
        }
    }

    /// The `kind` tag string as it appears in serialized JSON.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Request { .. } => "Request",
            Self::TextDelta { .. } => "TextDelta",
            Self::ReasoningDelta { .. } => "ReasoningDelta",
            Self::ToolCallDelta { .. } => "ToolCallDelta",
            Self::ToolResult { .. } => "ToolResult",
            Self::Usage { .. } => "Usage",
            Self::RetryOrCancel { .. } => "RetryOrCancel",
            Self::Finish { .. } => "Finish",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frames() -> Vec<TrajectoryFrame> {
        vec![
            TrajectoryFrame::Request {
                ts_ms: 1_000,
                messages: serde_json::json!([{"role": "user", "content": "hi"}]),
                tools: serde_json::json!([]),
                system_prompt: Some("you are a trader".into()),
            },
            TrajectoryFrame::TextDelta {
                ts_ms: 1_001,
                text: "Analyzing...".into(),
            },
            TrajectoryFrame::ReasoningDelta {
                ts_ms: 1_002,
                text: "The market is trending up.".into(),
            },
            TrajectoryFrame::ToolCallDelta {
                ts_ms: 1_003,
                tool_call_id: Some("call_001".into()),
                tool_name: Some("ohlcv".into()),
                input: Some(serde_json::json!({"symbol": "BTC"})),
            },
            TrajectoryFrame::ToolResult {
                ts_ms: 1_004,
                tool_call_id: "call_001".into(),
                output: serde_json::json!({"open": 60000}),
                error: None,
            },
            TrajectoryFrame::ToolResult {
                ts_ms: 1_005,
                tool_call_id: "call_002".into(),
                output: serde_json::json!(null),
                error: Some("timeout".into()),
            },
            TrajectoryFrame::Usage {
                ts_ms: 1_006,
                input_tokens: 120,
                output_tokens: 45,
                cache_read_tokens: 10,
                cache_write_tokens: 5,
                total_cost: 0.00234,
            },
            TrajectoryFrame::RetryOrCancel {
                ts_ms: 1_007,
                reason: "context_overflow".into(),
            },
            TrajectoryFrame::Finish {
                ts_ms: 1_008,
                reason: "stop".into(),
                error: None,
            },
            TrajectoryFrame::Finish {
                ts_ms: 1_009,
                reason: "error".into(),
                error: Some("upstream timeout".into()),
            },
        ]
    }

    #[test]
    fn frame_roundtrips_all_variants() {
        for f in sample_frames() {
            let json = serde_json::to_string(&f).unwrap();
            let back: TrajectoryFrame = serde_json::from_str(&json).unwrap();
            assert_eq!(f, back, "roundtrip failed for variant: {json}");
        }
    }

    #[test]
    fn kind_tag_is_present_in_json() {
        let f = TrajectoryFrame::TextDelta {
            ts_ms: 42,
            text: "hello".into(),
        };
        let json = serde_json::to_string(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["kind"], "TextDelta");
        assert_eq!(v["ts_ms"], 42);
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn ts_ms_accessor() {
        let f = TrajectoryFrame::Finish {
            ts_ms: 999,
            reason: "stop".into(),
            error: None,
        };
        assert_eq!(f.ts_ms(), 999);
    }

    #[test]
    fn kind_str_matches_serde_tag() {
        for f in sample_frames() {
            let json = serde_json::to_string(&f).unwrap();
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["kind"].as_str().unwrap(), f.kind_str());
        }
    }
}
