//! Integration test for F-4 (`harness-span-taxonomy-extension`).
//!
//! Pins the wire-level behaviour of the four new `SpanKind` variants
//! added by this track:
//!
//! - `tool.validate_input` / `tool.validate_output` — emitted as
//!   instantaneous brackets around every `tool_call::invoke(...)` in
//!   `execute_slot`. The bodies are no-ops; F-6 will fill them. F-4
//!   pins the ordering + the `tool_name` attribute so F-6 can rely on
//!   the shape.
//! - `state.transition` — emitted twice per run lifecycle from
//!   `ObsEmitter::emit_run_started` (`null → running`) and
//!   `emit_run_finished` (`running → <terminal>`). Carries the typed
//!   `SpanAttributes` bag + `{"from", "to"}` so the trace dock can
//!   render a per-run state timeline.
//! - `recovery.attempt` — F-4 reserves the wire identifier. NOT
//!   emitted by F-4; F-5 (`harness-recovery-state-machine`) owns
//!   emission. The round-trip lock at
//!   `xvision-observability/tests/span_kind_roundtrip.rs` guards the
//!   wire format; this file asserts that no F-4-owned code path
//!   emits it (regression guard against accidental emission slipping
//!   in via a future PR rebase).

use std::sync::Arc;

use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{
    AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, RunStatus, SpanKind,
};

fn trader_slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: vec!["price_of_thing".to_string()],
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

/// Dispatch that emits one `tool_use` turn then an `end_turn` text
/// turn. Drives `execute_slot` through exactly one tool-call iteration
/// so the validate brackets fire exactly once.
struct OneToolThenEndTurn;

#[async_trait]
impl LlmDispatch for OneToolThenEndTurn {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        // First call (no prior tool_result yet) → emit a ToolUse.
        // Second call (after we've appended the tool_result) →
        // emit a text-only EndTurn.
        let saw_tool_result = req.messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        if saw_tool_result {
            Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: r#"{"action":"hold"}"#.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 0,
                output_tokens: 0,
            })
        } else {
            Ok(LlmResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "tu-1".into(),
                    name: "price_of_thing".into(),
                    input: serde_json::json!({"symbol": "BTC"}),
                }],
                stop_reason: StopReason::ToolUse,
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }
}

#[tokio::test]
async fn validate_brackets_wrap_each_tool_call_in_order() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-validate-bracket-test");

    let slot = trader_slot();
    let input = SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch: Arc::new(OneToolThenEndTurn),
        tools: Arc::new(ToolRegistry::empty()),
        response_schema: None,
        max_tokens: None,
        obs: Some(emitter),
    };

    // Drive one tool-use iteration. The tool itself fails (registry
    // is empty), which is fine — the validate spans MUST emit even
    // on tool error per the F-4 contract.
    let _ = execute_slot(input).await;

    // Drain the bus into the recorder. Same pattern used by
    // `tests/eval_observability.rs` — the consumer task is async so
    // a single yield isn't enough for multi-event tests.
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let events = recorder.snapshot().await;

    // Filter to only SpanStarted kinds we care about, preserving
    // emission order on the bus.
    let kinds: Vec<SpanKind> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) => Some(s.kind),
            _ => None,
        })
        .collect();

    // F-4 contract acceptance: validate_input → ... → validate_output,
    // with the model.call span landing between the run-level
    // brackets. The tool.call span itself is not emitted by the
    // engine eval path today (separate tracked gap); the validate
    // spans bracket the `tool_call::invoke` invocation directly.
    let validate_input_idx = kinds
        .iter()
        .position(|k| matches!(k, SpanKind::ToolValidateInput))
        .expect("ToolValidateInput span was emitted");
    let validate_output_idx = kinds
        .iter()
        .position(|k| matches!(k, SpanKind::ToolValidateOutput))
        .expect("ToolValidateOutput span was emitted");
    assert!(
        validate_input_idx < validate_output_idx,
        "validate_input must precede validate_output: kinds={kinds:?}"
    );

    // `tool_name` attribute lands on both validate spans so F-6 has
    // the data it needs without re-parsing the span name field.
    let validate_spans: Vec<&xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s)
                if matches!(
                    s.kind,
                    SpanKind::ToolValidateInput | SpanKind::ToolValidateOutput
                ) =>
            {
                Some(s)
            }
            _ => None,
        })
        .collect();
    assert_eq!(validate_spans.len(), 2, "exactly two validate spans");
    for span in &validate_spans {
        assert_eq!(span.name, "price_of_thing", "span name carries tool_name");
        let attrs = span
            .attributes_json
            .as_ref()
            .expect("validate span attributes_json populated");
        let parsed: serde_json::Value = serde_json::from_str(attrs).expect("attrs parse");
        assert_eq!(
            parsed.get("tool_name").and_then(|v| v.as_str()),
            Some("price_of_thing"),
            "F-2 SpanAttributes.tool_name populated on validate spans"
        );
        assert_eq!(
            parsed.get("run_id").and_then(|v| v.as_str()),
            Some("run-validate-bracket-test"),
            "F-2 SpanAttributes.run_id populated on validate spans"
        );
    }

    // F-4 reserves `recovery.attempt` but does NOT emit it. Guards
    // against accidental emission slipping in via a future rebase.
    assert!(
        !kinds
            .iter()
            .any(|k| matches!(k, SpanKind::RecoveryAttempt)),
        "F-4 must not emit recovery.attempt — F-5 owns that seam. kinds={kinds:?}"
    );
}

