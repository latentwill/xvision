//! Enums used across the agent_runs ledger. All variants serialize as the
//! exact SQLite text values produced by the recorder, so a column-string
//! comparison in SQL matches a Rust `RunStatus::Completed` etc. Mismatches
//! between the Rust enum and the migration's text vocabulary are a
//! production bug; the round-trip test in `tests/types_roundtrip.rs`
//! locks the mapping in.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    /// Sidecar crashed mid-run; retry produced a new attempt. The
    /// `xvision-agent-client` supervisor sets this when it gives up on the
    /// current sidecar attempt and asks the recorder to mark it.
    Interrupted,
    /// Cline `maxIterations` was hit without a `submit_decision` call —
    /// the agent never produced a terminal action.
    AgentFailure,
}

impl RunStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::AgentFailure => "agent_failure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    Cancelled,
    /// Sidecar crash mid-span. Resumed runs leave the previous span as
    /// `interrupted` and open a fresh one.
    Interrupted,
}

impl SpanStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    #[serde(rename = "agent.run")]
    AgentRun,
    #[serde(rename = "agent.plan")]
    AgentPlan,
    /// The model invocation that produces the trade decision. One per
    /// decision cycle, nested under the enclosing [`SpanKind::AgentDecision`]
    /// span. Renamed from `ModelCall` (`"model.call"`) — a single-stage
    /// trading agent has exactly one decision-producing model call per
    /// cycle (the per-slot trader/regime/filter ROLES were retired), so the
    /// span says what the model is doing rather than carrying a generic APM
    /// name. The wire/DB string is `"decision.model"`.
    #[serde(rename = "decision.model")]
    DecisionModel,
    /// Inline chain-of-thought captured from a model's `<think>…</think>`
    /// block before the engine strips it out of the raw text (so the
    /// `{…}` decision object the trader parses isn't shadowed by the
    /// reasoning trace). Emitted as a CHILD of the enclosing
    /// [`SpanKind::DecisionModel`] span by the Cline executor when a CoT
    /// model emits reasoning. The reasoning body is payload-gated exactly
    /// like the model-call prompt/response (blob-backed + redacted under
    /// `redacted`/`full_debug`, never stored under `hash_only`); the
    /// `reasoning_char_count` attribute is always recorded for cost
    /// legibility. Added by WS-17 (`trace-obs` reasoning capture) —
    /// previously the chain-of-thought was discarded at the strip site.
    /// Renamed from `ModelReasoning` (`"model.reasoning"`); the wire/DB
    /// string is `"decision.reasoning"`.
    #[serde(rename = "decision.reasoning")]
    DecisionReasoning,
    #[serde(rename = "tool.call")]
    ToolCall,
    #[serde(rename = "approval.request")]
    ApprovalRequest,
    #[serde(rename = "approval.response")]
    ApprovalResponse,
    #[serde(rename = "sandbox.exec")]
    SandboxExec,
    #[serde(rename = "supervisor.review")]
    SupervisorReview,
    #[serde(rename = "financial.eval")]
    FinancialEval,
    #[serde(rename = "artifact.write")]
    ArtifactWrite,
    #[serde(rename = "ipc.notification")]
    IpcNotification,
    #[serde(rename = "skill.invoke")]
    SkillInvoke,
    /// One broker submit → fill/reject cycle. Emitted by the eval
    /// executor (paper + real wires) wrapping every `submit_order`
    /// call. Carries side / qty / price / fill status / error class on
    /// the matching [`BrokerCallStartedEvent`] /
    /// [`BrokerCallFinishedEvent`] payload. Added by
    /// `qa-trace-broker-spans` (round-2 intake item #8/#14): broker
    /// activity was previously invisible on the trace dock.
    #[serde(rename = "broker.call")]
    BrokerCall,
    /// Pre-condition validation for a tool call. Brackets the open
    /// of a [`SpanKind::ToolCall`] span. Added by F-4 from the
    /// 2026-05-18 harness observability audit as the instrumentation
    /// seam for F-6's typed schema validator — F-4 emits these spans
    /// with a no-op body so the wire format and ordering are pinned
    /// before F-6 drops the actual validation in.
    #[serde(rename = "tool.validate_input")]
    ToolValidateInput,
    /// Post-condition validation for a tool call. Brackets the close
    /// of a [`SpanKind::ToolCall`] span. Emitted even when the tool
    /// call errored so the post-state is always recorded. Same
    /// no-op-body / F-6-fills-it-in arrangement as
    /// [`SpanKind::ToolValidateInput`].
    #[serde(rename = "tool.validate_output")]
    ToolValidateOutput,
    /// One attempt by the recovery state machine to repair a failed
    /// run. F-4 reserves the wire identifier; the variant is NOT
    /// emitted anywhere in the engine yet. F-5
    /// (`harness-recovery-state-machine`) owns emission — when it
    /// promotes `classify_run_failure` from regex-on-error-string to
    /// a typed `FailureClass` dispatcher, each transition through it
    /// emits a `recovery.attempt` span carrying the failure class
    /// and the retry index in the F-2 `SpanAttributes` bag.
    #[serde(rename = "recovery.attempt")]
    RecoveryAttempt,
    /// One change in run lifecycle status (e.g. `Queued → Running`,
    /// `Running → Completed`). Emitted as an instantaneous span
    /// (open + immediate close-ok) from
    /// `ObsEmitter::emit_state_transition`. Carries `{"from", "to"}`
    /// in `attributes_json` alongside the F-2 `SpanAttributes` bag
    /// so the trace dock can render a per-run state timeline without
    /// having to diff successive snapshots.
    #[serde(rename = "state.transition")]
    StateTransition,
    /// One per-decision pipeline iteration (briefing → trader output →
    /// fill). Emitted by the eval executor wrapping the per-bar slice
    /// so the trace dock can show a per-decision breakdown (LLM /
    /// tool / fill timing) without inferring boundaries from the run
    /// timeline. Added by F43 (`trace-dock-emitters`) — previously the
    /// trace dock only had run-level spans.
    #[serde(rename = "agent.decision")]
    AgentDecision,
    /// LLM filter-capability agent invocation. Emitted by the engine's
    /// dispatch_capability path around each `run_llm_filter` call. Carries
    /// `{asset, verdict: "pass"|"reject", reason?}` in `attributes_json`.
    #[serde(rename = "filter.eval")]
    FilterEval,
    /// Risk-gate evaluation. Emitted by the eval harness (`BacktestRunner`)
    /// and ObsEmitter around each `RiskLayer::evaluate` call. Carries
    /// `{verdict: "approved"|"modified"|"vetoed", veto_reason?}`.
    #[serde(rename = "risk.gate")]
    RiskGate,
}

