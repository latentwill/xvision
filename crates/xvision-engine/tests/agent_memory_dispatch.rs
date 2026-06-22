//! V2D dispatcher wiring — integration tests for memory recall/write.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::TimeZone;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::agent::memory_recorder::{MemoryRecorder, RecallResult};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMode, Tier};
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus};

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
        source_window_start: None,
        source_window_end: None,
        training_window_end: None,
        promotion_state: Some("active".into()),
        attestation_id: Some("attest-test".into()),
        forgotten_at: None,
    }
}

#[tokio::test]
async fn recall_returns_empty_when_mode_is_off() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let recorder = MemoryRecorder::new(std::sync::Arc::new(store));
    let r = recorder
        .recall(MemoryMode::Off, "agent-1", "any query text", 5, None, 0)
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
    let recorder =
        MemoryRecorder::with_static_embedder(std::sync::Arc::new(store), "test-embedder", vec![1.0, 0.0]);
    // memory-provenance-in-decisions-trace: thread a non-zero
    // decision_id so the test exercises the new echo-back field on
    // `RecallResult::Hits`. The recall feeds into decision-cycle 42.
    let r = recorder
        .recall(MemoryMode::AgentScoped, "agent-1", "query", 5, None, 42)
        .await
        .unwrap();
    match r {
        RecallResult::Hits {
            matches,
            namespace,
            decision_id,
        } => {
            assert_eq!(namespace, "agent:agent-1");
            assert_eq!(matches.len(), 2);
            assert_eq!(decision_id, 42, "recall must echo decision_id verbatim");
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
            chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 1, 0).unwrap(),
        )
        .await
        .unwrap();
    // Observations are not visible via recall; assert via a direct
    // SQL probe so we can prove the write landed.
    let row: (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(source_window_end) FROM memory_items WHERE namespace = ? AND tier = 'observation'")
            .bind("agent:agent-1")
            .fetch_one(store_arc.pool())
            .await
            .unwrap();
    assert_eq!(row.0, 1);
    assert!(row.1.is_some(), "Observation must persist source_window_end");
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

async fn phase0_leakage_probe_prompt() -> String {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    let scenario_start = chrono::Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let before_window = chrono::Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap();
    let inside_window = chrono::Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap();

    let mut high = pattern_item(
        "phase0-high",
        "agent:phase0",
        "PHASE0_SAFE_HIGH_SCORE",
        vec![1.0, 0.0],
    );
    high.training_window_end = Some(before_window);
    store_arc.upsert_pattern(&high, "test-embedder").await.unwrap();

    let mut lower = pattern_item(
        "phase0-lower",
        "agent:phase0",
        "PHASE0_SAFE_LOWER_SCORE",
        vec![0.8, 0.2],
    );
    lower.training_window_end = Some(before_window);
    store_arc.upsert_pattern(&lower, "test-embedder").await.unwrap();

    let mut temporal_leak = pattern_item(
        "phase0-temporal-leak",
        "agent:phase0",
        "PHASE0_TEMPORAL_LEAK",
        vec![0.99, 0.01],
    );
    temporal_leak.training_window_end = Some(inside_window);
    store_arc
        .upsert_pattern(&temporal_leak, "test-embedder")
        .await
        .unwrap();

    let mut staged = pattern_item(
        "phase0-staged",
        "agent:phase0",
        "PHASE0_STAGED_LEAK",
        vec![0.98, 0.02],
    );
    staged.training_window_end = Some(before_window);
    staged.promotion_state = Some("staged".into());
    store_arc.upsert_pattern(&staged, "test-embedder").await.unwrap();

    let mut wrong_ns = pattern_item("phase0-global", "global", "PHASE0_GLOBAL_LEAK", vec![0.97, 0.03]);
    wrong_ns.training_window_end = Some(before_window);
    store_arc
        .upsert_pattern(&wrong_ns, "test-embedder")
        .await
        .unwrap();

    let observation = MemoryItem {
        id: "phase0-observation".into(),
        namespace: "agent:phase0".into(),
        tier: Tier::Observation,
        text: "PHASE0_OBSERVATION_LEAK".into(),
        embedding: vec![1.0, 0.0],
        created_at: before_window,
        run_id: Some("run-phase0".into()),
        scenario_id: Some("scenario-phase0".into()),
        cycle_idx: Some(1),
        source_window_start: Some(before_window),
        source_window_end: Some(before_window),
        training_window_end: None,
        promotion_state: None,
        attestation_id: None,
        forgotten_at: None,
    };
    store_arc
        .upsert_observation(&observation, "test-embedder")
        .await
        .unwrap();

    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![1.0, 0.0],
    ));
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"hold","conviction":0.5,"justification":"phase0"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "PHASE0_BASE_PROMPT".into(),
        upstream_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "phase0".into(),
        scenario_start: Some(scenario_start),
        source_window_start: Some(scenario_start),
        source_window_end: Some(scenario_start),
        run_id: "run-phase0".into(),
        scenario_id: "scenario-phase0".into(),
        cycle_idx: 1,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();

    dispatch.last_system_prompt()
}

