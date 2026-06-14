//! Byte-identical output parity: `execute_slot_cline` vs `LlmDispatch` path.
//!
//! Invariant (line 564 comment in `execute_cline.rs`): `execute_slot_cline`
//! wraps the sidecar's `decision_json` in exactly ONE `ContentBlock::Text` so
//! that `LlmResponse::text()` returns the verbatim JSON string — byte-identical
//! to an `LlmResponse` built from the same string on the `LlmDispatch` path.
//!
//! This matters because the downstream parser
//! (`TraderOutput::parse_response` / `dispatch_capability`) reads `resp.text()`
//! and must work without modification regardless of which runtime produced
//! the response. A regression here would silently break Cline-runtime decisions.
//!
//! The test uses the same `spawn_mock` / `mock_agentd.js` harness as
//! `cline_pipeline_flag.rs`. Requires `node` on PATH (same prerequisite that
//! gates all Cline integration tests).

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput, TrajectoryMode};
use xvision_engine::agent::llm::{ContentBlock, LlmResponse, ResponseSchema, StopReason};
use xvision_engine::strategies::slot::LLMSlot;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(cfg: serde_json::Value) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .expect("write cfg");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
}

/// The raw decision JSON the mock sidecar will return as `decision_json`.
const DECISION_JSON: &str = r#"{"action":"hold","conviction":0.5,"justification":"parity"}"#;

#[tokio::test]
async fn execute_slot_cline_output_is_byte_identical_to_llm_dispatch_path() {
    // Step 1: spawn the mock sidecar configured to return DECISION_JSON.
    let (client, _dir) = spawn_mock(json!({
        "decisionJson": DECISION_JSON,
    }))
    .await;

    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };

    let provider_entry = ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    };

    // Step 2: build ClineSlotInput with minimal, fully-specified fields.
    let input = ClineSlotInput {
        slot: &slot,
        provider_entry: &provider_entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide.".into(),
        upstream_inputs: json!({}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: "parity-test-1".into(),
        cline_client: Arc::new(client),
        trajectory_mode: TrajectoryMode::Record,
        record_slot_role: None,
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    };

    // Step 3: drive the sidecar path.
    let cline_resp = execute_slot_cline(input).await.unwrap();

    // Step 4: build the equivalent LlmDispatch-path response directly.
    let dispatch_resp = LlmResponse {
        content: vec![ContentBlock::Text {
            text: DECISION_JSON.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    };

    // Step 5: the Cline path MUST emit exactly one content block.
    assert_eq!(
        cline_resp.content.len(),
        1,
        "execute_slot_cline must produce exactly one ContentBlock (single-Text invariant); \
         got {} blocks: {:?}",
        cline_resp.content.len(),
        cline_resp
            .content
            .iter()
            .map(|b| match b {
                ContentBlock::Text { text } => format!("Text({text:?})"),
                ContentBlock::ToolUse { name, .. } => format!("ToolUse({name})"),
                ContentBlock::ToolResult { tool_use_id, .. } => format!("ToolResult({tool_use_id})"),
            })
            .collect::<Vec<_>>(),
    );

    // Step 6: the single block must be ContentBlock::Text.
    assert!(
        matches!(cline_resp.content[0], ContentBlock::Text { .. }),
        "the single ContentBlock from execute_slot_cline must be ContentBlock::Text; \
         downstream parser reads resp.text() which only concatenates Text blocks",
    );

    // Step 7: text() output is byte-identical between the Cline and LlmDispatch paths.
    assert_eq!(
        cline_resp.text(),
        dispatch_resp.text(),
        "cline_resp.text() must be byte-identical to the LlmDispatch-path resp.text(); \
         this regression lock ensures TraderOutput::parse_response works on both runtimes",
    );

    // Step 8: both are structurally equal as JSON (field-order-independent confirmation).
    let cline_val: serde_json::Value =
        serde_json::from_str(&cline_resp.text()).expect("cline_resp.text() must be valid JSON");
    let dispatch_val: serde_json::Value =
        serde_json::from_str(&dispatch_resp.text()).expect("dispatch_resp.text() must be valid JSON");
    assert_eq!(
        cline_val, dispatch_val,
        "the JSON values must be structurally equal (field-order independent); \
         cline={cline_val}, dispatch={dispatch_val}",
    );

    // Belt-and-suspenders: assert the verbatim string the sidecar returned
    // is the exact text the downstream parser will see.
    assert_eq!(
        cline_resp.text(),
        DECISION_JSON,
        "cline_resp.text() must be the verbatim DECISION_JSON the mock sidecar returned",
    );
}