impl SpanKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::AgentRun => "agent.run",
            Self::AgentPlan => "agent.plan",
            Self::DecisionModel => "decision.model",
            Self::DecisionReasoning => "decision.reasoning",
            Self::ToolCall => "tool.call",
            Self::ApprovalRequest => "approval.request",
            Self::ApprovalResponse => "approval.response",
            Self::SandboxExec => "sandbox.exec",
            Self::SupervisorReview => "supervisor.review",
            Self::FinancialEval => "financial.eval",
            Self::ArtifactWrite => "artifact.write",
            Self::IpcNotification => "ipc.notification",
            Self::SkillInvoke => "skill.invoke",
            Self::BrokerCall => "broker.call",
            Self::ToolValidateInput => "tool.validate_input",
            Self::ToolValidateOutput => "tool.validate_output",
            Self::RecoveryAttempt => "recovery.attempt",
            Self::StateTransition => "state.transition",
            Self::AgentDecision => "agent.decision",
            Self::FilterEval => "filter.eval",
            Self::RiskGate => "risk.gate",
        }
    }
}

/// Per the Cline SDK design's tool metadata: what side effects a tool can
/// have. Backtest mode rejects any tool with `ExternalWrite` unless the
/// strategy explicitly opts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectLevel {
    Pure,
    ReadOnly,
    ExternalRead,
    ExternalWrite,
}

impl SideEffectLevel {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::ReadOnly => "read_only",
            Self::ExternalRead => "external_read",
            Self::ExternalWrite => "external_write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    SafeRead,
    ExpensiveCompute,
    FileWrite,
    NetworkCall,
    StrategyMutation,
    RealTradeBlocked,
}

impl RiskLevel {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::SafeRead => "safe_read",
            Self::ExpensiveCompute => "expensive_compute",
            Self::FileWrite => "file_write",
            Self::NetworkCall => "network_call",
            Self::StrategyMutation => "strategy_mutation",
            Self::RealTradeBlocked => "real_trade_blocked",
        }
    }
}

