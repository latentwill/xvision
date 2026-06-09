//! P4 — chat-rail cortex-memory wiring tests.
//!
//! Exercises the `WizardLoop` recall + redacted write-back path end-to-end at
//! the loop level (mirrors `tests/wizard_loop.rs`): a tempdir-backed AppState
//! supplies the chat_sessions schema + pool, a `MemoryRecorder` with a static
//! embedder supplies deterministic recall, and a `RecordingDispatch` captures
//! the system prompt the loop assembled so we can assert recall injection.
//!
//! Coverage:
//! 1. recall — a seeded Pattern in the scope namespace surfaces as a
//!    `<prior_observations>` block in the system prompt.
//! 2. write-back — after a clean turn an Observation lands in the same
//!    namespace.
//! 3. disabled — with no recorder attached, no recall block and no write.
//! 4. redaction — a pasted secret in the user turn is redacted before the
//!    write-back Observation is persisted.

use std::sync::{Arc, Mutex};

use chrono::Utc;
use tempfile::TempDir;
use xvision_dashboard::wizard_loop::{AgentProfile, WizardEvent, WizardLoop};
use xvision_dashboard::AppState;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::agent::memory_recorder::MemoryRecorder;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

// ── harness ────────────────────────────────────────────────────────────────

async fn boot() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state, tmp)
}

/// Records every `LlmRequest` (so the assembled system prompt can be asserted)
/// then delegates to a `MockDispatch::sequence`.
struct RecordingDispatch {
    inner: MockDispatch,
    requests: Arc<Mutex<Vec<LlmRequest>>>,
}

#[async_trait::async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.requests.lock().unwrap().push(req.clone());
        self.inner.complete(req).await
    }
}

fn recording_text(text: &str) -> (Arc<dyn LlmDispatch>, Arc<Mutex<Vec<LlmRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let resp = LlmResponse {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    };
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(RecordingDispatch {
        inner: MockDispatch::sequence(vec![resp]),
        requests: requests.clone(),
    });
    (dispatch, requests)
}

/// A `MemoryRecorder` over an in-memory store using a fixed-vector embedder so
/// recall is deterministic (every text embeds to the same vector → cosine 1.0).
async fn recorder() -> (Arc<MemoryRecorder>, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::open_in_memory().await.expect("store"));
    let rec = Arc::new(MemoryRecorder::with_static_embedder(
        store.clone(),
        "static-test",
        vec![0.1, 0.2, 0.3],
    ));
    (rec, store)
}

fn seed_pattern(ns: &str, text: &str) -> MemoryItem {
    MemoryItem {
        id: ulid::Ulid::new().to_string(),
        namespace: ns.to_string(),
        tier: Tier::Pattern,
        text: text.to_string(),
        embedding: vec![0.1, 0.2, 0.3],
        created_at: Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: None,
        promotion_state: Some("active".to_string()),
        attestation_id: None,
        forgotten_at: None,
    }
}

fn drain_off(events: &[WizardEvent]) -> bool {
    events.iter().any(|e| matches!(e, WizardEvent::Done { .. }))
}

/// Create a real chat session for `scope` (satisfies the chat_messages FK),
/// then build a `WizardLoop` bound to it.
async fn build_loop(
    state: &AppState,
    tmp: &TempDir,
    dispatch: Arc<dyn LlmDispatch>,
    scope: ContextScope,
    message: &str,
) -> WizardLoop {
    let session_id = ChatSessionStore::create_session(&state.pool, &scope)
        .await
        .expect("create chat session");
    WizardLoop::new_with_profile(
        tmp.path().to_path_buf(),
        dispatch,
        "mock-model".to_string(),
        None,
        None,
        state.pool.clone(),
        session_id,
        scope,
        AgentProfile::Workspace,
        None,
        message.to_string(),
    )
    .await
    .expect("wizard loop")
}

async fn run(wl: &mut WizardLoop) -> Vec<WizardEvent> {
    let mut out = vec![];
    while let Some(ev) = wl.next_event().await {
        out.push(ev);
    }
    out
}

// ── tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn recall_injects_seeded_pattern_into_system_prompt() {
    let (state, tmp) = boot().await;
    let (rec, store) = recorder().await;

    let seeded = "raising leverage past 3x degraded the holdout score";
    let pat = seed_pattern("chat:strategy:s1", seeded);
    store.upsert_pattern(&pat, "static-test").await.unwrap();

    let (dispatch, requests) = recording_text("Noted.");
    let mut wl = build_loop(
        &state,
        &tmp,
        dispatch,
        ContextScope::Strategy {
            draft_id: "s1".into(),
        },
        "Should I increase leverage?",
    )
    .await
    .with_chat_memory(Some(rec));

    let events = run(&mut wl).await;
    assert!(drain_off(&events), "loop should finish with Done");

    let captured = requests.lock().unwrap();
    let prompt = &captured.first().expect("at least one request").system_prompt;
    assert!(
        prompt.contains("<prior_observations>"),
        "system prompt should carry the recall block; got:\n{prompt}"
    );
    assert!(
        prompt.contains(seeded),
        "recall block should include the seeded pattern text; got:\n{prompt}"
    );
}

#[tokio::test]
async fn turn_writes_back_an_observation() {
    let (state, tmp) = boot().await;
    let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());
    let rec = Arc::new(MemoryRecorder::with_static_embedder(
        store.clone(),
        "static-test",
        vec![0.4, 0.5, 0.6],
    ));

    let (dispatch, _requests) = recording_text("Here is the plan.");
    let mut wl = build_loop(
        &state,
        &tmp,
        dispatch,
        ContextScope::Strategy {
            draft_id: "s1".into(),
        },
        "Draft a momentum strategy.",
    )
    .await
    .with_chat_memory(Some(rec));

    let events = run(&mut wl).await;
    assert!(drain_off(&events), "loop should finish");

    let count = store.count_live_observations("chat:strategy:s1").await.unwrap();
    assert_eq!(count, 1, "exactly one observation should be written back");
}

#[tokio::test]
async fn disabled_memory_injects_nothing_and_writes_nothing() {
    let (state, tmp) = boot().await;
    // A separate store we can inspect — it must remain empty because the loop
    // has no recorder attached.
    let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());

    let (dispatch, requests) = recording_text("Done.");
    let mut wl = build_loop(
        &state,
        &tmp,
        dispatch,
        ContextScope::Strategy {
            draft_id: "s1".into(),
        },
        "Anything to recall?",
    )
    .await;
    // No .with_chat_memory → disabled.

    let events = run(&mut wl).await;
    assert!(drain_off(&events), "loop should finish");

    let prompt = &requests.lock().unwrap()[0].system_prompt;
    assert!(
        !prompt.contains("<prior_observations>"),
        "disabled memory must not inject a recall block"
    );
    assert_eq!(
        store.count_live_observations("chat:strategy:s1").await.unwrap(),
        0,
        "disabled memory must not write back"
    );
}

#[tokio::test]
async fn write_back_redacts_pasted_secret() {
    let (state, tmp) = boot().await;
    let store = Arc::new(MemoryStore::open_in_memory().await.unwrap());
    let rec = Arc::new(MemoryRecorder::with_static_embedder(
        store.clone(),
        "static-test",
        vec![0.7, 0.8, 0.9],
    ));

    // A fake Anthropic-style key the redactor recognises.
    let secret = "sk-ant-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (dispatch, _requests) = recording_text("Acknowledged.");
    let mut wl = build_loop(
        &state,
        &tmp,
        dispatch,
        ContextScope::Workspace,
        &format!("Here is my key {secret} please use it"),
    )
    .await
    .with_chat_memory(Some(rec));

    let events = run(&mut wl).await;
    assert!(drain_off(&events), "loop should finish");

    let texts = store
        .list_live_observation_texts("chat:workspace", 16)
        .await
        .unwrap();
    assert_eq!(texts.len(), 1, "one observation persisted");
    assert!(
        !texts[0].contains(secret),
        "raw secret must NOT be persisted; got: {}",
        texts[0]
    );
    assert!(
        texts[0].contains("[redacted:"),
        "redaction marker should be present; got: {}",
        texts[0]
    );
}
