//! WS-17 (reasoning capture) — engine-side `<think>` capture.
//!
//! The engine strips `<think>…</think>` blocks out of a CoT model's raw
//! text before JSON-extracting the trader decision (so reasoning traces
//! don't shadow the `{…}` decision object). Historically that strip
//! DISCARDED the chain-of-thought — the highest-signal "why" for the
//! flywheel. This track captures the inline reasoning at the strip site
//! and emits it as a `decision.reasoning` span (blob-backed + redacted),
//! nested under the `decision.model` span when one is threaded in.
//!
//! Three properties are asserted:
//!
//! 1. Clean-body invariance — `strip_and_capture_think_blocks` returns
//!    the same clean body the trader has always parsed (think block
//!    removed), AND surfaces the captured reasoning text separately.
//! 2. Emission + nesting — `emit_model_reasoning` opens a
//!    `SpanKind::DecisionReasoning` span as a CHILD of the supplied
//!    `decision.model` span id, carrying the reasoning text under full_debug.
//! 3. Retention gating — under `hash_only` NO reasoning body is stored
//!    (no blob written, no raw text on the span attributes); only the
//!    char-count is recorded for cost legibility.

use std::sync::Arc;

use xvision_engine::agent::execute_cline::strip_and_capture_think_blocks;
use xvision_engine::agent::observability::{ObsEmitter, ObsRetentionPolicy};
use xvision_observability::{
    BlobRef, BlobStore, NoopRecorder, ObservabilityConfig, RetentionMode, RunEvent, RunEventBus, SpanKind,
};

fn policy_for(mode: RetentionMode) -> ObsRetentionPolicy {
    let mut cfg = ObservabilityConfig::default();
    cfg.retention.mode = mode;
    cfg.retention.store_prompts = true;
    cfg.retention.store_responses = true;
    cfg.retention.max_payload_bytes = 200_000;
    ObsRetentionPolicy::from_config(&cfg)
}

/// Drain helper — mirrors `agent_observability_blob.rs`. The bus consumer
/// runs in a separate task, so quiesce + yield until the published events
/// surface on the `NoopRecorder` snapshot.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn find_reasoning_started(events: &[RunEvent]) -> &xvision_observability::SpanStartedEvent {
    events
        .iter()
        .find_map(|e| match e {
            RunEvent::SpanStarted(s) if s.kind == SpanKind::DecisionReasoning => Some(s),
            _ => None,
        })
        .expect("a SpanStarted(DecisionReasoning) event must be published")
}

const SAMPLE: &str = "<think>The 1h trend is up and RSI just crossed 30, so a long looks favorable.</think>\
     {\"action\":\"long_open\",\"conviction\":0.7,\"justification\":\"momentum\"}";

const CLEAN: &str = "{\"action\":\"long_open\",\"conviction\":0.7,\"justification\":\"momentum\"}";
const REASONING: &str = "The 1h trend is up and RSI just crossed 30, so a long looks favorable.";

#[test]
fn strip_and_capture_returns_clean_body_and_reasoning() {
    let (clean, reasoning) = strip_and_capture_think_blocks(SAMPLE);
    // The trader still parses byte-identical clean JSON.
    assert_eq!(
        clean, CLEAN,
        "clean body must be byte-identical to the historic strip output"
    );
    // The chain-of-thought is now surfaced instead of discarded.
    assert_eq!(
        reasoning.as_deref(),
        Some(REASONING),
        "captured reasoning must equal the inner <think> text"
    );
}