/// Which provider-capability path produced the structured output for a
/// model call. Recorded per row so we can tell at audit time whether the
/// legacy schema-injection-in-system-prompt fallback fired vs. the modern
/// paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityPath {
    ToolChoice,
    ResponseFormat,
    SchemaInjection,
    StructuredOutput,
    StreamingToolCalls,
}

impl CapabilityPath {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::ToolChoice => "tool_choice",
            Self::ResponseFormat => "response_format",
            Self::SchemaInjection => "schema_injection",
            Self::StructuredOutput => "structured_output",
            Self::StreamingToolCalls => "streaming_tool_calls",
        }
    }
}

/// Where a tool came from. `Mcp(name)` is the server name; `Native` is a
/// xvision-owned Rust tool; `ClineBuiltin` is a Cline built-in (disabled by
/// default for trading agents per the Cline SDK spec).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolOrigin {
    Native,
    Mcp(String),
    ClineBuiltin,
}

impl ToolOrigin {
    pub fn as_db_string(&self) -> String {
        match self {
            Self::Native => "native".to_owned(),
            Self::Mcp(server) => format!("mcp:{server}"),
            Self::ClineBuiltin => "cline_builtin".to_owned(),
        }
    }

    /// Parse the DB column value back into a `ToolOrigin`. Returns `None`
    /// for unknown shapes so the caller can decide how to handle drift.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "native" => Some(Self::Native),
            "cline_builtin" => Some(Self::ClineBuiltin),
            other => other.strip_prefix("mcp:").map(|name| Self::Mcp(name.to_owned())),
        }
    }
}

/// Typed bag carried in `SpanStartedEvent.attributes_json`. All fields
/// optional so emission sites populate only what's in scope at the
/// call. Serializes to a flat JSON object with `skip_serializing_if`
/// so `None` fields are absent in the wire payload rather than
/// `"field": null` — keeps the recorded `attributes_json` payload
/// small and forward-compatible with new fields.
///
/// F-1 introduced real `prompt_hash` digests. F-2 (this struct) makes
/// those digests join-able with the per-span context they were
/// produced in. Subsequent harness tracks add their own fields here:
/// F-3 wires `prompt_version` from a new `agent_slots` column; F-4
/// adds `recovery.attempt` spans that thread `retry_count` through;
/// F-5 carries the typed `FailureClass` per recovery transition.
///
/// The struct is deliberately *not* `#[serde(deny_unknown_fields)]` on
/// the deserialize side — older recorded rows predate later additions
/// and must continue to parse cleanly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanAttributes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Slot role label ("regime" / "trader" / free-form
    /// per strategy). Sourced from `LLMSlot.role` at the engine
    /// dispatch site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i32>,
    /// Reserved for F-3 (`harness-prompt-version-field`). Until that
    /// migration lands the field is always `None` on emission; once
    /// `agent_slots.prompt_version` exists, the dispatch site will
    /// populate it from the slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
    /// Per-decision index inside an eval run, populated on
    /// `agent.decision` spans (and propagated to `tool.call` /
    /// `decision.model` child spans where the call site knows it). Added
    /// by F43 (`trace-dock-emitters`) so the dashboard can group
    /// per-decision spans without re-deriving the index from the bar
    /// timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_index: Option<i64>,
}

impl SpanAttributes {
    /// Serialize to the wire form stored in `attributes_json`. Returns
    /// `None` when every field is `None` so an entirely empty bag does
    /// not write `"{}"` and waste a row's worth of bytes. Panics
    /// cannot occur in practice — every field is a primitive Serialize
    /// impl — but on the off chance serialization fails we fall back
    /// to `None` rather than poison the span emission.
    pub fn to_attributes_json(&self) -> Option<String> {
        if self == &Self::default() {
            return None;
        }
        serde_json::to_string(self).ok()
    }

    /// Merge this typed bag into a pre-existing JSON object payload
    /// (e.g. the `broker_call` sub-object the broker span already
    /// carries). Fields with `Some` value are written at the top
    /// level; existing keys in `base` are preserved. Returns the
    /// serialized merged object.
    ///
    /// Used by the broker-call span site so the typed attributes
    /// coexist with the broker-specific `broker_call` sub-object
    /// without nesting.
    pub fn merge_into_object(&self, mut base: serde_json::Map<String, serde_json::Value>) -> String {
        let typed = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(typed_map) = typed {
            for (k, v) in typed_map {
                base.entry(k).or_insert(v);
            }
        }
        serde_json::Value::Object(base).to_string()
    }
}

