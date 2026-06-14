//! Regression tests for the no-short-circuit execution guardrails (Phase
//! 4.2). One test per short-circuit class proves the precondition is
//! detected and yields the distinct code + remediation + correct
//! typed-error mapping. These run under `cargo test -p xvision-engine
//! --lib guardrails` with no SQLite / ApiContext.

use super::*;
use xvision_observability::unified_event::UnifiedPayload;

/// Helper: assert a short-circuit has the expected code, a non-empty
/// remediation, and maps to the expected typed-error payload variant.
fn assert_class(sc: &ShortCircuit, expected_code: &str, expected_kind: &str) {
    assert_eq!(sc.code(), expected_code, "code mismatch");
    assert!(
        !sc.remediation().trim().is_empty(),
        "remediation must be non-empty for {expected_code}"
    );
    let payload = sc.to_typed_error();
    // The typed error carries the same code + remediation, never silent.
    let err = match &payload {
        UnifiedPayload::ErrorMissingCapability(e)
        | UnifiedPayload::ErrorMissingTool(e)
        | UnifiedPayload::ErrorInvalidSchema(e)
        | UnifiedPayload::ErrorProviderUnavailable(e)
        | UnifiedPayload::ErrorPolicyDenied(e)
        | UnifiedPayload::ErrorPersistenceFailed(e) => e,
        other => panic!("not an Error* payload: {other:?}"),
    };
    assert_eq!(err.code, expected_code, "typed-error code mismatch");
    assert_eq!(
        err.remediation.as_deref(),
        Some(sc.remediation().as_str()),
        "typed-error remediation must echo ShortCircuit::remediation"
    );
    assert!(
        !err.message.trim().is_empty(),
        "typed-error message must be non-empty"
    );
    assert_eq!(
        super::typed_error_event_kind(&payload),
        expected_kind,
        "event-kind mapping mismatch"
    );
    // CLI JSON agrees with the typed error.
    let json = sc.to_cli_json();
    assert_eq!(json["short_circuit"], expected_code);
    assert_eq!(json["event_kind"], expected_kind);
    assert_eq!(json["remediation"], sc.remediation());
}

#[test]
fn missing_tool_is_detected_and_typed() {
    // Required tool absent from the available set → detected.
    let err = check_missing_tool("trader", "ohlcv", &["indicator_panel".into()])
        .expect_err("absent tool must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::MissingTool {
            role: "trader".into(),
            tool: "ohlcv".into()
        }
    );
    assert_class(&err, "missing_tool", "error_missing_tool");
    // Present tool → no short-circuit.
    assert!(check_missing_tool("trader", "ohlcv", &["ohlcv".into()]).is_ok());
}

#[test]
fn disabled_tool_is_detected_and_distinct_from_missing() {
    // Registered but disabled → DisabledTool.
    let err = check_tool_enabled("filter", "indicator_panel", true, false)
        .expect_err("registered+disabled tool must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::DisabledTool {
            role: "filter".into(),
            tool: "indicator_panel".into()
        }
    );
    assert_class(&err, "disabled_tool", "error_missing_tool");
    // Distinct code from missing_tool even though both map to ErrorMissingTool.
    assert_ne!(err.code(), "missing_tool");
    // Registered + enabled → ok. Not registered → not this guardrail's job.
    assert!(check_tool_enabled("filter", "indicator_panel", true, true).is_ok());
    assert!(check_tool_enabled("filter", "indicator_panel", false, false).is_ok());
}

#[test]
fn provider_unavailable_is_detected_and_typed() {
    let err = check_provider_available("trader", "anthropic", &["openai".into()])
        .expect_err("unavailable provider must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::ProviderUnavailable {
            role: "trader".into(),
            provider: "anthropic".into()
        }
    );
    assert_class(&err, "provider_unavailable", "error_provider_unavailable");
    assert!(check_provider_available("trader", "openai", &["openai".into()]).is_ok());
}

#[test]
fn missing_prompt_is_detected_and_typed() {
    let err =
        check_prompt_present("trader", "   \n\t ").expect_err("whitespace-only prompt must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::MissingPrompt {
            role: "trader".into()
        }
    );
    assert_class(&err, "missing_prompt", "error_missing_capability");
    assert!(check_prompt_present("trader", "Make a disciplined decision.").is_ok());
}

#[test]
fn invalid_output_schema_is_detected_and_typed() {
    let err = check_output_schema("trader", "TraderDecision", false, "missing field `action`")
        .expect_err("invalid output must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::InvalidOutputSchema {
            role: "trader".into(),
            expected: "TraderDecision".into(),
            detail: "missing field `action`".into(),
        }
    );
    assert_class(&err, "invalid_output_schema", "error_invalid_schema");
    assert!(check_output_schema("trader", "TraderDecision", true, "").is_ok());
}

