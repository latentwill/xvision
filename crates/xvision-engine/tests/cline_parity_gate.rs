//! Stage 3 Task 7 / inheritance item 6 — LlmDispatch vs Cline-record parity
//! gate. This is the GATE that must be green before the routine `LlmDispatch`
//! flag is retired (Task 10): it proves the Cline path produces the SAME
//! `TraderDecision`-shaped output as the legacy `LlmDispatch` path for a fixed
//! set of cycles.
//!
//! ## Tolerance (documented decision — do NOT loosen to pass)
//!
//! Decisions are compared field-by-field on their parsed JSON:
//! * **Structured fields** (`action`, and any string/bool/enum field) must be
//!   EXACTLY equal — a different action is a real divergence, never tolerated.
//! * **Numeric fields** (e.g. `conviction`, sizing, stop/target prices) must
//!   be equal within an absolute epsilon of `1e-9`. Rationale: both paths
//!   serialize the SAME model output through `serde_json`; the only legal
//!   source of difference is f64 round-trip representation
//!   (`serde_json` parses `0.8` deterministically, so in practice the values
//!   are bit-identical — the epsilon exists to document intent, not to paper
//!   over a real divergence). If a numeric field differs by more than the
//!   epsilon, the Cline path has a real bug — fix the Cline path, do not widen
//!   this epsilon.
//!
//! ## What this exercises
//!
//! The actual risk the gate guards is response-wrapping fidelity: the Cline
//! executor wraps the sidecar's `submit_decision` payload into an
//! `LlmResponse` (`ContentBlock::Text { decision_json }`) that the SAME
//! downstream parser reads as the LlmDispatch path. The gate asserts that for
//! identical model output, `resp.text()` round-trips to a structurally-equal
//! decision through both runtimes.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput, TrajectoryMode};
use xvision_engine::agent::llm::{ContentBlock, LlmResponse, ResponseSchema, StopReason};
use xvision_engine::eval::executor::trader_output::validate_trader_output_text;
use xvision_engine::strategies::slot::LLMSlot;

const NUMERIC_EPSILON: f64 = 1e-9;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(dir: &TempDir, decision_json: &str) -> AgentClient {
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&json!({ "decisionJson": decision_json })).unwrap(),
    )
    .unwrap();
    AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)")
}

fn anthropic_slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

fn anthropic_entry() -> ProviderEntry {
    ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    }
}

/// The LlmDispatch path's `LlmResponse` for a fixed decision: a single Text
/// block carrying the decision JSON verbatim (exactly the shape the real
/// dispatch + decision parser produce). This is the reference the Cline path
/// must match.
fn llm_dispatch_response(decision_json: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: decision_json.to_string(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 11,
        output_tokens: 7,
    }
}

/// Assert two decision JSON values are equal within the documented tolerance:
/// structured fields EXACT, numeric fields within `NUMERIC_EPSILON`.
fn assert_decisions_parity(reference: &serde_json::Value, candidate: &serde_json::Value, ctx: &str) {
    match (reference, candidate) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{ctx}: decision object key sets diverge"
            );
            for (k, av) in a {
                let bv = b.get(k).unwrap_or_else(|| panic!("{ctx}: missing key {k}"));
                assert_decisions_parity(av, bv, &format!("{ctx}.{k}"));
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{ctx}: array length diverges");
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                assert_decisions_parity(av, bv, &format!("{ctx}[{i}]"));
            }
        }
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            // Numeric tolerance: within NUMERIC_EPSILON (documented above).
            let (af, bf) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (af - bf).abs() <= NUMERIC_EPSILON,
                "{ctx}: numeric divergence beyond epsilon: {af} vs {bf}"
            );
        }
        // All other scalar kinds (string/bool/null): EXACT equality.
        _ => assert_eq!(reference, candidate, "{ctx}: structured field diverges"),
    }
}

