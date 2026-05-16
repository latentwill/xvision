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
    #[serde(rename = "model.call")]
    ModelCall,
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
}

impl SpanKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::AgentRun => "agent.run",
            Self::AgentPlan => "agent.plan",
            Self::ModelCall => "model.call",
            Self::ToolCall => "tool.call",
            Self::ApprovalRequest => "approval.request",
            Self::ApprovalResponse => "approval.response",
            Self::SandboxExec => "sandbox.exec",
            Self::SupervisorReview => "supervisor.review",
            Self::FinancialEval => "financial.eval",
            Self::ArtifactWrite => "artifact.write",
            Self::IpcNotification => "ipc.notification",
            Self::SkillInvoke => "skill.invoke",
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
