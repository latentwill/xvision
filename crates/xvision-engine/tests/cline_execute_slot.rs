//! Stage 1 integration test for `execute_slot_cline` (Cline runtime
//! unification, Task 5 + Task 8 / inheritance item 2).
//!
//! Drives one slot through a mock `xvision-agentd` sidecar — a pure-Node
//! stdlib JSON-RPC server (`tests/fixtures/mock_agentd.js`) that needs no
//! `@cline/sdk` build. `AgentClient::spawn` launches it exactly as it
//! launches the real sidecar (`node <bin> --socket <path>`, waiting for the
//! `{"event":"ready"}` stderr line), so the start_run -> step -> end_run
//! lifecycle is exercised end-to-end.
//!
//! Asserts:
//! * happy path — `submit_decision` payload round-trips into an
//!   `LlmResponse` the existing parser accepts (`resp.text()` is the JSON);
//! * provider matrix abort (item 5) — local-candle aborts, no fallback;
//! * failure boundary (item 2) — missing decision, non-completed status,
//!   non-JSON payload, and sidecar crash mid-step all surface typed errors
//!   (the cycle fails, never a silent empty decision);
//! * idempotency (item 2) — a duplicate `start_run` with the same run_id is
//!   rejected by the sidecar and surfaces as a typed start-run error.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput};
use xvision_engine::agent::llm::ResponseSchema;
use xvision_engine::strategies::slot::LLMSlot;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

/// Spawn the mock sidecar with an optional per-socket config (controls the
/// step behaviour). Returns the client plus the TempDir whose lifetime must
/// outlive the client (it owns the socket path).
async fn spawn_mock(cfg: Option<serde_json::Value>) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    if let Some(cfg) = cfg {
        std::fs::write(
            dir.path().join("agentd.sock.cfg"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .expect("write cfg");
    }
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
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

fn slot_input<'a>(
    slot: &'a LLMSlot,
    entry: &'a ProviderEntry,
    client: Arc<AgentClient>,
    run_id: &str,
) -> ClineSlotInput<'a> {
    ClineSlotInput {
        slot,
        provider_entry: entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec!["indicators.rsi".into()],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.into(),
        cline_client: client,
        trajectory_mode: xvision_engine::agent::execute_cline::TrajectoryMode::Record,
        record_slot_role: None,
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    }
}

#[tokio::test]
async fn cline_slot_returns_submit_decision_as_llm_response() {
    let (client, _dir) = spawn_mock(Some(json!({
        "decisionJson": r#"{"action":"long_open","conviction":0.8,"justification":"mock"}"#
    })))
    .await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let resp = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-1::trader"))
        .await
        .expect("cline slot must produce an LlmResponse");

    // The existing parser reads `resp.text()`; assert the submit_decision
    // payload round-trips verbatim.
    let decision: serde_json::Value = serde_json::from_str(&resp.text()).expect("decision JSON");
    assert_eq!(decision["action"], "long_open");
    assert_eq!(resp.input_tokens, 11);
    assert_eq!(resp.output_tokens, 7);

    Arc::try_unwrap(client).ok().unwrap().shutdown().await.unwrap();
}

#[tokio::test]
async fn cline_slot_aborts_on_unmapped_provider() {
    // local-candle has no Cline mapping (item 5): a hard abort, no fallback.
    let (client, _dir) = spawn_mock(None).await;
    let client = Arc::new(client);
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "local".into(),
        allowed_tools: Vec::new(),
        provider: Some("local".into()),
        model: Some("mock".into()),
    };
    let entry = ProviderEntry {
        name: "local".into(),
        kind: ProviderKind::LocalCandle,
        base_url: String::new(),
        api_key_env: String::new(),
        enabled_models: vec!["mock".into()],
    };

    let err = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-x::trader"))
        .await
        .expect_err("unmapped provider must abort");
    let msg = format!("{err:#}");
    assert!(msg.contains("no Cline mapping"), "got: {msg}");
}

#[tokio::test]
async fn cline_slot_fails_when_decision_missing() {
    // Agent completed the run but never called submit_decision — the cycle
    // must fail, NOT synthesize an empty/hold decision (item 2).
    let (client, _dir) = spawn_mock(Some(json!({ "decisionJson": "OMIT" }))).await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let err = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-2::trader"))
        .await
        .expect_err("missing decision must fail the cycle");
    let msg = format!("{err:#}");
    assert!(msg.contains("without calling submit_decision"), "got: {msg}");
}

#[tokio::test]
async fn cline_slot_fails_on_non_completed_status() {
    let (client, _dir) = spawn_mock(Some(json!({
        "stepStatus": "aborted",
        "decisionJson": "OMIT"
    })))
    .await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let err = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-3::trader"))
        .await
        .expect_err("non-completed status must fail the cycle");
    let msg = format!("{err:#}");
    assert!(msg.contains("did not complete"), "got: {msg}");
    assert!(msg.contains("aborted"), "got: {msg}");
}

#[tokio::test]
async fn cline_slot_fails_on_non_json_decision() {
    let (client, _dir) = spawn_mock(Some(json!({ "decisionJson": "NOTJSON" }))).await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let err = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-4::trader"))
        .await
        .expect_err("non-JSON decision must fail the cycle");
    let msg = format!("{err:#}");
    assert!(msg.contains("not valid JSON"), "got: {msg}");
}

#[tokio::test]
async fn cline_slot_surfaces_sidecar_crash_mid_step() {
    // The sidecar destroys the connection + exits on step (item 2: crash
    // boundary). The transport error must surface typed — never a silent
    // empty decision.
    let (client, _dir) = spawn_mock(Some(json!({ "crashOnStep": true }))).await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let err = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "cycle-5::trader"))
        .await
        .expect_err("sidecar crash mid-step must surface a typed error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("step failed") || msg.contains("transport") || msg.contains("crash"),
        "crash error should identify a sidecar/transport failure; got: {msg}"
    );
}

#[tokio::test]
async fn cline_duplicate_run_id_is_rejected() {
    // run_id is the idempotency key (item 2): the first run succeeds, a
    // second start_run with the same id is rejected by the sidecar and
    // surfaces as a typed start-run error — no double execution.
    let (client, _dir) = spawn_mock(None).await;
    let client = Arc::new(client);
    let slot = anthropic_slot();
    let entry = anthropic_entry();

    let first = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "dup-run::trader")).await;
    assert!(first.is_ok(), "first run should succeed: {first:?}");

    let second = execute_slot_cline(slot_input(&slot, &entry, client.clone(), "dup-run::trader"))
        .await
        .expect_err("duplicate run_id must be rejected");
    let msg = format!("{second:#}");
    assert!(msg.contains("start_run failed"), "got: {msg}");
    assert!(
        msg.contains("dup-run::trader"),
        "error should name the run_id; got: {msg}"
    );
}
