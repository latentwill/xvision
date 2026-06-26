//! Phase B — Router capability dispatch + validator backward-target reject.
//!
//! Covers the third acceptance test from
//! `team/contracts/agent-graph-capability-dispatch.md`:
//!
//!   Router emits `RouteSelection` pointing forward; pipeline jumps as
//!   instructed; backward target rejected at validate time.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use xvision_engine::agent::dispatch_capability::{dispatch_capability, AgentOutput, DispatchInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::{AgentRef, EdgePredicate};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::validate::{validate_strategy, ValidationError};
use xvision_engine::strategies::{PipelineDef, PipelineEdge, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

/// Queue-based dispatch double — pops one response per call so we can
/// stage Router-output → Trader-output in order.
struct QueueDispatch {
    queue: Mutex<Vec<LlmResponse>>,
    seen: Mutex<Vec<LlmRequest>>,
}

impl QueueDispatch {
    fn new(responses: Vec<LlmResponse>) -> Self {
        Self {
            queue: Mutex::new(responses),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for QueueDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        let mut q = self.queue.lock().unwrap();
        if q.is_empty() {
            anyhow::bail!("queue dispatch: no canned response left");
        }
        Ok(q.remove(0))
    }
}

fn text_response(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: text.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 3,
        output_tokens: 5,
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
        nano: None,
    }
}

fn fixture_strategy(agents: Vec<AgentRef>, kind: PipelineKind, edges: Vec<PipelineEdge>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZROUTER".into(),
            display_name: "RouterTest".into(),
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
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef { kind, edges },
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

// ── Router emits RouteSelection pointing forward; pipeline jumps ────────

#[tokio::test]
async fn router_jumps_pipeline_forward_to_target_index() {
    // Three-agent Sequential pipeline: Router → (skipped) → Trader.
    // Router selects target_agent_ref_index=2 (Trader), so the middle
    // agent (a Filter stub here) is skipped entirely.
    let agents = vec![
        AgentRef {
            agent_id: "01HZR".into(),
            role: "router".into(),
            activates: Some(Capability::Router),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZC".into(),
            role: "middle_agent".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents, PipelineKind::Sequential, Vec::new());
    let slots = vec![resolved("router"), resolved("middle_agent"), resolved("trader")];

    // Router output → Trader output. Middle agent (Filter) is a stub (no LLM call).
    let dispatch = Arc::new(QueueDispatch::new(vec![
        text_response(r#"{"target_agent_ref_index": 2}"#),
        text_response(r#"{"action":"hold","conviction":0.1,"justification":"r"}"#),
    ]));
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
        run_id: "run-r".into(),
        scenario_id: "sc-r".into(),
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

    // Exactly two LLM calls: one for Router, one for Trader. The
    // middle agent was skipped because the Router jumped to index 2.
    let requests = dispatch.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected Router + Trader dispatches; got {}",
        requests.len(),
    );
    assert!(outs.trader.is_some(), "trader must have run after Router jump");
}

// ── Direct dispatch test: Router emits RouteSelection ──────────────────

#[tokio::test]
async fn dispatch_capability_router_returns_route_selection() {
    let resolved_slot = resolved("router");
    let slot = resolved_slot.slot.clone();
    let dispatch = Arc::new(QueueDispatch::new(vec![text_response(
        r#"{"target_agent_ref_index": 3}"#,
    )]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Route the pipeline.".into(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        current_index: 1,
        total_agents: 5,
        activates: Capability::Router,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("router dispatch succeeds");

    match outcome.output {
        AgentOutput::Router(sel) => {
            assert_eq!(sel.target_agent_ref_index, 3);
        }
        other => panic!("expected Router output, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_capability_router_rejects_backward_target() {
    let resolved_slot = resolved("router");
    let slot = resolved_slot.slot.clone();
    let dispatch = Arc::new(QueueDispatch::new(vec![text_response(
        r#"{"target_agent_ref_index": 0}"#,
    )]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let err = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Route the pipeline.".into(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        current_index: 2, // Router at index 2; target=0 is backward.
        total_agents: 5,
        activates: Capability::Router,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("not strictly greater"),
        "expected backward-target rejection, got: {err}",
    );
}

// ── Validator: backward edge rejected at draft time ───────────────────

#[test]
fn validate_strategy_rejects_backward_edge_in_graph_pipeline() {
    // Graph: filter @ idx 0 → trader @ idx 1 → filter @ idx 0 (backward).
    // The predicate satisfies the upstream-Filter check; the backward
    // direction is the violation.
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let edges = vec![PipelineEdge {
        from_role: "trader".into(),
        to_role: "filter".into(),
        condition: Some(EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: serde_json::json!("trend"),
        }),
    }];
    let strategy = fixture_strategy(agents, PipelineKind::Graph, edges);
    let err = validate_strategy(&strategy).expect_err("backward edge must fail validation");
    match err {
        ValidationError::BackwardEdge { from, to } => {
            assert_eq!(from, "trader");
            assert_eq!(to, "filter");
        }
        other => panic!("expected BackwardEdge, got {other:?}"),
    }
}

#[test]
fn validate_strategy_rejects_predicate_without_upstream_filter() {
    // Graph edge between two Trader-capable agents carries a
    // predicate; no Filter precedes it, so the predicate could never
    // fire.
    let agents = vec![
        AgentRef {
            agent_id: "01HZT1".into(),
            role: "trader_a".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT2".into(),
            role: "trader_b".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let edges = vec![PipelineEdge {
        from_role: "trader_a".into(),
        to_role: "trader_b".into(),
        condition: Some(EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: serde_json::json!("trend"),
        }),
    }];
    let strategy = fixture_strategy(agents, PipelineKind::Graph, edges);
    let err = validate_strategy(&strategy).expect_err("predicate without Filter must fail");
    match err {
        ValidationError::PredicateWithoutUpstreamFilter { from, to } => {
            assert_eq!(from, "trader_a");
            assert_eq!(to, "trader_b");
        }
        other => panic!("expected PredicateWithoutUpstreamFilter, got {other:?}"),
    }
}

#[test]
fn validate_strategy_accepts_forward_edge_with_upstream_filter() {
    // Filter @ 0 → Trader @ 1 with an edge from Filter to Trader
    // carrying a predicate. Forward + upstream Filter exists.
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let edges = vec![PipelineEdge {
        from_role: "filter".into(),
        to_role: "trader".into(),
        condition: Some(EdgePredicate::Eq {
            signal_field: "regime".into(),
            value: serde_json::json!("trend"),
        }),
    }];
    let strategy = fixture_strategy(agents, PipelineKind::Graph, edges);
    validate_strategy(&strategy).expect("valid forward graph with upstream filter");
}

#[test]
fn validate_strategy_accepts_unconditional_edge() {
    // Edge with `condition: None` is always valid — no upstream
    // filter required.
    let agents = vec![
        AgentRef {
            agent_id: "01HZT1".into(),
            role: "trader_a".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT2".into(),
            role: "trader_b".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let edges = vec![PipelineEdge {
        from_role: "trader_a".into(),
        to_role: "trader_b".into(),
        condition: None,
    }];
    let strategy = fixture_strategy(agents, PipelineKind::Graph, edges);
    validate_strategy(&strategy).expect("unconditional forward edge is always valid");
}
