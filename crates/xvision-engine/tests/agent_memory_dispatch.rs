//! V2D dispatcher wiring — integration tests for memory recall/write.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason};
use xvision_engine::agent::memory_recorder::{MemoryRecorder, RecallResult};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMode};

#[tokio::test]
async fn recall_returns_empty_when_mode_is_off() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let recorder = MemoryRecorder::new(std::sync::Arc::new(store));
    let r = recorder
        .recall(MemoryMode::Off, "agent-1", "any query text", 5)
        .await
        .unwrap();
    assert!(matches!(r, RecallResult::Skipped));
}

#[tokio::test]
async fn recall_returns_top_k_for_agent_scoped() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // Pre-seed two items in the agent-scoped namespace.
    for (id, text) in [("m1", "first note"), ("m2", "second note")] {
        store
            .upsert(
                &MemoryItem {
                    id: id.into(),
                    namespace: "agent:agent-1".into(),
                    text: text.into(),
                    embedding: vec![1.0, 0.0],
                    created_at: chrono::Utc::now(),
                    source_run_id: None,
                    source_cycle_id: None,
                },
                "test-embedder",
            )
            .await
            .unwrap();
    }
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::new(store),
        "test-embedder",
        vec![1.0, 0.0],
    );
    let r = recorder
        .recall(MemoryMode::AgentScoped, "agent-1", "query", 5)
        .await
        .unwrap();
    match r {
        RecallResult::Hits { matches, namespace } => {
            assert_eq!(namespace, "agent:agent-1");
            assert_eq!(matches.len(), 2);
        }
        other => panic!("expected Hits, got {other:?}"),
    }
}

#[tokio::test]
async fn record_writes_into_correct_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = std::sync::Arc::new(store);
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::clone(&store_arc),
        "test-embedder",
        vec![0.0, 1.0],
    );
    recorder
        .record(
            MemoryMode::AgentScoped,
            "agent-1",
            "decision text",
            None,
            None,
        )
        .await
        .unwrap();
    let hits = store_arc
        .query("agent:agent-1", &[0.0, 1.0], 5)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text, "decision text");
}

/// Dispatch double that captures every `LlmRequest` it observes so we can
/// assert on the assembled `system_prompt` `execute_slot` handed to the
/// dispatcher. Mirrors the inline `RecordingDispatch` in
/// `crates/xvision-engine/src/agent/execute.rs` test module — duplicated
/// here because that type lives behind `#[cfg(test)]` in the engine
/// crate and isn't visible to integration tests.
struct CapturingDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    response: LlmResponse,
}

impl CapturingDispatch {
    fn new(response_text: &str) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            response: LlmResponse {
                content: vec![ContentBlock::Text {
                    text: response_text.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        }
    }

    fn last_system_prompt(&self) -> String {
        self.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("dispatch was never called")
            .system_prompt
    }
}

#[async_trait]
impl LlmDispatch for CapturingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn execute_slot_prepends_prior_observations_when_agent_scoped() {
    // Arrange: pre-seed two memories in the agent-scoped namespace,
    // build a MemoryRecorder with a deterministic StaticEmbedder, and
    // attach it to SlotInput. The CapturingDispatch then lets us peek
    // at the LlmRequest.system_prompt the recall+assembly seam handed
    // to the dispatcher.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    for (id, text) in [
        ("m1", "FIRST_PRIOR_OBS_FIXTURE"),
        ("m2", "SECOND_PRIOR_OBS_FIXTURE"),
    ] {
        store_arc
            .upsert(
                &MemoryItem {
                    id: id.into(),
                    namespace: "agent:agent-xyz".into(),
                    text: text.into(),
                    embedding: vec![1.0, 0.0],
                    created_at: chrono::Utc::now(),
                    source_run_id: None,
                    source_cycle_id: None,
                },
                "test-embedder",
            )
            .await
            .unwrap();
    }
    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![1.0, 0.0],
    ));

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "BASE_SYSTEM_PROMPT".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({"ohlcv_history": []}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-xyz".into(),
    })
    .await
    .expect("execute_slot must succeed with CapturingDispatch");

    let sys = dispatch.last_system_prompt();
    assert!(
        sys.contains("<prior_observations>"),
        "expected <prior_observations> block in system_prompt, got: {sys}",
    );
    assert!(
        sys.contains("</prior_observations>"),
        "expected closing </prior_observations> tag, got: {sys}",
    );
    assert!(
        sys.contains("FIRST_PRIOR_OBS_FIXTURE"),
        "expected first pre-seeded memory in system_prompt, got: {sys}",
    );
    assert!(
        sys.contains("SECOND_PRIOR_OBS_FIXTURE"),
        "expected second pre-seeded memory in system_prompt, got: {sys}",
    );
    assert!(
        sys.contains("BASE_SYSTEM_PROMPT"),
        "expected original slot prompt preserved after recall block, got: {sys}",
    );
    // Prior observations must come BEFORE the original prompt body so
    // the model treats them as upstream context.
    let prior_idx = sys.find("<prior_observations>").unwrap();
    let base_idx = sys.find("BASE_SYSTEM_PROMPT").unwrap();
    assert!(
        prior_idx < base_idx,
        "prior_observations must precede base prompt body",
    );
}

