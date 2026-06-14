//! Acceptance tests for the `trader-noop-skip` track
//! (`team/intake/2026-05-21-eval-honesty-and-agent-graph.md`).
//!
//! The pre-LLM gate skips the LLM call when the seed explicitly provides a
//! `legal_actions` set containing only `hold` AND the trader slot has
//! `noop_skip` enabled (the default).
//! A synthesized `TraderDecision` is returned with `noop_skip` provenance so
//! the trace surface shows the skip.
//!
//! Four acceptance scenarios:
//!
//! 1. Legal action set is hold-only, slot default (`noop_skip = None`
//!    → true) → LLM mock never called, decision carries noop_skip provenance.
//! 2. Portfolio long without a hold-only allowed-action set → LLM IS called.
//! 3. Portfolio long, `noop_skip = Some(false)` → LLM IS called.
//! 4. Portfolio flat (`position_size == 0`) → LLM IS called regardless of
//!    noop_skip.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::InputsPolicy;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{ActivationMode, PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HNOOP_SKIP".into(),
            display_name: "NoopSkip Test".into(),
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
        agents: Vec::new(),
        pipeline: PipelineDef::sequential(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

/// A dispatch that counts how many times it was called and always returns
/// a canned hold response. Tests assert on `call_count`.
struct CountingDispatch {
    call_count: Arc<AtomicU32>,
    response_text: String,
}

impl CountingDispatch {
    fn new(response_text: &str) -> (Arc<AtomicU32>, Arc<Self>) {
        let count = Arc::new(AtomicU32::new(0));
        let d = Arc::new(Self {
            call_count: Arc::clone(&count),
            response_text: response_text.into(),
        });
        (count, d)
    }
}

#[async_trait::async_trait]
impl LlmDispatch for CountingDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: self.response_text.clone(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 5,
            output_tokens: 5,
        })
    }
}

fn trader_slot(noop_skip: bool) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: String::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip,
    }
}

fn seed_with_position(position_size: f64) -> serde_json::Value {
    serde_json::json!({
        "asset": "BTC/USD",
        "portfolio_state": {
            "position_size": position_size,
            "equity": 100_000.0,
            "buying_power": 50_000.0,
            "mark_price": 50_000.0,
        },
        "market_data": {}
    })
}

fn seed_with_hold_only_actions(position_size: f64) -> serde_json::Value {
    let mut seed = seed_with_position(position_size);
    seed["legal_actions"] = serde_json::json!(["hold"]);
    seed
}

// ---------------------------------------------------------------------------
// Test 1: hold-only allowed-action set, noop_skip default → LLM NOT called
// ---------------------------------------------------------------------------

/// Acceptance criterion 1: when the seed explicitly says only `hold` is
/// available and the slot's `noop_skip` is enabled, the LLM provider mock must
/// NEVER be called and the resulting `PipelineOutputs.trader` must carry
/// `noop_skip` provenance.
#[tokio::test]
async fn noop_skip_fires_when_seed_allows_only_hold() {
    let strategy = fixture_strategy();
    let agent_slots = vec![trader_slot(/* noop_skip = */ true)];

    let (call_count, dispatch) = CountingDispatch::new(
        r#"{"action":"long_open","conviction":0.9,"justification":"would have gone long"}"#,
    );
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: seed_with_hold_only_actions(100.0),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
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

    // LLM must never have been called.
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        0,
        "noop_skip must prevent the LLM from being called when only hold is available"
    );

    // Trader output must be present (not silently dropped).
    let trader_text = outs
        .trader
        .expect("trader output must be present even when noop_skip fires")
        .text();

    assert!(
        trader_text.contains("noop_skip"),
        "trader decision must carry noop_skip provenance; got: {trader_text:?}"
    );
    assert!(
        trader_text.contains("hold"),
        "synthesized decision must be action=hold; got: {trader_text:?}"
    );

    // Token counts must be zero — no provider was called.
    assert_eq!(
        outs.total_input_tokens, 0,
        "noop_skip must produce zero input tokens"
    );
    assert_eq!(
        outs.total_output_tokens, 0,
        "noop_skip must produce zero output tokens"
    );
}

// ---------------------------------------------------------------------------
// Test 2: portfolio long, no hold-only allowed-action set → LLM IS called
// ---------------------------------------------------------------------------

/// Acceptance criterion 2: an open position alone is not enough to skip the
/// trader. Close/flat is still a state-changing decision the model may need to
/// make.
#[tokio::test]
async fn noop_skip_does_not_fire_just_because_portfolio_is_long() {
    let strategy = fixture_strategy();
    let agent_slots = vec![trader_slot(/* noop_skip = */ true)];

    let (call_count, dispatch) =
        CountingDispatch::new(r#"{"action":"hold","conviction":0.1,"justification":"hold in corner"}"#);
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let _outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: seed_with_position(100.0),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
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

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "LLM must be called when position is held but no hold-only allowed-action set is present"
    );
}

// ---------------------------------------------------------------------------
// Test 3: portfolio long, noop_skip = Some(false) → LLM IS called
// ---------------------------------------------------------------------------

/// Acceptance criterion 3: when `noop_skip` is explicitly disabled
/// (`Some(false)`), the LLM IS called even if the seed says only hold is
/// available so operators who want "what would the model say in a corner?" can
/// opt out.
#[tokio::test]
async fn noop_skip_disabled_calls_llm_even_when_only_hold_is_available() {
    let strategy = fixture_strategy();
    let agent_slots = vec![trader_slot(/* noop_skip = */ false)];

    let (call_count, dispatch) =
        CountingDispatch::new(r#"{"action":"hold","conviction":0.1,"justification":"hold in corner"}"#);
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let _outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: seed_with_hold_only_actions(100.0),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
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

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "LLM must be called when noop_skip = false, even when only hold is available"
    );
}

// ---------------------------------------------------------------------------
// Test 4: portfolio flat → LLM IS called regardless of noop_skip
// ---------------------------------------------------------------------------

/// Acceptance criterion 4: when the portfolio is flat (`position_size == 0`)
/// both long_open and short_open are legal, so the gate must NOT fire even
/// when `noop_skip` is enabled.
#[tokio::test]
async fn noop_skip_does_not_fire_when_portfolio_flat() {
    let strategy = fixture_strategy();
    let agent_slots = vec![trader_slot(/* noop_skip = */ true)];

    let (call_count, dispatch) =
        CountingDispatch::new(r#"{"action":"long_open","conviction":0.8,"justification":"bullish entry"}"#);
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let _outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        // position_size = 0 → flat portfolio
        seed_inputs: seed_with_position(0.0),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
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

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "LLM must be called when portfolio is flat (both opens are legal)"
    );
}
