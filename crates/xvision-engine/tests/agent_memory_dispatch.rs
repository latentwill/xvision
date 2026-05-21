//! V2D dispatcher wiring — integration tests for memory recall/write.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::TimeZone;
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
use xvision_memory::types::{MemoryItem, MemoryMode, Tier};

fn pattern_item(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Pattern,
        text: text.into(),
        embedding: emb,
        created_at: chrono::Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        training_window_end: None,
    }
}

#[tokio::test]
async fn recall_returns_empty_when_mode_is_off() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let recorder = MemoryRecorder::new(std::sync::Arc::new(store));
    let r = recorder
        .recall(MemoryMode::Off, "agent-1", "any query text", 5, None)
        .await
        .unwrap();
    assert!(matches!(r, RecallResult::Skipped));
}

#[tokio::test]
async fn recall_returns_top_k_for_agent_scoped() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // Pre-seed two Patterns in the agent-scoped namespace. Recall is
    // Patterns-only, so seeding Observations here would never surface.
    for (id, text) in [("m1", "first note"), ("m2", "second note")] {
        store
            .upsert_pattern(
                &pattern_item(id, "agent:agent-1", text, vec![1.0, 0.0]),
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
        .recall(MemoryMode::AgentScoped, "agent-1", "query", 5, None)
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
async fn record_writes_observation_into_correct_namespace() {
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
            "run-1".into(),
            "scenario-1".into(),
            7,
        )
        .await
        .unwrap();
    // Observations are not visible via recall; assert via a direct
    // SQL probe so we can prove the write landed.
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_items WHERE namespace = ? AND tier = 'observation'",
    )
    .bind("agent:agent-1")
    .fetch_one(store_arc.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);
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
    // Arrange: pre-seed two PATTERNS in the agent-scoped namespace,
    // build a MemoryRecorder with a deterministic StaticEmbedder, and
    // attach it to SlotInput. The CapturingDispatch then lets us peek
    // at the LlmRequest.system_prompt the recall+assembly seam handed
    // to the dispatcher. Recall is Patterns-only under F+L+T, so the
    // pre-seed MUST be Patterns or nothing would surface.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    for (id, text) in [
        ("m1", "FIRST_PRIOR_OBS_FIXTURE"),
        ("m2", "SECOND_PRIOR_OBS_FIXTURE"),
    ] {
        store_arc
            .upsert_pattern(
                &pattern_item(id, "agent:agent-xyz", text, vec![1.0, 0.0]),
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
        scenario_start: None,
        run_id: "run-fixture".into(),
        scenario_id: "scenario-fixture".into(),
        cycle_idx: 0,
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
        "expected first pre-seeded pattern in system_prompt, got: {sys}",
    );
    assert!(
        sys.contains("SECOND_PRIOR_OBS_FIXTURE"),
        "expected second pre-seeded pattern in system_prompt, got: {sys}",
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
async fn recall_wraps_each_pattern_in_caselaw_framing() {
    // Phase 1.5 L (rhetorical) wrapper — each recalled Pattern is
    // framed as a precedent ("A prior decision noted ... Consider
    // whether this situation matches the present cycle.") instead of a
    // raw bullet.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    store_arc
        .upsert_pattern(
            &pattern_item("p1", "agent:agent-caselaw", "RANGE_BOUND_FADES_BREAKOUTS", vec![1.0, 0.0]),
            "test-embedder",
        )
        .await
        .unwrap();

    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![1.0, 0.0],
    ));

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "BASE".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("{}"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-caselaw".into(),
        scenario_start: None,
        run_id: "r".into(),
        scenario_id: "s".into(),
        cycle_idx: 0,
    })
    .await
    .unwrap();

    let sys = dispatch.last_system_prompt();
    assert!(
        sys.contains("A prior decision noted:"),
        "case-law wrapper opener missing in system_prompt: {sys}",
    );
    assert!(
        sys.contains("Consider whether this situation matches the present cycle."),
        "case-law wrapper closer missing in system_prompt: {sys}",
    );
    assert!(
        sys.contains("RANGE_BOUND_FADES_BREAKOUTS"),
        "pattern body missing in system_prompt: {sys}",
    );
}

