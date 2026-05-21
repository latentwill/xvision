//! Integration test for F-5 (`harness-recovery-state-machine`).
//!
//! Drives `execute_slot` through the repeated-tool-failure block list
//! and asserts:
//!
//! - First failure of a `(tool_name, input)` pair is silent (no
//!   `recovery.attempt` span). The agent receives `is_error: true` and
//!   the existing self-healing path handles it.
//! - Second-and-later failures of the same pair emit
//!   `recovery.attempt` spans with monotonically rising `retry_count`.
//! - The N-th failure (where N == `MAX_TOOL_RETRIES_PER_PAIR`) emits
//!   `recovery.attempt` with `SpanStatus::Error` and trips the block.
//!   Subsequent attempts of the same pair never call into the tool. The
//!   model receives a structured `repeated_tool_failure` tool_result so
//!   it can re-decide with a different input.
//!
//! Recovery-class typed dispatcher coverage lives in unit tests at
//! `crates/xvision-engine/src/agent/recovery.rs` (13 tests, every
//! `FailureClass` variant). This file exercises the *seam* — the
//! tracker wired into `execute_slot` — end-to-end through the
//! observability bus.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::recovery::MAX_TOOL_RETRIES_PER_PAIR;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::{Tool, ToolName, ToolRegistry};
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStatus};

fn slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "test.model".into(),
        allowed_tools: vec!["always_fails".to_string()],
        provider: Some("test".into()),
        model: Some("test".into()),
    }
}

/// Tool whose `invoke` always returns an error. The error message is
/// stable so the agent's self-healing re-decide loop produces an
/// identical second/third request, which is exactly what the F-5
/// block-list keys on.
struct AlwaysFailsTool;

#[async_trait]
impl Tool for AlwaysFailsTool {
    fn name(&self) -> ToolName {
        ToolName::new("always_fails")
    }
    fn description(&self) -> &'static str {
        "test-only tool that always errors with a stable message"
    }
    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("test-deterministic failure")
    }
}

struct CountingAlwaysFailsTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for CountingAlwaysFailsTool {
    fn name(&self) -> ToolName {
        ToolName::new("always_fails")
    }
    fn description(&self) -> &'static str {
        "test-only tool that always errors and counts invocations"
    }
    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("test-deterministic failure")
    }
}

/// Dispatch that emits the SAME tool_use call N times then ends with
/// a text turn. The fixed input shape is the load-bearing detail — it's
/// what makes the F-5 input-hash dedup actually trip.
struct RepeatedToolCallDispatch {
    iterations: Mutex<usize>,
    max_iterations: usize,
}

impl RepeatedToolCallDispatch {
    fn new(max_iterations: usize) -> Self {
        Self {
            iterations: Mutex::new(0),
            max_iterations,
        }
    }
}

#[async_trait]
impl LlmDispatch for RepeatedToolCallDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut count = self.iterations.lock().unwrap();
        if *count >= self.max_iterations {
            return Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: r#"{"action":"hold","conviction":0.0,"justification":"stop"}"#.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 0,
                output_tokens: 0,
            });
        }
        *count += 1;
        Ok(LlmResponse {
            content: vec![ContentBlock::ToolUse {
                id: format!("tu-{count}"),
                name: "always_fails".into(),
                input: serde_json::json!({"symbol": "BTC/USD"}),
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 0,
            output_tokens: 0,
        })
    }
}

async fn drain(bus: &RunEventBus) {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

fn build_input<'a>(
    slot: &'a LLMSlot,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    emitter: ObsEmitter,
) -> SlotInput<'a> {
    SlotInput {
        slot,
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter),
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
    }
}

#[tokio::test]
async fn first_tool_failure_emits_no_recovery_span() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-f5-first-fail");

    let mut tools = ToolRegistry::empty();
    tools.register(Arc::new(AlwaysFailsTool));
    let s = slot();
    let input = build_input(
        &s,
        Arc::new(RepeatedToolCallDispatch::new(1)),
        Arc::new(tools),
        emitter,
    );

    let _ = execute_slot(input).await;
    drain(&bus).await;

    let events = recorder.snapshot().await;
    let recovery_spans: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RecoveryAttempt) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        recovery_spans.is_empty(),
        "first failure of a pair must not emit recovery.attempt — that's normal self-healing. \
         spans={recovery_spans:?}"
    );
}

#[tokio::test]
async fn second_failure_of_same_pair_emits_recovery_attempt_with_retry_count_one() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-f5-second-fail");

    let mut tools = ToolRegistry::empty();
    tools.register(Arc::new(AlwaysFailsTool));
    let s = slot();
    let input = build_input(
        &s,
        Arc::new(RepeatedToolCallDispatch::new(2)),
        Arc::new(tools),
        emitter,
    );

    let _ = execute_slot(input).await;
    drain(&bus).await;

    let events = recorder.snapshot().await;
    let recovery_starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RecoveryAttempt) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        recovery_starts.len(),
        1,
        "exactly one recovery.attempt on the 2nd failure (retry_count=1). \
         got {} recovery spans",
        recovery_starts.len()
    );
    let span = recovery_starts[0];
    let attrs: serde_json::Value = serde_json::from_str(
        span.attributes_json
            .as_ref()
            .expect("recovery span attributes_json"),
    )
    .expect("attrs parse");
    assert_eq!(
        attrs.get("class_tag").and_then(|v| v.as_str()),
        Some("repeated_tool_failure"),
        "class_tag on the recovery span"
    );
    assert_eq!(
        attrs.get("retry_count").and_then(|v| v.as_i64()),
        Some(1),
        "retry_count = 1 on the 2nd failure"
    );
}