#[tokio::test]
async fn execute_slot_writes_final_decision_into_namespace() {
    // Companion to the recall test: after the final EndTurn turn the
    // recorder should have a new entry in the agent-scoped namespace
    // carrying the assistant's final text. Uses a fresh in-memory
    // store so we can assert hit count == 1.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![0.25, 0.75],
    ));

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "BASE_SYSTEM_PROMPT".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("FINAL_DECISION_FIXTURE_TEXT"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-xyz".into(),
    })
    .await
    .unwrap();

    let hits = store_arc
        .query("agent:agent-xyz", &[0.25, 0.75], 5)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1, "memory_write must persist exactly one item");
    assert_eq!(hits[0].text, "FINAL_DECISION_FIXTURE_TEXT");
}

/// Build a minimal `Strategy` whose `agents` slot is populated so
/// `run_pipeline` takes the agent-pipeline branch (the one this PR
/// wired the recorder into) rather than the legacy regime/intern/
/// trader branch. The legacy slots are cleared explicitly so the
/// agent-pipeline path is the only path that fires.
fn pipeline_fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7ZPIPELINE_MEM".into(),
            display_name: "Pipeline Memory Threading".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            required_models: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::sequential(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

/// End-to-end smoke test for the V2D Phase 3 wiring bridge.
///
/// Builds a `PipelineInputs` whose `memory_recorder` is `Some`, holds a
/// pre-seeded namespace, and carries one `ResolvedAgentSlot` with
/// `memory_mode = AgentScoped` + a non-empty `agent_id`. After
/// `run_pipeline` returns, the recorder's store must contain a NEW item
/// under `agent:<agent_id>` (in addition to the pre-seed) — proving the
/// recall+write seam actually fired through the pipeline call site, not
/// just through `execute_slot` directly (which the sibling tests cover).
#[tokio::test]
async fn pipeline_threads_memory_recorder_to_execute_slot() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);

    // Pre-seed one memory in the agent-scoped namespace so we can
    // distinguish "the recorder fired and wrote a new item" from
    // "the recorder never touched the store".
    store_arc
        .upsert(
            &MemoryItem {
                id: "preseed-1".into(),
                namespace: "agent:agent-pipeline-fixture".into(),
                text: "PRESEED_FIXTURE".into(),
                embedding: vec![0.5, 0.5],
                created_at: chrono::Utc::now(),
                source_run_id: None,
                source_cycle_id: None,
            },
            "test-embedder",
        )
        .await
        .unwrap();

    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![0.5, 0.5],
    ));

    let strategy = pipeline_fixture_strategy();
    let agent_slots = vec![ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        max_tokens: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-pipeline-fixture".into(),
    }];

    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"PIPELINE_THREADED_DECISION"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: Some(recorder),
    })
    .await
    .expect("run_pipeline must succeed");
    assert!(
        outs.trader.is_some(),
        "trader-role slot must populate PipelineOutputs.trader",
    );

    // The store now contains the preseed plus exactly one new item that
    // carries the dispatched assistant text. If the recorder were not
    // threaded, the store would still hold only the preseed.
    let hits = store_arc
        .query("agent:agent-pipeline-fixture", &[0.5, 0.5], 10)
        .await
        .unwrap();
    assert_eq!(
        hits.len(),
        2,
        "pipeline must write exactly one new memory item alongside the preseed; \
         got {hits:?}",
    );
    assert!(
        hits.iter().any(|m| m.text.contains("PIPELINE_THREADED_DECISION")),
        "new memory item must carry the assistant's final text, proving the \
         recorder ran inside execute_slot via the pipeline; got {hits:?}",
    );
    assert!(
        hits.iter().any(|m| m.text == "PRESEED_FIXTURE"),
        "preseed must survive — the recall+write path must not blow away \
         existing namespace contents; got {hits:?}",
    );
}