#[cfg(test)]
mod span_attributes_tests {
    use super::*;

    #[test]
    fn empty_attributes_serialize_to_none() {
        // Avoid writing "{}" into the column when nothing is populated;
        // the recorder treats absent and empty-object as semantically
        // distinct (absent = emission site had nothing to say).
        let attrs = SpanAttributes::default();
        assert!(attrs.to_attributes_json().is_none());
    }

    #[test]
    fn populated_attributes_skip_none_fields() {
        let attrs = SpanAttributes {
            run_id: Some("run-1".into()),
            stage: Some("trader".into()),
            model: Some("claude-sonnet-4.6".into()),
            provider: Some("anthropic".into()),
            ..SpanAttributes::default()
        };
        let json = attrs.to_attributes_json().expect("non-empty bag serializes");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = parsed.as_object().unwrap();
        // Present fields make it through.
        assert_eq!(obj.get("run_id").and_then(|v| v.as_str()), Some("run-1"));
        assert_eq!(obj.get("stage").and_then(|v| v.as_str()), Some("trader"));
        assert_eq!(
            obj.get("model").and_then(|v| v.as_str()),
            Some("claude-sonnet-4.6")
        );
        assert_eq!(obj.get("provider").and_then(|v| v.as_str()), Some("anthropic"));
        // None fields are absent, not serialized as null. Keeps the
        // payload compact and forward-compatible.
        assert!(!obj.contains_key("agent_id"));
        assert!(!obj.contains_key("tool_name"));
        assert!(!obj.contains_key("retry_count"));
        assert!(!obj.contains_key("prompt_version"));
        assert!(!obj.contains_key("decision_index"));
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let attrs = SpanAttributes {
            run_id: Some("r".into()),
            agent_id: Some("a".into()),
            stage: Some("trader".into()),
            model: Some("m".into()),
            provider: Some("p".into()),
            tool_name: Some("get_quote".into()),
            retry_count: Some(2),
            prompt_version: Some("v1".into()),
            decision_index: Some(42),
        };
        let json = attrs.to_attributes_json().expect("populated bag serializes");
        let parsed: SpanAttributes = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, attrs);
    }

    #[test]
    fn merge_into_object_preserves_existing_keys() {
        // Broker-call site path: typed attrs and the broker_call
        // sub-object must coexist in one flat payload.
        let attrs = SpanAttributes {
            run_id: Some("run-9".into()),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        base.insert(
            "broker_call".into(),
            serde_json::json!({ "side": "Buy", "symbol": "AAPL", "qty": 1.0 }),
        );
        let merged = attrs.merge_into_object(base);
        let parsed: serde_json::Value = serde_json::from_str(&merged).unwrap();
        let obj = parsed.as_object().unwrap();
        // Typed field promoted to top-level.
        assert_eq!(obj.get("run_id").and_then(|v| v.as_str()), Some("run-9"));
        // Existing broker_call sub-object kept verbatim.
        let bc = obj.get("broker_call").and_then(|v| v.as_object()).unwrap();
        assert_eq!(bc.get("side").and_then(|v| v.as_str()), Some("Buy"));
        assert_eq!(bc.get("symbol").and_then(|v| v.as_str()), Some("AAPL"));
    }

    #[test]
    fn merge_does_not_overwrite_existing_top_level_keys() {
        // If a caller already put a key in `base` that collides with a
        // typed field, the existing value wins. Defensive: prevents a
        // future caller from accidentally clobbering a more specific
        // payload by adding a SpanAttributes field with the same name.
        let attrs = SpanAttributes {
            run_id: Some("from-typed".into()),
            ..SpanAttributes::default()
        };
        let mut base = serde_json::Map::new();
        base.insert("run_id".into(), serde_json::json!("from-base"));
        let merged = attrs.merge_into_object(base);
        let parsed: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(parsed.get("run_id").and_then(|v| v.as_str()), Some("from-base"));
    }

    #[test]
    fn deserialize_tolerates_unknown_fields() {
        // Older rows may pre-date later additions; new rows may carry
        // fields this crate version doesn't know about. Both must
        // parse cleanly into the `Option`s we do know.
        let json = r#"{"run_id":"r","unknown_future_field":"x","stage":"trader"}"#;
        let parsed: SpanAttributes = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.run_id.as_deref(), Some("r"));
        assert_eq!(parsed.stage.as_deref(), Some("trader"));
    }
}