#[tokio::test]
async fn recall_excludes_pattern_when_training_window_overlaps_scenario() {
    // Phase 1.5 T (temporal) leakage guard — a Pattern whose
    // training_window_end falls AFTER the current scenario start must
    // be filtered out, even though it lives in the right namespace.
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    let mut p = pattern_item(
        "p1",
        "agent:agent-leakage",
        "TRAINED_INSIDE_THE_REPLAY_WINDOW",
        vec![1.0, 0.0],
    );
    p.training_window_end = Some(chrono::Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap());
    store_arc.upsert_pattern(&p, "test-embedder").await.unwrap();

    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![1.0, 0.0],
    ));

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "BASE".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("{}"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let scenario_start = chrono::Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();

    execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-leakage".into(),
        scenario_start: Some(scenario_start),
        run_id: "r".into(),
        scenario_id: "s".into(),
        cycle_idx: 0,
    })
    .await
    .unwrap();

    let sys = dispatch.last_system_prompt();
    assert!(
        !sys.contains("<prior_observations>"),
        "training window overlap must suppress the prior_observations block entirely, got: {sys}",
    );
    assert!(
        !sys.contains("TRAINED_INSIDE_THE_REPLAY_WINDOW"),
        "leaked pattern body present in system_prompt: {sys}",
    );
}

#[tokio::test]
async fn execute_slot_writes_final_decision_into_namespace() {
    // Companion to the recall test: after the final EndTurn turn the
    // recorder should have a new Observation entry in the agent-scoped
    // namespace carrying the assistant's final text.
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
        scenario_start: None,
        run_id: "run-fixture".into(),
        scenario_id: "scenario-fixture".into(),
        cycle_idx: 0,
    })
    .await
    .unwrap();

    // Observations aren't returned by `query` (Patterns-only), so probe
    // SQL directly to assert exactly one Observation landed with the
    // final assistant text.
    let row: (i64, String) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(GROUP_CONCAT(text), '') \
         FROM memory_items WHERE namespace = ? AND tier = 'observation'",
    )
    .bind("agent:agent-xyz")
    .fetch_one(store_arc.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);
    assert!(row.1.contains("FINAL_DECISION_FIXTURE_TEXT"));
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
/// Builds a `PipelineInputs` whose `memory_recorder` is `Some` and one
/// `ResolvedAgentSlot` with `memory_mode = AgentScoped` + a non-empty
/// `agent_id`. After `run_pipeline` returns, the recorder's store must
/// contain exactly one new Observation in the agent-scoped namespace —
/// proving the recall+write seam fired through the pipeline call site.
///
/// Under F+L+T this test pre-seeds nothing (a pre-seed Observation
/// would never surface to recall, and a pre-seed Pattern would change
/// the assertion shape from the original test); instead it just
/// asserts the count goes from 0 → 1.
#[tokio::test]
async fn pipeline_threads_memory_recorder_to_execute_slot() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);

    // Pre-seed one Pattern in the agent-scoped namespace so recall has
    // something to surface AND so we can prove the recorder ran by
    // counting Observations (which only the recorder writes) post-run.
    // Patterns + Observations live in the same table, distinguished by
    // tier — so the assertion after run_pipeline checks the
    // tier='observation' count specifically.
    store_arc
        .upsert_pattern(
            &pattern_item(
                "preseed-pattern-1",
                "agent:agent-pipeline-fixture",
                "PRESEED_PATTERN_FIXTURE",
                vec![0.5, 0.5],
            ),
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
        // Live/paper-style call (no temporal filter) so the preseed
        // Pattern surfaces to recall — proving the recall path actually
        // ran inside execute_slot.
        scenario_start: None,
        run_id: "pipeline-run-1".into(),
        scenario_id: "pipeline-scenario-1".into(),
        cycle_idx: 0,
    })
    .await
    .expect("run_pipeline must succeed");
    assert!(
        outs.trader.is_some(),
        "trader-role slot must populate PipelineOutputs.trader",
    );

    // Observations live alongside Patterns in the same table — probe
    // via direct SQL because recall (Patterns-only) hides them. Expect
    // exactly one new Observation carrying the assistant's final text.
    let row: (i64, String) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(GROUP_CONCAT(text), '') \
         FROM memory_items WHERE namespace = ? AND tier = 'observation'",
    )
    .bind("agent:agent-pipeline-fixture")
    .fetch_one(store_arc.pool())
    .await
    .unwrap();
    assert_eq!(
        row.0, 1,
        "pipeline must write exactly one new Observation; got count={}, texts={}",
        row.0, row.1,
    );
    assert!(
        row.1.contains("PIPELINE_THREADED_DECISION"),
        "new Observation must carry the assistant's final text, proving the \
         recorder ran inside execute_slot via the pipeline; got texts={}",
        row.1,
    );

    // And the preseed Pattern must still be there (recorder must not
    // blow away namespace contents).
    let pattern_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_items WHERE namespace = ? AND tier = 'pattern'",
    )
    .bind("agent:agent-pipeline-fixture")
    .fetch_one(store_arc.pool())
    .await
    .unwrap();
    assert_eq!(pattern_row.0, 1, "preseed pattern must survive");
}