#[tokio::test]
async fn run_lifecycle_emits_two_state_transition_spans() {
    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-state-transition-test");

    emitter.emit_run_started("test objective", "full_debug").await;
    emitter.emit_run_finished(RunStatus::Completed, None).await;

    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let events = recorder.snapshot().await;

    // Find every state.transition span in order.
    let transitions: Vec<&xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::StateTransition) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(
        transitions.len(),
        2,
        "expected one transition per lifecycle endpoint, got {}",
        transitions.len()
    );

    let parse_attrs = |s: &xvision_observability::SpanStartedEvent| -> serde_json::Value {
        let raw = s
            .attributes_json
            .as_ref()
            .expect("state.transition attributes_json populated");
        serde_json::from_str(raw).expect("attrs parse")
    };

    // First transition: null → running, emitted by emit_run_started.
    let start_attrs = parse_attrs(transitions[0]);
    assert!(
        start_attrs.get("from").map(|v| v.is_null()).unwrap_or(false),
        "run-start transition: `from` should be JSON null. attrs={start_attrs}"
    );
    assert_eq!(
        start_attrs.get("to").and_then(|v| v.as_str()),
        Some("running"),
        "run-start transition: `to` should be \"running\""
    );

    // Second transition: running → completed, emitted by emit_run_finished.
    let end_attrs = parse_attrs(transitions[1]);
    assert_eq!(
        end_attrs.get("from").and_then(|v| v.as_str()),
        Some("running"),
        "run-finish transition: `from` should be \"running\""
    );
    assert_eq!(
        end_attrs.get("to").and_then(|v| v.as_str()),
        Some("completed"),
        "run-finish transition: `to` should be \"completed\""
    );

    // `run_id` always populated via the F-2 SpanAttributes bag.
    assert_eq!(
        start_attrs.get("run_id").and_then(|v| v.as_str()),
        Some("run-state-transition-test"),
        "run-start: SpanAttributes.run_id populated"
    );
    assert_eq!(
        end_attrs.get("run_id").and_then(|v| v.as_str()),
        Some("run-state-transition-test"),
        "run-finish: SpanAttributes.run_id populated"
    );

    // Each transition closes with status=ok in the same tick (the
    // open/close pair makes the span instantaneous in duration).
    let finished_count = events
        .iter()
        .filter(|e| matches!(e, RunEvent::SpanFinished(_)))
        .count();
    assert!(
        finished_count >= 2,
        "expected at least 2 SpanFinished events (one per transition), got {finished_count}"
    );
}
