//! Parity guard: `dispatch_capability` for `Capability::Trader` must set
//! `raw_response` to the *same* `LlmResponse` that ends up inside
//! `AgentOutput::Trader(t).response` — the `TraderDecision` wrapper must
//! never mutate or divergently clone the underlying text.
//!
//! Pinned invariant (from the line-103/319 comment in `dispatch_capability.rs`):
//!
//!   `outcome.raw_response.unwrap().text() == outcome.output (as Trader).response.text()`
//!
//! Both must equal the canned mock JSON supplied by `RecordingDispatch`.
//! Token counts `(7, 11)` from the mock must also flow through unchanged.
//!
//! See spec: `team/contracts/agent-graph-capability-dispatch.md` and
//! the parity-dispatch-capability-byte-identical track.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use xvision_engine::agent::dispatch_capability::{dispatch_capability, AgentOutput, DispatchInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::ResolvedAgentSlot;
use xvision_engine::agents::Capability;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// RecordingDispatch — identical pattern to `agent_graph_dispatch.rs`
// ---------------------------------------------------------------------------

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

fn resolved_trader() -> ResolvedAgentSlot {
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
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: false,
        nano: None,
    }
}

// ---------------------------------------------------------------------------
// The parity assertion
// ---------------------------------------------------------------------------

const CANNED_JSON: &str = r#"{"action":"hold","conviction":0.3,"justification":"parity"}"#;

#[tokio::test]
async fn raw_response_text_is_byte_identical_to_trader_decision_response_text() {
    let resolved_slot = resolved_trader();
    let slot = resolved_slot.slot.clone();
    let dispatch = Arc::new(RecordingDispatch::new(CANNED_JSON));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Decide.".into(),
        upstream_inputs: serde_json::json!({}),
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
        run_id: "run-parity".into(),
        scenario_id: "sc-parity".into(),
        cycle_idx: 0,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
        current_index: 0,
        total_agents: 1,
        activates: Capability::Trader,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("dispatch_capability must succeed");

    // 1. raw_response must be Some for Trader — never None.
    assert!(
        outcome.raw_response.is_some(),
        "outcome.raw_response must be Some for Capability::Trader",
    );

    // 2. Extract raw_response text.
    let raw_text = outcome.raw_response.unwrap().text();

    // 3. Extract the text from inside AgentOutput::Trader.
    let trader_text = match outcome.output {
        AgentOutput::Trader(t) => t.response.text(),
        other => panic!("expected AgentOutput::Trader, got {other:?}"),
    };

    // 4. Core invariant: byte-identical. TraderDecision must not mutate
    //    or divergently clone the LlmResponse that dispatch_capability returns
    //    in raw_response.
    assert_eq!(
        raw_text, trader_text,
        "raw_response.text() must be byte-identical to AgentOutput::Trader.response.text()",
    );

    // 5. Both must equal the canned mock JSON (no silent transformation).
    assert_eq!(
        raw_text, CANNED_JSON,
        "raw_response.text() must equal the canned mock JSON verbatim",
    );

    // 6. Token counts flow through unchanged from the mock LlmResponse.
    assert_eq!(outcome.input_tokens, 7, "input_tokens must match mock (7)");
    assert_eq!(outcome.output_tokens, 11, "output_tokens must match mock (11)");
}