#[tokio::test]
async fn third_failure_trips_block_and_emits_recovery_failed() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-f5-third-fail");

    let calls = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::empty();
    tools.register(Arc::new(CountingAlwaysFailsTool { calls: calls.clone() }));
    let s = slot();
    // MAX_TOOL_RETRIES_PER_PAIR=3: the 3rd actual failure trips the
    // block and emits `recovery.failed`. Drive one extra model
    // iteration to prove subsequent identical attempts short-circuit
    // and do not invoke the tool again.
    let total_iterations = (MAX_TOOL_RETRIES_PER_PAIR + 1) as usize;
    let input = build_input(
        &s,
        Arc::new(RepeatedToolCallDispatch::new(total_iterations)),
        Arc::new(tools),
        emitter,
    );

    let _ = execute_slot(input).await;
    drain(&bus).await;

    let events = recorder.snapshot().await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        MAX_TOOL_RETRIES_PER_PAIR as usize,
        "the attempt after the block trip must not invoke the failing tool"
    );

    // Pair every SpanStarted(RecoveryAttempt) with its SpanFinished
    // (same span_id) so we can read the SpanStatus.
    let mut started: std::collections::HashMap<String, &xvision_observability::SpanStartedEvent> =
        std::collections::HashMap::new();
    let mut finished: std::collections::HashMap<String, &xvision_observability::SpanFinishedEvent> =
        std::collections::HashMap::new();
    for e in &events {
        match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::RecoveryAttempt) => {
                started.insert(s.span_id.clone(), s);
            }
            RunEvent::SpanFinished(s) => {
                finished.insert(s.span_id.clone(), s);
            }
            _ => {}
        }
    }
    assert_eq!(
        started.len(),
        2,
        "exactly two recovery.attempt spans across failures 2..=3 \
         (one retry rise + one block). started count = {}, span_ids = {:?}",
        started.len(),
        started.keys().collect::<Vec<_>>(),
    );

    // The block-trip span carries SpanStatus::Error (emit_recovery_failed),
    // every preceding rise carries SpanStatus::Ok (emit_recovery_attempt).
    let mut error_count = 0;
    let mut ok_count = 0;
    for span_id in started.keys() {
        let fin = finished.get(span_id).expect("paired SpanFinished");
        match fin.status {
            SpanStatus::Error => error_count += 1,
            SpanStatus::Ok => ok_count += 1,
            other => panic!("unexpected recovery span status: {other:?}"),
        }
    }
    assert_eq!(
        error_count, 1,
        "exactly one recovery span ends in Error (the block trip)"
    );
    assert_eq!(
        ok_count, 1,
        "only the second failure ends in Ok (the rising retry before the block)"
    );

    // The blocked invocation never calls into the tool — the model's
    // tool_result content is the structured repeated_tool_failure
    // message. We assert the message shape via the recovery.failed
    // span's error_json.
    let blocked_fin = finished
        .values()
        .find(|f| matches!(f.status, SpanStatus::Error))
        .expect("blocked recovery span");
    let err_json: serde_json::Value =
        serde_json::from_str(blocked_fin.error_json.as_ref().expect("error_json")).expect("error_json parse");
    assert_eq!(
        err_json.get("class_tag").and_then(|v| v.as_str()),
        Some("repeated_tool_failure"),
        "recovery.failed error_json carries class_tag"
    );
    let msg = err_json
        .get("message")
        .and_then(|v| v.as_str())
        .expect("error_json.message");
    assert!(
        msg.contains("always_fails"),
        "error_json.message names the blocked tool: {msg}"
    );
    assert!(msg.contains("blocked"), "error_json.message says blocked: {msg}");
}

#[tokio::test]
async fn classify_run_failure_adapter_preserves_wire_tags() {
    // The thin adapter in `eval::executor::mod.rs::classify_run_failure`
    // now delegates to `recovery::classify(err).tag()`. Pin the wire
    // shape so a downstream eval review consumer never sees a tag
    // drift just because the implementation moved modules.
    use xvision_engine::eval::executor::classify_run_failure;

    let cases: &[(&str, &'static str)] = &[
        // Trader output via string fallback
        ("trader_output[invalid_json]: not json", "invalid_json"),
        ("trader_output[missing_field]: x", "missing_field"),
        // Broker classes
        (
            "alpaca create_order: bracket orders not supported for this asset class",
            "broker_unsupported",
        ),
        (
            "alpaca create_order: insufficient buying power",
            "broker_insufficient_funds",
        ),
        ("alpaca order 01H... rejected", "broker_rejected"),
        (
            "alpaca order 01H... did not fill within 5 polls",
            "broker_timeout",
        ),
        // Provider classes
        ("openrouter request timed out after 60s", "provider_timeout"),
        ("tcp connect: connection refused", "provider_connect"),
        (
            "anthropic api error: 500 internal server error",
            "provider_http_error",
        ),
        // Circuit breaker
        (
            "[repeated_broker_error] N=3 consecutive broker_min_order_size rejections",
            "repeated_broker_error",
        ),
        // Unknown fallback
        ("some completely unrecognized error", "unclassified"),
    ];
    for (msg, expected_tag) in cases {
        let err = anyhow::anyhow!((*msg).to_string());
        assert_eq!(
            classify_run_failure(&err),
            *expected_tag,
            "wire tag drift for: {msg}"
        );
    }
}
