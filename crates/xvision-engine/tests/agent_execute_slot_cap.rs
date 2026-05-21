//! Regression test for the `qa-execute-slot-cap` track
//! (`team/contracts/qa-execute-slot-cap.md`).
//!
//! Bounds the `execute_slot` tool-use loop with a hard iteration cap.
//! Without it, a model that always emits `ToolUse` (no `EndTurn`) would
//! burn through the upstream LLM budget and wedge the run — see QA
//! finding #1, 2026-05-17 codebase review.
//!
//! After this track, the loop terminates with a typed
//! `ExecuteSlotError::ToolLoopCapExceeded` carrying enough payload to
//! diagnose which slot wedged and what it tried to call.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, ExecuteSlotError, SlotInput, MAX_TOOL_LOOP_ITERATIONS};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::{Tool, ToolName, ToolRegistry};

/// Dispatch double that always returns a `ToolUse` block calling
/// `xvision_health_ping`. Counts how many times it was called so a
/// failing test can show how many round-trips actually happened.
struct LoopingDispatch {
    call_count: Mutex<usize>,
    tool_name: String,
}

impl LoopingDispatch {
    fn new(tool_name: &str) -> Self {
        Self {
            call_count: Mutex::new(0),
            tool_name: tool_name.to_string(),
        }
    }

    fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl LlmDispatch for LoopingDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut n = self.call_count.lock().unwrap();
        *n += 1;
        let id = format!("tu-{n}");
        Ok(LlmResponse {
            content: vec![ContentBlock::ToolUse {
                id,
                name: self.tool_name.clone(),
                input: serde_json::json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 7,
            output_tokens: 11,
        })
    }
}

struct CountingHealthTool {
    invoke_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for CountingHealthTool {
    fn name(&self) -> ToolName {
        ToolName::new("xvision_health_ping")
    }

    fn description(&self) -> &'static str {
        "test health ping"
    }

    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.invoke_count.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({"ok": true}))
    }
}

fn trader_slot_with_health_tool() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: vec!["xvision_health_ping".into()],
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

/// Acceptance: a fake model that always emits `ToolUse` triggers the
/// iteration cap rather than looping forever. The typed error carries
/// the slot role, model id, tool names called, accumulated token
/// counts, and last stop reason.
#[tokio::test]
async fn execute_slot_caps_runaway_tool_use_loop() {
    assert_eq!(
        MAX_TOOL_LOOP_ITERATIONS, 12,
        "tool-loop cap is a budget contract, not just an implementation detail",
    );
    let slot = trader_slot_with_health_tool();
    let dispatch = Arc::new(LoopingDispatch::new("xvision_health_ping"));
    let tool_invocations = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::empty();
    registry.register(Arc::new(CountingHealthTool {
        invoke_count: tool_invocations.clone(),
    }));
    let tools = Arc::new(registry);

    let err = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
    })
    .await
    .expect_err("runaway tool-use loop must terminate at the iteration cap");

    let cap_err = err
        .downcast_ref::<ExecuteSlotError>()
        .expect("error must downcast to ExecuteSlotError");

    match cap_err {
        ExecuteSlotError::ToolLoopCapExceeded {
            role,
            model,
            iterations,
            tool_names,
            input_tokens,
            output_tokens,
            last_stop_reason,
        } => {
            assert_eq!(role, "trader", "role must be carried in the payload");
            assert_eq!(
                model, "claude-sonnet-4-6",
                "model id must come from slot.effective_model() — which on this \
                 fixture returns the bare model name (no provider prefix)",
            );
            assert_eq!(
                *iterations, MAX_TOOL_LOOP_ITERATIONS,
                "cap must fire after MAX_TOOL_LOOP_ITERATIONS rounds, not earlier or later",
            );
            assert!(
                tool_names.iter().all(|n| n == "xvision_health_ping"),
                "every recorded tool call should be the looping fixture, got {tool_names:?}",
            );
            assert_eq!(
                tool_names.len(),
                MAX_TOOL_LOOP_ITERATIONS,
                "one tool call recorded per iteration",
            );
            assert!(
                matches!(last_stop_reason, StopReason::ToolUse),
                "last stop_reason should be ToolUse, got {last_stop_reason:?}",
            );
            // Each LoopingDispatch::complete returned input=7, output=11.
            // The cap fires BEFORE the (MAX+1)th call, so accumulated
            // tokens reflect exactly MAX iterations through the
            // dispatch.
            assert_eq!(*input_tokens, 7 * MAX_TOOL_LOOP_ITERATIONS as u32);
            assert_eq!(*output_tokens, 11 * MAX_TOOL_LOOP_ITERATIONS as u32);
        }
    }

    assert_eq!(
        dispatch.call_count(),
        MAX_TOOL_LOOP_ITERATIONS,
        "dispatcher must be invoked exactly MAX_TOOL_LOOP_ITERATIONS times — \
         no extra call after the cap",
    );
    assert_eq!(
        tool_invocations.load(Ordering::SeqCst),
        MAX_TOOL_LOOP_ITERATIONS,
        "registered tool must be invoked once per capped iteration",
    );
}
