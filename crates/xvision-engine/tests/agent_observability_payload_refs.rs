//! Integration test for `harness-payload-write-bodies`.
//!
//! Verifies that `execute_slot` — driven through `ObsEmitter` with a
//! real `BlobStore` — correctly writes prompt and response bytes under
//! `full_debug` retention and suppresses writes under `hash_only`.
//!
//! Two scenarios:
//!
//! 1. `full_debug` + `store_prompts` + `store_responses` → both
//!    `prompt_payload_ref` and `response_payload_ref` on the emitted
//!    `ModelCallFinishedEvent` are `Some(_)`, and `BlobStore::read`
//!    on each ref recovers the original prompt request body /
//!    response text verbatim.
//!
//! 2. `hash_only` → both refs are `None`; nothing is written to the
//!    `BlobStore`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::{canonical_request_bytes, ObsEmitter, ObsRetentionPolicy};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{
    BlobRef, BlobStore, ModelCallFinishedEvent, NoopRecorder, ObservabilityConfig, RetentionMode, RunEvent,
    RunEventBus,
};

/// Fixed assistant text returned by the canned dispatcher. This is
/// what the test expects to find verbatim in the response blob.
const ASSISTANT_TEXT: &str = r#"{"action":"hold","conviction":0.5,"justification":"test payload ref"}"#;

/// Minimal `LlmDispatch` that returns a fixed response so the test
/// doesn't need a live provider.
#[derive(Default)]
struct CannedDispatch {
    last_request: Mutex<Option<LlmRequest>>,
}

impl CannedDispatch {
    fn last_request(&self) -> LlmRequest {
        self.last_request
            .lock()
            .expect("last_request mutex poisoned")
            .clone()
            .expect("dispatcher must capture the LlmRequest")
    }
}

#[async_trait]
impl LlmDispatch for CannedDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        *self.last_request.lock().expect("last_request mutex poisoned") = Some(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: ASSISTANT_TEXT.to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 5,
            output_tokens: 12,
        })
    }
}

fn trader_slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

fn policy_for(mode: RetentionMode) -> ObsRetentionPolicy {
    let mut cfg = ObservabilityConfig::default();
    cfg.retention.mode = mode;
    cfg.retention.store_prompts = true;
    cfg.retention.store_responses = true;
    cfg.retention.max_payload_bytes = 200_000;
    ObsRetentionPolicy::from_config(&cfg)
}

/// Drain events from the bus by quiescing + yielding briefly so the
/// `NoopRecorder`'s internal consumer task has time to process them.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn find_finished(events: &[RunEvent]) -> &ModelCallFinishedEvent {
    events
        .iter()
        .find_map(|e| match e {
            RunEvent::ModelCallFinished(m) => Some(m),
            _ => None,
        })
        .expect("ModelCallFinished event must be published by execute_slot")
}

/// `full_debug` round-trip: `execute_slot` produces a
/// `ModelCallFinishedEvent` with non-None `prompt_payload_ref` and
/// `response_payload_ref`, and `BlobStore::read` on each ref returns
/// the original prompt request body / response text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_debug_execute_slot_writes_prompt_and_response_blobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-payload-refs-full-debug")
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(store.clone());
    let dispatch = Arc::new(CannedDispatch::default());

    let slot = trader_slot();
    let result = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "You are a deterministic test trader.".into(),
        upstream_inputs: serde_json::json!({ "price": 100.0 }),
        dispatch: dispatch.clone(),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        response_schema: None,
        max_tokens: Some(1234),
        temperature: Some(0.2),
        obs: Some(emitter),
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .expect("execute_slot must succeed with CannedDispatch");

    // Sanity: the response carries the canned text.
    let text = result.text();
    assert!(
        text.contains("hold"),
        "expected canned response text in result, got: {text:?}"
    );

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);

    // Both refs must be populated.
    let pref = m
        .prompt_payload_ref
        .as_ref()
        .expect("full_debug must populate prompt_payload_ref");
    let rref = m
        .response_payload_ref
        .as_ref()
        .expect("full_debug must populate response_payload_ref");

    // Prompt blob is the canonical JSON of the exact LlmRequest handed to
    // the dispatcher, not just a digest marker or partial prompt field.
    let prompt_bytes = store
        .read(&BlobRef(pref.clone()))
        .expect("prompt blob must exist in BlobStore");
    let captured_request = dispatch.last_request();
    assert_eq!(
        prompt_bytes,
        canonical_request_bytes(&captured_request),
        "stored prompt blob must equal the full LlmRequest dispatched to the provider",
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&prompt_bytes).expect("prompt blob must be valid JSON");
    assert_eq!(
        parsed["system_prompt"].as_str().unwrap(),
        "You are a deterministic test trader.",
        "stored prompt must contain the slot's system_prompt verbatim",
    );
    assert_eq!(parsed["max_tokens"], 1234);
    assert_eq!(parsed["temperature"], 0.2);

    // Response blob must decode back to the exact assistant text the
    // canned dispatcher returned.
    let resp_bytes = store
        .read(&BlobRef(rref.clone()))
        .expect("response blob must exist in BlobStore");
    assert_eq!(
        std::str::from_utf8(&resp_bytes).unwrap(),
        ASSISTANT_TEXT,
        "stored response blob must equal the verbatim assistant text",
    );
}

/// `hash_only` suppression: `execute_slot` produces a
/// `ModelCallFinishedEvent` with both `prompt_payload_ref` and
/// `response_payload_ref` as `None`, and nothing is written to the
/// `BlobStore`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hash_only_execute_slot_leaves_payload_refs_none() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-payload-refs-hash-only")
        .with_retention(policy_for(RetentionMode::HashOnly))
        .with_blob_store(store.clone());

    let slot = trader_slot();
    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "You are a deterministic test trader.".into(),
        upstream_inputs: serde_json::json!({ "price": 100.0 }),
        dispatch: Arc::new(CannedDispatch::default()),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter),
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .expect("execute_slot must succeed with CannedDispatch");

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);

    assert!(
        m.prompt_payload_ref.is_none(),
        "hash_only must leave prompt_payload_ref None, got {:?}",
        m.prompt_payload_ref,
    );
    assert!(
        m.response_payload_ref.is_none(),
        "hash_only must leave response_payload_ref None, got {:?}",
        m.response_payload_ref,
    );

    // The blob store must not have been written to at all.
    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .map(|d| d.collect::<Result<Vec<_>, _>>().unwrap_or_default())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hash_only must not call BlobStore::write; found blob entries: {entries:?}",
    );
}