#[tokio::test]
async fn phase0_leakage_regression_harness_flt_prompt_is_deterministic() {
    let first = phase0_leakage_probe_prompt().await;
    let second = phase0_leakage_probe_prompt().await;

    assert_eq!(
        first, second,
        "identical leakage probes must render byte-identical prompts"
    );
    assert!(first.contains("<prior_observations>"));
    assert!(first.contains("A prior decision noted:"));
    assert!(first.contains("Consider whether this situation matches the present cycle."));
    assert!(first.contains("PHASE0_SAFE_HIGH_SCORE"));
    assert!(first.contains("PHASE0_SAFE_LOWER_SCORE"));
    assert!(first.contains("PHASE0_BASE_PROMPT"));
    assert!(!first.contains("PHASE0_OBSERVATION_LEAK"));
    assert!(!first.contains("PHASE0_TEMPORAL_LEAK"));
    assert!(!first.contains("PHASE0_STAGED_LEAK"));
    assert!(!first.contains("PHASE0_GLOBAL_LEAK"));

    let high_idx = first.find("PHASE0_SAFE_HIGH_SCORE").unwrap();
    let lower_idx = first.find("PHASE0_SAFE_LOWER_SCORE").unwrap();
    let base_idx = first.find("PHASE0_BASE_PROMPT").unwrap();
    assert!(high_idx < lower_idx, "higher-score Pattern must render first");
    assert!(lower_idx < base_idx, "memory block must precede base prompt");
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
        attested_with: "anthropic.claude-sonnet-4.6".into(),
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
        system_prompt: "BASE_SYSTEM_PROMPT".into(),
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
        source_window_start: None,
        source_window_end: None,
        run_id: "run-fixture".into(),
        scenario_id: "scenario-fixture".into(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
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
            &pattern_item(
                "p1",
                "agent:agent-caselaw",
                "RANGE_BOUND_FADES_BREAKOUTS",
                vec![1.0, 0.0],
            ),
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
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("{}"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "BASE".into(),
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
        source_window_start: None,
        source_window_end: None,
        run_id: "r".into(),
        scenario_id: "s".into(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
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
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("{}"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let scenario_start = chrono::Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();

    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "BASE".into(),
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
        source_window_start: Some(scenario_start),
        source_window_end: Some(scenario_start),
        run_id: "r".into(),
        scenario_id: "s".into(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
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
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("FINAL_DECISION_FIXTURE_TEXT"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let source_window_start = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let source_window_end = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 15, 0).unwrap();

    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "BASE_SYSTEM_PROMPT".into(),
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
        source_window_start: Some(source_window_start),
        source_window_end: Some(source_window_end),
        run_id: "run-fixture".into(),
        scenario_id: "scenario-fixture".into(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
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

#[tokio::test]
async fn execute_slot_skips_observation_write_without_source_window() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    let recorder = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![0.25, 0.75],
    ));

    let recorder_obs = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder_obs.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-missing-window");

    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    let dispatch = Arc::new(CapturingDispatch::new("MISSING_WINDOW_DECISION"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "BASE_SYSTEM_PROMPT".into(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter),
        memory: Some(recorder),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-missing-window".into(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-missing-window".into(),
        scenario_id: "scenario-missing-window".into(),
        cycle_idx: 9,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();

    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory_items WHERE namespace = ? AND tier = 'observation'")
            .bind("agent:agent-missing-window")
            .fetch_one(store_arc.pool())
            .await
            .unwrap();
    assert_eq!(
        row.0, 0,
        "execute_slot must not synthesize source windows for Observation writes",
    );

    let events = drain_events(&bus, &recorder_obs).await;
    assert!(
        !events.iter().any(|e| matches!(e, RunEvent::MemoryWrite(_))),
        "missing source windows must not emit a MemoryWrite event",
    );
    let missing = events
        .iter()
        .find_map(|e| match e {
            RunEvent::EngineEvent(e) if e.kind == "memory_write_missing_source_window" => Some(e),
            _ => None,
        })
        .expect("missing source windows must emit a typed engine event");
    assert_eq!(missing.run_id, "run-missing-window");
    let payload: serde_json::Value = serde_json::from_str(missing.payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload["flywheel_cycle_id"], "run-missing-window:9");
    assert_eq!(payload["namespace"], "agent:agent-missing-window");
    assert_eq!(payload["missing_source_window_start"], true);
    assert_eq!(payload["missing_source_window_end"], true);
}

/// Build a minimal `Strategy` whose `agents` slot is populated so
/// `run_pipeline` takes the agent-pipeline branch (the one this PR
/// wired the recorder into) rather than the legacy regime/trader
/// branch. The legacy slots are cleared explicitly so the
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
            attested_with: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::sequential(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
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
/// Under F+L+T the test pre-seeds one Pattern so recall has a legal
/// tier to surface, then asserts the recorder writes exactly one new
/// Observation for the executed slot.
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
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: "decide".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-pipeline-fixture".into(),
        noop_skip: true,
        nano: None,
    }];

    // Use "long_open" not "hold" — flat/hold are skipped by the memory write
    // path (U7: skip non-state-changing decisions). The test verifies the
    // recorder path, so it must use an action that produces an Observation.
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.6,"justification":"PIPELINE_THREADED_DECISION"}"#,
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
        source_window_start: Some(chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
        source_window_end: Some(chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 15, 0).unwrap()),
        run_id: "pipeline-run-1".into(),
        scenario_id: "pipeline-scenario-1".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
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
    let pattern_row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory_items WHERE namespace = ? AND tier = 'pattern'")
            .bind("agent:agent-pipeline-fixture")
            .fetch_one(store_arc.pool())
            .await
            .unwrap();
    assert_eq!(pattern_row.0, 1, "preseed pattern must survive");
}

/// Drain events from the bus by quiescing + yielding briefly so the
/// `NoopRecorder`'s internal consumer task has time to process them.
async fn drain_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

/// memory-provenance-in-decisions-trace: when `execute_slot` runs with
/// an `ObsEmitter` wired + V2D memory mode + recall hits, the emitter
/// must publish a `RunEvent::MemoryRecall` carrying:
///   - `decision_id` matching the `SlotInput.cycle_idx` argument
///   - the recall set's item ids
///   - the namespace the recall resolved
///
/// Anything less leaves the eval-review surface unable to answer
/// "which memories drove decision N" — the foundation finding this
/// contract unblocks (`memory-aware-eval-findings`) depends on the
/// `(run_id, decision_id, memory_item_id)` tuple landing on the bus.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_slot_emits_memory_recall_event_with_decision_id() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = Arc::new(store);
    for (id, text) in [
        ("recall_m1", "FIRST_RECALL_FIXTURE"),
        ("recall_m2", "SECOND_RECALL_FIXTURE"),
    ] {
        store_arc
            .upsert_pattern(
                &pattern_item(id, "agent:agent-prov", text, vec![1.0, 0.0]),
                "test-embedder",
            )
            .await
            .unwrap();
    }
    let recorder_memory = Arc::new(MemoryRecorder::with_static_embedder(
        Arc::clone(&store_arc),
        "test-embedder",
        vec![1.0, 0.0],
    ));

    let recorder_obs = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder_obs.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-prov-fixture");

    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    };
    // Use "long_open" not "hold" — flat/hold are skipped by the memory write
    // path (U7: skip non-state-changing decisions). The test verifies both
    // the MemoryWrite and MemoryRecall events, so it must trigger a write.
    let dispatch = Arc::new(CapturingDispatch::new(
        r#"{"action":"long_open","conviction":0.6,"justification":"prov"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    // Drive cycle_idx=7 — the emitted MemoryRecall.decision_id must
    // echo this back. Empty cycle_idx (0) would still pass the type
    // check but would let a silent off-by-one in the threading bypass
    // the test, so we pick a non-zero number.
    execute_slot(SlotInput {
        slot: &slot,
        system_prompt: "BASE".into(),
        upstream_inputs: serde_json::json!({"ohlcv_history": []}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter),
        memory: Some(recorder_memory),
        memory_mode: MemoryMode::AgentScoped,
        agent_id: "agent-prov".into(),
        scenario_start: None,
        source_window_start: Some(chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
        source_window_end: Some(chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 15, 0).unwrap()),
        run_id: "run-prov-fixture".into(),
        scenario_id: "scenario-prov".into(),
        cycle_idx: 7,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .expect("execute_slot must succeed");

    let events = drain_events(&bus, &recorder_obs).await;
    let recall = events
        .iter()
        .find_map(|e| match e {
            RunEvent::MemoryRecall(m) => Some(m),
            _ => None,
        })
        .expect(
            "execute_slot must publish a MemoryRecall event when ObsEmitter is wired \
             + memory mode is non-Off + recall returns hits",
        );

    assert_eq!(recall.run_id, "run-prov-fixture");
    assert_eq!(recall.flywheel_cycle_id.as_deref(), Some("run-prov-fixture:7"));
    assert_eq!(
        recall.decision_id, 7,
        "MemoryRecall.decision_id must match SlotInput.cycle_idx — \
         this is the per-decision provenance the contract installs",
    );
    assert_eq!(recall.namespace, "agent:agent-prov");

    let item_ids: Vec<String> = recall.items.iter().map(|it| it.id.clone()).collect();
    assert!(
        item_ids.contains(&"recall_m1".to_string()),
        "MemoryRecall.items must carry the recall set; got: {item_ids:?}",
    );
    assert!(
        item_ids.contains(&"recall_m2".to_string()),
        "MemoryRecall.items must carry the recall set; got: {item_ids:?}",
    );

    // Each item must carry a non-empty score + preview so the dashboard
    // has enough to render without re-querying the memory store.
    for item in &recall.items {
        assert!(
            item.score.is_finite() && item.score > 0.0,
            "recall item must carry a finite positive score; got: {}",
            item.score,
        );
        assert!(
            !item.text_preview.is_empty(),
            "recall item must carry a non-empty text_preview",
        );
    }

    let write = events
        .iter()
        .find_map(|e| match e {
            RunEvent::MemoryWrite(m) => Some(m),
            _ => None,
        })
        .expect("execute_slot must publish a MemoryWrite event after recording the final decision");
    assert_eq!(write.run_id, "run-prov-fixture");
    assert_eq!(write.flywheel_cycle_id.as_deref(), Some("run-prov-fixture:7"));
    assert_eq!(write.decision_id, 7);
    assert_eq!(write.namespace, "agent:agent-prov");
    assert!(!write.memory_item_id.is_empty());
    // The preview is derived from the EpisodicObservation JSON; check that
    // it carries the action from the mock response (updated from "hold" to
    // "long_open" when the U7 flat/hold skip was added — holds are skipped).
    assert!(write.text_preview.contains("long_open"));
}