/// The fixed set of decisions exercised by the gate — a representative mix of
/// actions + numeric fields. Each is run through both runtimes.
fn fixed_cycles() -> Vec<&'static str> {
    vec![
        r#"{"action":"long_open","conviction":0.8,"justification":"trend up"}"#,
        r#"{"action":"short_open","conviction":0.62,"justification":"vol expansion"}"#,
        r#"{"action":"hold","conviction":0.5,"justification":"range-bound"}"#,
        r#"{"action":"flat","conviction":0.9,"justification":"target hit"}"#,
    ]
}

#[tokio::test]
async fn cline_record_matches_llm_dispatch_over_fixed_cycles() {
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    for (i, decision_json) in fixed_cycles().into_iter().enumerate() {
        // (a) LlmDispatch reference decision.
        let reference_resp = llm_dispatch_response(decision_json);
        validate_trader_output_text(&reference_resp.text(), "parity-reference", i as u32)
            .expect("reference must satisfy trader_output schema");
        let reference: serde_json::Value =
            serde_json::from_str(&reference_resp.text()).expect("reference parses");

        // (b) Cline-record path: the sidecar returns the same model output;
        //     execute_slot_cline wraps it into an LlmResponse.
        let dir = TempDir::new().unwrap();
        let client = Arc::new(spawn_mock(&dir, decision_json).await);
        let candidate_resp = execute_slot_cline(ClineSlotInput {
            slot: &slot,
            provider_entry: &entry,
            api_key: Some("k".into()),
            system_prompt: "Decide whether to trade.".into(),
            upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
            response_schema: ResponseSchema::trader_output(),
            allowed_tools: vec!["indicators.rsi".into()],
            max_tokens: Some(4096),
            max_wall_ms: None,
            run_id: format!("parity-{i}::trader"),
            cline_client: client.clone(),
            trajectory_mode: TrajectoryMode::Record,
            record_slot_role: None,
            obs: None,
            model_call_span_id: None,
            reasoning_effort: None,
        })
        .await
        .expect("cline-record must produce an LlmResponse");
        let candidate: serde_json::Value =
            serde_json::from_str(&candidate_resp.text()).expect("candidate parses");
        validate_trader_output_text(&candidate_resp.text(), "parity-candidate", i as u32)
            .expect("candidate must satisfy trader_output schema");

        // Parity within the documented tolerance.
        assert_decisions_parity(&reference, &candidate, &format!("cycle[{i}]"));

        Arc::try_unwrap(client).ok().unwrap().shutdown().await.unwrap();
    }
}

#[test]
fn parity_helper_flags_action_divergence() {
    // The gate must NOT tolerate a structured-field divergence (a different
    // action is a real bug). Confirm the comparator catches it.
    let a = json!({"action": "long_open", "conviction": 0.8});
    let b = json!({"action": "short_open", "conviction": 0.8});
    let result = std::panic::catch_unwind(|| assert_decisions_parity(&a, &b, "test"));
    assert!(result.is_err(), "action divergence must fail the gate");
}

#[test]
fn parity_helper_tolerates_epsilon_numeric_drift() {
    // A sub-epsilon numeric difference is tolerated (documented f64
    // round-trip allowance); a supra-epsilon difference is NOT.
    let a = json!({"conviction": 0.8});
    let b = json!({"conviction": 0.8 + 1e-12});
    assert_decisions_parity(&a, &b, "epsilon-ok"); // within epsilon → passes

    let a = json!({"conviction": 0.8});
    let b = json!({"conviction": 0.81});
    let result = std::panic::catch_unwind(|| assert_decisions_parity(&a, &b, "epsilon-bad"));
    assert!(result.is_err(), "supra-epsilon numeric drift must fail the gate");
}

#[test]
fn parity_gate_rejects_schema_invalid_equal_payloads() {
    let invalid = r#"{"action":"long_open","conviction":0.8,"justification":"trend up","stop_pct":2.5}"#;
    let reference_resp = llm_dispatch_response(invalid);
    let candidate_resp = llm_dispatch_response(invalid);

    assert!(
        validate_trader_output_text(&reference_resp.text(), "invalid-reference", 0).is_err(),
        "reference with extra fields must fail trader_output schema"
    );
    assert!(
        validate_trader_output_text(&candidate_resp.text(), "invalid-candidate", 0).is_err(),
        "candidate with extra fields must fail trader_output schema"
    );
}
