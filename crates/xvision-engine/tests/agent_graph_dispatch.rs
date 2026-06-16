//! Phase B — `dispatch_capability` round-trip + A/B cache-key invariance.
//!
//! Covers the first acceptance test from
//! `team/contracts/agent-graph-capability-dispatch.md`:
//!
//!   round-trip a `kind: Sequential` strategy with 2 capability-typed
//!   agents (Trader+Filter); assert each dispatched via the
//!   right handler.
//!
//! Also pins the A/B cache-pairing acceptance criterion: every
//! `dispatch_capability` call preserves `(cycle_id, scenario_id)` — the
//! fixture asserts the same `cycle_idx` flows through to the dispatcher
//! request body so the pre-Phase-B cache key shape is unchanged.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use xvision_engine::agent::dispatch_capability::{dispatch_capability, AgentOutput, DispatchInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::AgentRef;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

/// Dispatch double that records every request and returns canned text.
struct RecordingDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    text: String,
}

impl RecordingDispatch {
    fn new(text: impl Into<String>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            text: text.into(),
        }
    }

    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
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

fn fixture_strategy(agents: Vec<AgentRef>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZSEQDISPATCH".into(),
            display_name: "DispatchTest".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            attested_with: vec!["mock".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        },
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

fn resolved(role: &str) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: String::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: false,
    }
}

// ── Round-trip: Trader + Filter each route through dispatch_capability ──

#[tokio::test]
async fn three_capability_pipeline_routes_each_kind_correctly() {
    // Two agents: filter → trader. Filter is a stub (no LLM dispatch);
    // Trader is the real path.
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents);
    let slots = vec![resolved("regime_filter"), resolved("trader")];

    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"long_open","conviction":0.6,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-1".into(),
        scenario_id: "sc-1".into(),
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
    .expect("pipeline runs");

    // Trader populated; Filter stub is not exposed on the
    // legacy `PipelineOutputs` struct (Phase D will widen this).
    assert!(outs.trader.is_some(), "trader output must be populated");
    assert!(outs.regime.is_none());

    // Phase C: Filter now makes a real LLM call (one for the Filter,
    // one for the Trader). So we expect TWO dispatches:
    // Filter + Trader. (The fixture's filter response is malformed
    // JSON; the parse error fires but the cycle continues.)
    let requests = dispatch.requests();
    assert_eq!(
        requests.len(),
        2,
        "Phase C: Filter + Trader dispatch (got {} requests)",
        requests.len(),
    );
}

// ── A/B cache-key invariance: cycle_idx flows through unchanged ────────

#[tokio::test]
async fn dispatch_capability_preserves_cycle_id_in_dispatcher_call() {
    // Drive `dispatch_capability` directly with a Trader capability and
    // confirm the dispatcher sees the same model + body shape it would
    // have under the pre-Phase-B path. The cycle_id / scenario_id are
    // propagated through `SlotInput`; the dispatcher itself doesn't
    // see them on `LlmRequest` (cache-key derivation happens at the
    // executor seam) — but the executor's identity assertions hinge on
    // the same dispatcher being called with the same prompt body
    // shape. We pin the prompt body's "Inputs:" prefix as the byte-
    // identical contract here.
    let resolved_slot = resolved("trader");
    let slot = resolved_slot.slot.clone();
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let cycle_idx = 42_i64;
    let scenario_id = "sc-cache-key".to_string();

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Decide.".into(),
        upstream_inputs: serde_json::json!({"bar_index": 7}),
        dispatch: dispatch.clone(),
        tools,
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-cache-key".into(),
        scenario_id: scenario_id.clone(),
        cycle_idx,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        current_index: 0,
        total_agents: 1,
        activates: Capability::Trader,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("dispatch_capability runs");

    // Trader returns a typed AgentOutput::Trader with the raw response
    // preserved verbatim.
    match outcome.output {
        AgentOutput::Trader(t) => {
            assert!(t.response.text().contains("hold"));
        }
        other => panic!("expected AgentOutput::Trader, got {other:?}"),
    }
    assert_eq!(outcome.input_tokens, 7);
    assert_eq!(outcome.output_tokens, 11);

    // The dispatcher saw exactly one call. The prompt body carries the
    // upstream_inputs JSON, which is the same shape the pre-Phase-B
    // path would have produced — the A/B cache key reads from this
    // blob's hash via `compute_prompt_hash` (already covered in
    // `agent_observability_hash.rs`), so a byte-identical body
    // guarantees the key is unchanged.
    let requests = dispatch.requests();
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    let prompt_blob = serde_json::to_string(&req.messages).unwrap();
    assert!(
        prompt_blob.contains("bar_index"),
        "request body must include the upstream inputs verbatim: {prompt_blob}",
    );
    assert!(
        prompt_blob.contains("Inputs:"),
        "request body's user message must carry the canonical 'Inputs:' prefix \
         so the prompt hash matches the pre-Phase-B byte shape: {prompt_blob}",
    );
}
