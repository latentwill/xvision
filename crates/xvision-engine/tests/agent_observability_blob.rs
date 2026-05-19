//! Round-trip regression for `harness-payload-blob-write`.
//!
//! Closes the gap PR #282's investigation identified: prior to this
//! track, `emit_model_call_finished` hardcoded
//! `prompt_payload_ref: None` / `response_payload_ref: None`, leaving
//! the trace dock to render the
//! "prompt body not captured for this run — re-run to capture"
//! placeholder on every `full_debug` run.
//!
//! Three retention modes are exercised:
//!
//! - `full_debug` → both refs are `Some(_)` and the bytes stored in the
//!   `BlobStore` decode back to the verbatim prompt request body and the
//!   verbatim assistant text.
//! - `hash_only` → both refs are `None`; nothing lands in the
//!   `BlobStore` (no production caller of `BlobStore::write` from the
//!   emitter on this mode).
//! - `redacted` → both refs are `Some(_)`, but the bytes stored on disk
//!   are the post-`Redactor` form. The recorded blob must not contain
//!   the source secret (`sk-ant-…` style key).

use std::sync::Arc;

use async_trait::async_trait;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason};
use xvision_engine::agent::observability::{ObsEmitter, ObsRetentionPolicy};
use xvision_observability::{
    BlobRef, BlobStore, ModelCallFinishedEvent, NoopRecorder, ObservabilityConfig, RetentionMode,
    RunEvent, RunEventBus,
};

/// LlmDispatch that returns a fixed assistant text. The body is the
/// payload we'll assert was written verbatim under `full_debug`.
struct CannedDispatch {
    text: String,
}

#[async_trait]
impl LlmDispatch for CannedDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 7,
            output_tokens: 11,
        })
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

fn user_prompt() -> LlmRequest {
    LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "You are a deterministic test agent.".into(),
        messages: vec![Message::user_text("What is 2+2?")],
        max_tokens: Some(128),
        tools: Vec::new(),
        temperature: None,
        response_schema: None,
    }
}

/// Drain helper. The bus consumer is a separate task; we yield + sleep
/// until the published event surfaces on the `NoopRecorder` snapshot.
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
        .expect("ModelCallFinished event published")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_debug_persists_prompt_and_response_blobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let req = user_prompt();
    let assistant = "The answer is 4.".to_string();

    let emitter = ObsEmitter::new(bus.clone(), "run-full-debug")
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(store.clone());

    emitter
        .emit_model_call_finished_with_payloads(
            "span-1",
            "anthropic",
            "claude-sonnet-4-6",
            Some(7),
            Some(11),
            None,
            "sha256:dummy".to_string(),
            Some("sha256:dummy".to_string()),
            Some(&req),
            Some(assistant.as_str()),
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);
    let pref = m
        .prompt_payload_ref
        .as_ref()
        .expect("full_debug must populate prompt_payload_ref");
    let rref = m
        .response_payload_ref
        .as_ref()
        .expect("full_debug must populate response_payload_ref");

    let prompt_bytes = store.read(&BlobRef(pref.clone())).expect("prompt blob");
    // Stored prompt is the canonical JSON of the prompt-digest input
    // (system_prompt + messages + tools). Decoding back to JSON must
    // recover the original system prompt verbatim.
    let parsed: serde_json::Value = serde_json::from_slice(&prompt_bytes).expect("prompt is JSON");
    assert_eq!(
        parsed["system_prompt"].as_str().unwrap(),
        "You are a deterministic test agent."
    );

    let resp_bytes = store.read(&BlobRef(rref.clone())).expect("response blob");
    assert_eq!(
        std::str::from_utf8(&resp_bytes).unwrap(),
        "The answer is 4."
    );

    let _ = CannedDispatch { text: assistant };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hash_only_leaves_payload_refs_none_and_writes_no_blobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let req = user_prompt();
    let assistant = "The answer is 4.".to_string();

    let emitter = ObsEmitter::new(bus.clone(), "run-hash-only")
        .with_retention(policy_for(RetentionMode::HashOnly))
        .with_blob_store(store.clone());

    emitter
        .emit_model_call_finished_with_payloads(
            "span-2",
            "anthropic",
            "claude-sonnet-4-6",
            Some(7),
            Some(11),
            None,
            "sha256:dummy".to_string(),
            Some("sha256:dummy".to_string()),
            Some(&req),
            Some(assistant.as_str()),
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);
    assert!(
        m.prompt_payload_ref.is_none(),
        "hash_only must leave prompt_payload_ref None, got {:?}",
        m.prompt_payload_ref
    );
    assert!(
        m.response_payload_ref.is_none(),
        "hash_only must leave response_payload_ref None, got {:?}",
        m.response_payload_ref
    );

    // No files in the blob root: the emitter must not call BlobStore::write at all.
    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .map(|d| d.collect::<Result<Vec<_>, _>>().unwrap_or_default())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hash_only must not touch the blob store, found: {entries:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redacted_scrubs_secret_before_writing_blob() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    // Inject a synthetic Anthropic API key into both prompt + response.
    // The redactor's v1 pattern set covers `sk-ant-` keys; the stored
    // blob must NOT contain the raw key.
    let mut req = user_prompt();
    req.system_prompt = "secret=sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-end".into();
    let assistant = "leak: sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB-end".to_string();

    let emitter = ObsEmitter::new(bus.clone(), "run-redacted")
        .with_retention(policy_for(RetentionMode::Redacted))
        .with_blob_store(store.clone());

    emitter
        .emit_model_call_finished_with_payloads(
            "span-3",
            "anthropic",
            "claude-sonnet-4-6",
            Some(7),
            Some(11),
            None,
            "sha256:dummy".to_string(),
            Some("sha256:dummy".to_string()),
            Some(&req),
            Some(assistant.as_str()),
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);
    let pref = m
        .prompt_payload_ref
        .as_ref()
        .expect("redacted must populate prompt_payload_ref");
    let rref = m
        .response_payload_ref
        .as_ref()
        .expect("redacted must populate response_payload_ref");

    let prompt_bytes = store.read(&BlobRef(pref.clone())).expect("prompt blob");
    let prompt_str = std::str::from_utf8(&prompt_bytes).unwrap();
    assert!(
        !prompt_str.contains("sk-ant-api03-AAAAAAAA"),
        "redacted prompt blob must not contain raw secret, got: {prompt_str}"
    );

    let resp_bytes = store.read(&BlobRef(rref.clone())).expect("response blob");
    let resp_str = std::str::from_utf8(&resp_bytes).unwrap();
    assert!(
        !resp_str.contains("sk-ant-api03-BBBBBBBB"),
        "redacted response blob must not contain raw secret, got: {resp_str}"
    );
}
