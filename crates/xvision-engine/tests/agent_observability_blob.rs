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

use xvision_engine::agent::llm::{LlmRequest, Message, ResponseSchema};
use xvision_engine::agent::observability::{ObsEmitter, ObsRetentionPolicy};
use xvision_observability::{
    BlobRef, BlobStore, ModelCallFinishedEvent, NoopRecorder, ObservabilityConfig, RetentionMode, RunEvent,
    RunEventBus,
};

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
        cache_control: None,
        force_json: false,
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
    assert_eq!(std::str::from_utf8(&resp_bytes).unwrap(), "The answer is 4.");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_blob_includes_response_schema_for_reconstruction() {
    // PR #319 review (P2): the persisted prompt blob must carry the
    // full LlmRequest, not just the hash input. Anthropic appends
    // `response_schema` into the system prompt at dispatch time, so a
    // FullDebug blob built only from (system_prompt, messages, tools)
    // would silently drop the schema instructions on trader calls.
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let mut req = user_prompt();
    req.response_schema = Some(ResponseSchema::trader_output());

    let emitter = ObsEmitter::new(bus.clone(), "run-schema-blob")
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(store.clone());

    emitter
        .emit_model_call_finished_with_payloads(
            "span-schema",
            "anthropic",
            "claude-sonnet-4-6",
            Some(7),
            Some(11),
            None,
            "sha256:dummy".to_string(),
            Some("sha256:dummy".to_string()),
            Some(&req),
            Some("ok"),
        )
        .await;

    let events = collect_events(&bus, &recorder).await;
    let m = find_finished(&events);
    let pref = m
        .prompt_payload_ref
        .as_ref()
        .expect("full_debug must populate prompt_payload_ref");
    let prompt_bytes = store.read(&BlobRef(pref.clone())).expect("prompt blob");
    let parsed: serde_json::Value = serde_json::from_slice(&prompt_bytes).expect("prompt blob is JSON");

    assert_eq!(
        parsed["response_schema"]["name"].as_str(),
        Some("trader_output"),
        "blob must include response_schema; got: {parsed:#?}",
    );
    assert!(
        parsed["model"].as_str().is_some(),
        "blob must include the requested model id",
    );
    assert!(
        parsed["max_tokens"].is_number(),
        "blob must include max_tokens (sampling knobs aid reconstruction)",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blob_writes_honor_max_payload_bytes_cap() {
    // PR #319 review (P2): both prompt and response blob writes must
    // apply the configured `max_payload_bytes` cap before
    // BlobStore::write — the body path already does via apply_to_body;
    // before this fix the blob path bypassed it and a huge prompt
    // lived full-size until the janitor's eventual truncation pass.
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    // Configure a tiny cap. The serialized LlmRequest is well over
    // 64 bytes; the response is far over too.
    let mut cfg = ObservabilityConfig::default();
    cfg.retention.mode = RetentionMode::FullDebug;
    cfg.retention.store_prompts = true;
    cfg.retention.store_responses = true;
    cfg.retention.max_payload_bytes = 64;
    let policy = ObsRetentionPolicy::from_config(&cfg);

    let mut req = user_prompt();
    req.system_prompt = "x".repeat(10_000);
    let assistant = "y".repeat(10_000);

    let emitter = ObsEmitter::new(bus.clone(), "run-tiny-cap")
        .with_retention(policy)
        .with_blob_store(store.clone());

    emitter
        .emit_model_call_finished_with_payloads(
            "span-cap",
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
    let pref = m.prompt_payload_ref.as_ref().expect("prompt ref present");
    let rref = m.response_payload_ref.as_ref().expect("response ref present");

    let prompt_bytes = store.read(&BlobRef(pref.clone())).expect("prompt blob");
    let resp_bytes = store.read(&BlobRef(rref.clone())).expect("response blob");

    // Truncation appends the "…" marker (3 UTF-8 bytes). Both blobs
    // must be at or just past the cap, never the full 10_000+ bytes.
    let cap = 64;
    let marker_len = "…".len();
    assert!(
        prompt_bytes.len() <= cap + marker_len,
        "prompt blob exceeds cap+marker: {} > {}",
        prompt_bytes.len(),
        cap + marker_len,
    );
    assert!(
        resp_bytes.len() <= cap + marker_len,
        "response blob exceeds cap+marker: {} > {}",
        resp_bytes.len(),
        cap + marker_len,
    );
    assert!(
        prompt_bytes.ends_with("…".as_bytes()),
        "truncated prompt blob must end with the ellipsis marker",
    );
    assert!(
        resp_bytes.ends_with("…".as_bytes()),
        "truncated response blob must end with the ellipsis marker",
    );
}