#[test]
fn strip_and_capture_no_think_block_returns_none() {
    let (clean, reasoning) = strip_and_capture_think_blocks(CLEAN);
    assert_eq!(clean, CLEAN);
    assert!(reasoning.is_none(), "no <think> block ⇒ no reasoning captured");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_debug_emits_decision_reasoning_span_nested_under_decision_model() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-reasoning-fd")
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(store.clone());

    let decision_model_span = "decision-model-span-1";
    emitter
        .emit_model_reasoning(Some(decision_model_span.to_string()), REASONING)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_reasoning_started(&events);

    // (a) nested under the decision.model span.
    assert_eq!(
        started.parent_span_id.as_deref(),
        Some(decision_model_span),
        "decision.reasoning span must be a child of the decision.model span"
    );

    let attrs: serde_json::Value = serde_json::from_str(
        started
            .attributes_json
            .as_deref()
            .expect("attributes_json present"),
    )
    .expect("attributes_json is JSON");

    // char count is always recorded for cost legibility.
    assert_eq!(
        attrs["reasoning_char_count"].as_u64(),
        Some(REASONING.chars().count() as u64),
        "reasoning_char_count must be recorded"
    );
    // Under full_debug the reasoning body is persisted to a blob.
    let blob_ref = attrs["reasoning_blob_ref"]
        .as_str()
        .expect("full_debug must populate reasoning_blob_ref");
    let bytes = store
        .read(&BlobRef(blob_ref.to_string()))
        .expect("reasoning blob");
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        REASONING,
        "stored reasoning blob must round-trip the chain-of-thought verbatim"
    );

    // The span must be closed (SpanFinished) so it doesn't dangle open.
    let finished = events
        .iter()
        .any(|e| matches!(e, RunEvent::SpanFinished(f) if f.span_id == started.span_id));
    assert!(finished, "decision.reasoning span must be closed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hash_only_stores_no_reasoning_body() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-reasoning-hash")
        .with_retention(policy_for(RetentionMode::HashOnly))
        .with_blob_store(store.clone());

    emitter
        .emit_model_reasoning(Some("decision-model-span-2".to_string()), REASONING)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_reasoning_started(&events);

    let attrs: serde_json::Value = serde_json::from_str(
        started
            .attributes_json
            .as_deref()
            .expect("attributes_json present"),
    )
    .expect("attributes_json is JSON");

    // char count is still recorded under hash_only (no raw body leak).
    assert_eq!(
        attrs["reasoning_char_count"].as_u64(),
        Some(REASONING.chars().count() as u64),
        "reasoning_char_count must be recorded even under hash_only"
    );
    // No blob ref, no inline body.
    assert!(
        attrs
            .get("reasoning_blob_ref")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "hash_only must NOT populate reasoning_blob_ref, got {:?}",
        attrs.get("reasoning_blob_ref")
    );
    assert!(
        attrs.get("reasoning_text").map(|v| v.is_null()).unwrap_or(true),
        "hash_only must NOT inline the reasoning_text, got {:?}",
        attrs.get("reasoning_text")
    );

    // Nothing on disk.
    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .map(|d| d.collect::<Result<Vec<_>, _>>().unwrap_or_default())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hash_only must not touch the blob store, found: {entries:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redacted_scrubs_secret_from_reasoning_blob() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let leaky = "decided to long after seeing key sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-end in the logs";

    let emitter = ObsEmitter::new(bus.clone(), "run-reasoning-redacted")
        .with_retention(policy_for(RetentionMode::Redacted))
        .with_blob_store(store.clone());

    emitter
        .emit_model_reasoning(Some("decision-model-span-3".to_string()), leaky)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_reasoning_started(&events);
    let attrs: serde_json::Value = serde_json::from_str(
        started
            .attributes_json
            .as_deref()
            .expect("attributes_json present"),
    )
    .expect("attributes_json is JSON");

    let blob_ref = attrs["reasoning_blob_ref"]
        .as_str()
        .expect("redacted must populate reasoning_blob_ref");
    let bytes = store
        .read(&BlobRef(blob_ref.to_string()))
        .expect("reasoning blob");
    let stored = std::str::from_utf8(&bytes).unwrap();
    assert!(
        !stored.contains("sk-ant-api03-AAAAAAAA"),
        "redacted reasoning blob must not contain the raw secret, got: {stored}"
    );
}
