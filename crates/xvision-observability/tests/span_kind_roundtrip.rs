//! Round-trip lock for `SpanKind`. Each variant's serde wire string
//! MUST equal its `as_db_str()` output — the file-level docstring on
//! `types.rs` notes this invariant ("a column-string comparison in
//! SQL matches a Rust `RunStatus::Completed` etc"). The same applies
//! to `SpanKind` because the SQLite recorder writes `kind.as_db_str()`
//! and the dashboard projection deserializes the same column through
//! serde — divergence between the two surfaces silently breaks the
//! span renderer.
//!
//! Failure mode this guards against: a new variant is added with
//! `#[serde(rename = "...")]` but its arm in `as_db_str()` is
//! forgotten (or vice versa). The mismatch is invisible until a row
//! lands and the projection layer fails to deserialize.
//!
//! Pinned by F-4 (`harness-span-taxonomy-extension`) alongside the
//! addition of `ToolValidateInput`, `ToolValidateOutput`,
//! `RecoveryAttempt`, and `StateTransition`. The test enumerates
//! every variant explicitly — adding one without updating the list
//! fails compilation in `serde_json::from_str` line below.

use xvision_observability::SpanKind;

/// `(variant, wire_string)` for every `SpanKind`. If you add a
/// variant, add it here.
fn all_variants() -> Vec<(SpanKind, &'static str)> {
    vec![
        (SpanKind::AgentRun, "agent.run"),
        (SpanKind::AgentPlan, "agent.plan"),
        (SpanKind::DecisionModel, "decision.model"),
        (SpanKind::DecisionReasoning, "decision.reasoning"),
        (SpanKind::ToolCall, "tool.call"),
        (SpanKind::ApprovalRequest, "approval.request"),
        (SpanKind::ApprovalResponse, "approval.response"),
        (SpanKind::SandboxExec, "sandbox.exec"),
        (SpanKind::SupervisorReview, "supervisor.review"),
        (SpanKind::FinancialEval, "financial.eval"),
        (SpanKind::ArtifactWrite, "artifact.write"),
        (SpanKind::IpcNotification, "ipc.notification"),
        (SpanKind::SkillInvoke, "skill.invoke"),
        (SpanKind::BrokerCall, "broker.call"),
        (SpanKind::ToolValidateInput, "tool.validate_input"),
        (SpanKind::ToolValidateOutput, "tool.validate_output"),
        (SpanKind::RecoveryAttempt, "recovery.attempt"),
        (SpanKind::StateTransition, "state.transition"),
        (SpanKind::AgentDecision, "agent.decision"),
        (SpanKind::FilterEval, "filter.eval"),
        (SpanKind::RiskGate, "risk.gate"),
    ]
}

#[test]
fn as_db_str_matches_serde_wire_string_for_every_variant() {
    for (variant, expected) in all_variants() {
        assert_eq!(variant.as_db_str(), expected, "as_db_str() drift for {variant:?}");
        let json = serde_json::to_string(&variant).expect("serialize");
        // serde wire form is a quoted JSON string, e.g. `"agent.run"`.
        assert_eq!(
            json,
            format!("\"{expected}\""),
            "serde wire-form drift for {variant:?}"
        );
    }
}

#[test]
fn serde_round_trip_for_every_variant() {
    for (variant, _) in all_variants() {
        let json = serde_json::to_string(&variant).expect("serialize");
        let parsed: SpanKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, variant, "round-trip mismatch for {variant:?}");
    }
}

#[test]
fn new_f4_variants_have_dotted_wire_form() {
    // F-4 follows the dotted-namespace convention established by the
    // existing variants. Locking this prevents a future rename to
    // e.g. `tool_validate_input` (underscore-only) which would break
    // the trace dock's per-kind dispatch in the dashboard.
    assert_eq!(SpanKind::ToolValidateInput.as_db_str(), "tool.validate_input");
    assert_eq!(SpanKind::ToolValidateOutput.as_db_str(), "tool.validate_output");
    assert_eq!(SpanKind::RecoveryAttempt.as_db_str(), "recovery.attempt");
    assert_eq!(SpanKind::StateTransition.as_db_str(), "state.transition");
    assert_eq!(SpanKind::AgentDecision.as_db_str(), "agent.decision");
    assert_eq!(SpanKind::FilterEval.as_db_str(), "filter.eval");
    assert_eq!(SpanKind::RiskGate.as_db_str(), "risk.gate");
}