#[test]
fn empty_demo_set_is_detected_when_required() {
    let err = check_demo_set_nonempty("trader", true, 0)
        .expect_err("required-but-empty demo set must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::EmptyDemoSet {
            capability: "trader".into()
        }
    );
    assert_class(&err, "empty_demo_set", "error_policy_denied");
    // Non-empty, or not required → ok.
    assert!(check_demo_set_nonempty("trader", true, 3).is_ok());
    assert!(check_demo_set_nonempty("trader", false, 0).is_ok());
}

#[test]
fn stale_optimized_prompt_is_detected_on_hash_mismatch() {
    let err = check_optimized_prompt_fresh("trader", "sig_old_abc", "sig_new_def")
        .expect_err("signature-hash mismatch must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::StaleOptimizedPrompt {
            role: "trader".into(),
            snapshot_signature_hash: "sig_old_abc".into(),
            current_signature_hash: "sig_new_def".into(),
        }
    );
    assert_class(&err, "stale_optimized_prompt", "error_invalid_schema");
    // Matching hashes → fresh.
    assert!(check_optimized_prompt_fresh("trader", "sig_x", "sig_x").is_ok());
}

#[test]
fn filter_signal_requested_but_absent_is_detected() {
    let err = check_filter_signal_present("trader", "regime_filter", false)
        .expect_err("absent upstream filter signal must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::FilterSignalRequestedButAbsent {
            consumer_role: "trader".into(),
            producer_role: "regime_filter".into(),
        }
    );
    assert_class(
        &err,
        "filter_signal_requested_but_absent",
        "error_missing_capability",
    );
    assert!(check_filter_signal_present("trader", "regime_filter", true).is_ok());
}

#[test]
fn strategy_references_unattached_slot_is_detected() {
    let err =
        check_slot_attached("reviewer", "01AGENT", false).expect_err("unattached slot must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::StrategyReferencesUnattachedSlot {
            role: "reviewer".into(),
            agent_id: "01AGENT".into(),
        }
    );
    assert_class(
        &err,
        "strategy_references_unattached_slot",
        "error_missing_capability",
    );
    assert!(check_slot_attached("reviewer", "01AGENT", true).is_ok());
}

#[test]
fn dashboard_artifact_without_persisted_row_is_detected() {
    let err = check_artifact_persisted("create_strategy", "strategy", false)
        .expect_err("UI artifact without a persisted row must short-circuit");
    assert_eq!(
        err,
        ShortCircuit::DashboardArtifactWithoutPersistedRow {
            action: "create_strategy".into(),
            artifact_kind: "strategy".into(),
        }
    );
    assert_class(
        &err,
        "dashboard_artifact_without_persisted_row",
        "error_persistence_failed",
    );
    assert!(check_artifact_persisted("create_strategy", "strategy", true).is_ok());
}

#[test]
fn every_class_has_a_distinct_code_covering_the_spec_list() {
    use std::collections::BTreeSet;
    // The ten spec classes, instantiated.
    let all = [
        ShortCircuit::MissingTool {
            role: "r".into(),
            tool: "t".into(),
        },
        ShortCircuit::DisabledTool {
            role: "r".into(),
            tool: "t".into(),
        },
        ShortCircuit::ProviderUnavailable {
            role: "r".into(),
            provider: "p".into(),
        },
        ShortCircuit::MissingPrompt { role: "r".into() },
        ShortCircuit::InvalidOutputSchema {
            role: "r".into(),
            expected: "e".into(),
            detail: "d".into(),
        },
        ShortCircuit::EmptyDemoSet {
            capability: "c".into(),
        },
        ShortCircuit::StaleOptimizedPrompt {
            role: "r".into(),
            snapshot_signature_hash: "a".into(),
            current_signature_hash: "b".into(),
        },
        ShortCircuit::FilterSignalRequestedButAbsent {
            consumer_role: "c".into(),
            producer_role: "p".into(),
        },
        ShortCircuit::StrategyReferencesUnattachedSlot {
            role: "r".into(),
            agent_id: "a".into(),
        },
        ShortCircuit::DashboardArtifactWithoutPersistedRow {
            action: "a".into(),
            artifact_kind: "k".into(),
        },
    ];
    let codes: BTreeSet<&str> = all.iter().map(|sc| sc.code()).collect();
    assert_eq!(codes.len(), all.len(), "duplicate short-circuit codes");
    // The exported const enumerates exactly these ten codes.
    let const_codes: BTreeSet<&str> = SHORT_CIRCUIT_CODES.iter().copied().collect();
    assert_eq!(
        codes, const_codes,
        "SHORT_CIRCUIT_CODES drifted from variant codes"
    );
    assert_eq!(SHORT_CIRCUIT_CODES.len(), 10);
}

#[test]
fn report_mirror_matches_cli_json() {
    let sc = ShortCircuit::MissingTool {
        role: "trader".into(),
        tool: "ohlcv".into(),
    };
    let report = ShortCircuitReport::from(&sc);
    assert_eq!(report.short_circuit, "missing_tool");
    assert_eq!(report.event_kind, "error_missing_tool");
    assert_eq!(report.remediation, sc.remediation());
    // Round-trips through serde.
    let json = serde_json::to_string(&report).unwrap();
    let back: ShortCircuitReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, report);
}
