//! `execute_slot` — drives one LLM slot through a tool-use loop.
//!
//! The slot's `allowed_tools` list is converted into `ToolDefinition`s and
//! advertised to the model each turn. When the model emits
//! `ContentBlock::ToolUse` blocks we route them through the slot's
//! `ToolRegistry`, append `ToolResult` blocks to the conversation, and
//! re-call until the model emits a text-only `EndTurn` or the dispatch/tool
//! layer returns an error.

use std::sync::Arc;

use crate::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, ResponseSchema, StopReason,
};
use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::agent::tool_call;
use crate::strategies::slot::LLMSlot;
use crate::tools::ToolRegistry;

/// Hard cap on the number of tool-use round-trips inside `execute_slot`.
/// A pathological model that always emits `ToolUse` (no `EndTurn`) would
/// otherwise loop until the upstream LLM budget or wall clock ran out —
/// see `qa/2026-05-17-comprehensive-codebase-review.md` finding #1. The
/// dashboard wizard has a sibling constant for its own iteration count;
/// the two are intentionally independent.
///
/// Picked at the top of the 8–12 range from the contract — generous
/// enough for legitimate multi-tool plans, tight enough to catch a loop
/// long before it burns through realistic per-decision budgets.
pub const MAX_TOOL_LOOP_ITERATIONS: usize = 12;

/// Typed errors that `execute_slot` can produce. Wrapped in
/// `anyhow::Error` at the call boundary so existing `Result<_,
/// anyhow::Error>` callers (the engine pipeline, eval executors) keep
/// compiling unchanged — but downstream observability code (e.g. the
/// post-Phase-B `qa-trace-error-surfacing` track) can `downcast_ref` to
/// match on the specific variant and pull the structured payload.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteSlotError {
    /// The tool-use loop ran for `MAX_TOOL_LOOP_ITERATIONS` rounds
    /// without the model emitting `EndTurn` or running out of tool
    /// calls. Carries enough payload for an operator to diagnose which
    /// slot wedged and what it was doing.
    #[error(
        "execute_slot: tool-use loop exhausted after {iterations} iterations \
         (slot role={role}, model={model}, last stop_reason={last_stop_reason:?}, \
         tools_called={tool_names:?}, input_tokens={input_tokens}, \
         output_tokens={output_tokens})"
    )]
    ToolLoopCapExceeded {
        role: String,
        model: String,
        iterations: usize,
        tool_names: Vec<String>,
        input_tokens: u32,
        output_tokens: u32,
        last_stop_reason: StopReason,
    },
}

pub struct SlotInput<'a> {
    pub slot: &'a LLMSlot,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
    pub response_schema: Option<ResponseSchema>,
    /// **Deprecated.** Vestigial per-request budget that used to thread
    /// the operator's `AgentSlot.max_tokens` override through to the
    /// dispatcher. Retained on the struct so existing callers (the eval
    /// pipeline, in-tree integration tests) keep compiling — but
    /// `execute_slot` ignores this field and always hands the dispatcher
    /// `None`, which makes the llm-layer resolve the cap from the model
    /// library (`lookup_model(model).auto_max_tokens()` for Anthropic;
    /// OpenAI-compat omits the field and lets the provider apply its
    /// own default). See the 2026-05-17 `qa-remove-agent-max-tokens`
    /// track for the rationale — leaving operators a per-slot override
    /// was a footgun (4096 set on a 384k-output model silently capped
    /// production runs).
    pub max_tokens: Option<u32>,
    /// Observability emitter (`qa-eval-observability-wiring`, 2026-05-17).
    /// When `Some`, every LLM dispatch inside this slot emits a
    /// `ModelCall` span + `ModelCallFinished` (success) or
    /// `SpanFinished{Error}` (failure) on the observability bus, so
    /// eval runs surface in `/api/agent-runs/<run_id>` and the trace
    /// dock renders failures (PR #238). `None` is the default —
    /// existing call sites (legacy pipeline, unit tests) opt out
    /// trivially and the emit code becomes a no-op.
    pub obs: Option<ObsEmitter>,
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    // agent-error-feedback-self-healing: pull the broker feedback
    // out of `upstream_inputs` BEFORE serialising the JSON dump so
    // the diagnostic lands in the proper ToolResult carrier instead
    // of being duplicated as inline JSON in the user prompt.
    let mut inputs_for_prompt = input.upstream_inputs.clone();
    let agent_error_feedback = inputs_for_prompt
        .as_object_mut()
        .and_then(|o| o.remove("agent_error_feedback"))
        .filter(|v| !v.is_null());

    let initial_user = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data; emit your final decision as JSON.",
        serde_json::to_string_pretty(&inputs_for_prompt)?
    );

    let tool_defs = tool_call::definitions_for_slot(&input.slot.allowed_tools, &input.tools);

    let mut messages: Vec<Message> = Vec::with_capacity(3);

    // When the executor stashed a recoverable broker error from the
    // prior decision cycle, surface it as a proper ToolResult with
    // `is_error: true` BEFORE the live user turn. The model sees a
    // synthetic prior tool_use + the failure result, then the
    // current cycle's inputs — matching the contract's "tool-result
    // with is_error: true" wording. The synthetic tool_use_id is
    // deterministic so a future re-run can correlate.
    if let Some(feedback) = agent_error_feedback {
        let decision_idx = feedback
            .get("decision_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let tool_use_id = format!("broker_call_prior_cycle_{decision_idx}");
        let tool_input = feedback
            .as_object()
            .map(|obj| {
                serde_json::json!({
                    "asset": obj.get("asset"),
                    "intended_action": "broker submit",
                })
            })
            .unwrap_or(serde_json::Value::Null);
        messages.push(Message {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolUse {
                id: tool_use_id.clone(),
                name: "broker.submit_order".into(),
                input: tool_input,
            }],
        });
        messages.push(Message {
            role: "user".into(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id,
                content: serde_json::to_string(&feedback).unwrap_or_else(|_| "{}".to_string()),
                is_error: Some(true),
            }],
        });
    }

    messages.push(Message {
        role: "user".into(),
        content: vec![ContentBlock::Text { text: initial_user }],
    });

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;

    // Per qa-remove-agent-max-tokens (2026-05-17): always hand the
    // dispatcher `None`. `input.max_tokens` (whether it came from a
    // legacy persisted `AgentSlot.max_tokens` or a caller that
    // hand-built a `SlotInput`) is intentionally ignored so the cap
    // resolves from the model library, not from operator config.
    let dispatcher_max_tokens: Option<u32> = None;

    // Cap on tool-use round-trips (qa-execute-slot-cap, 2026-05-17). A
    // misbehaving model that always emits `ToolUse` would otherwise
    // loop until upstream budget exhaustion. The cap counts iterations
    // BEFORE the dispatch call; the final EndTurn/empty-uses turn does
    // NOT consume an iteration since it short-circuits below.
    let mut iterations: usize = 0;
    let mut tool_names_called: Vec<String> = Vec::new();
    let mut last_stop_reason: StopReason = StopReason::EndTurn;

    loop {
        if iterations >= MAX_TOOL_LOOP_ITERATIONS {
            tracing::warn!(
                slot_role = %input.slot.role,
                model = %input.slot.effective_model(),
                iterations,
                last_stop_reason = ?last_stop_reason,
                tool_names = ?tool_names_called,
                input_tokens = total_input_tokens,
                output_tokens = total_output_tokens,
                "execute_slot tool-use loop exhausted iteration cap",
            );
            return Err(anyhow::Error::new(ExecuteSlotError::ToolLoopCapExceeded {
                role: input.slot.role.clone(),
                model: input.slot.effective_model(),
                iterations,
                tool_names: tool_names_called,
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
                last_stop_reason,
            }));
        }

        let req = LlmRequest {
            model: input.slot.effective_model(),
            system_prompt: input.slot.prompt.clone(),
            messages: messages.clone(),
            max_tokens: dispatcher_max_tokens,
            tools: tool_defs.clone(),
            temperature: None,
            response_schema: input
                .response_schema
                .clone()
                .or_else(|| response_schema_for_slot(input.slot)),
        };

        // Open a ModelCall span around this dispatch iteration. Per
        // qa-eval-observability-wiring (2026-05-17): the operator's
        // `[unclassified] error decoding response body` from an eval
        // run never appeared in the trace because the engine's
        // dispatch path had no observability emission. Now every
        // round-trip is bracketed by SpanStarted / SpanFinished, and
        // failures land as `status=error` with the dispatch error
        // message in `error_json` so `SpanInspector` (PR #238) renders
        // it.
        let model_str = req.model.clone();
        let provider_str = input
            .slot
            .provider
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let span_id = fresh_span_id();
        // Compute prompt_hash before `req` is moved into `dispatch.complete(req)`.
        // The digest is deterministic over (system_prompt, messages, tools)
        // and prefixed `sha256:` for explicit algorithm tagging.
        let prompt_hash = crate::agent::observability::compute_prompt_hash(&req);
        // `harness-payload-blob-write`: keep a clone of the prompt so
        // `emit_model_call_finished_with_payloads` can persist it
        // under FullDebug / Redacted retention. The clone is cheap
        // (Arc-shared strings + Vecs) and only retained until the
        // companion emit closes the span, then dropped. Under
        // HashOnly retention the emitter never reads the bytes, so
        // the work is wasted by ~one clone per dispatch — acceptable
        // tradeoff vs. routing the request back through the emitter.
        let prompt_for_blob: Option<crate::agent::llm::LlmRequest> = input.obs.as_ref().map(|_| req.clone());
        if let Some(obs) = input.obs.as_ref() {
            obs.emit_model_call_started(&span_id, None, &provider_str, &model_str, Some(&input.slot.role))
                .await;
        }

        let resp = match input.dispatch.complete(req).await {
            Ok(r) => r,
            Err(e) => {
                if let Some(obs) = input.obs.as_ref() {
                    let msg = format!("{e:#}");
                    obs.emit_span_finished_error(&span_id, &msg).await;
                }
                return Err(e);
            }
        };
        // Accumulate assistant text once; reused for the streaming
        // delta bridge and the response_hash.
        let assistant_text: String = {
            use crate::agent::llm::ContentBlock;
            resp.content
                .iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        };
        let response_hash = if assistant_text.is_empty() {
            None
        } else {
            Some(crate::agent::observability::compute_response_hash(
                &assistant_text,
            ))
        };
        // Bridge to the trace dock's streaming pull-quote: emit the
        // accumulated assistant text as a single `AssistantTextDelta`
        // before closing the span so `SpanInspector` renders the body
        // even though the underlying dispatch is non-streaming today.
        // Real chunked SSE on AnthropicDispatch / OpenaiCompatDispatch
        // is a follow-up — when they emit per-chunk deltas the frontend
        // already accumulates into the same slot.
        //
        // Retention is enforced inside `emit_assistant_text_delta`: when
        // the active policy is anything other than FullDebug +
        // store_responses, the body is suppressed at the source. We
        // still publish the event so the dashboard can update span
        // counts, just without raw text.
        if let Some(obs) = input.obs.as_ref() {
            if !assistant_text.is_empty() {
                obs.emit_assistant_text_delta(&span_id, &assistant_text).await;
            }
        }
        if let Some(obs) = input.obs.as_ref() {
            obs.emit_model_call_finished_with_payloads(
                &span_id,
                &provider_str,
                &model_str,
                Some(resp.input_tokens),
                Some(resp.output_tokens),
                None,
                prompt_hash,
                response_hash,
                prompt_for_blob.as_ref(),
                if assistant_text.is_empty() {
                    None
                } else {
                    Some(assistant_text.as_str())
                },
            )
            .await;
            obs.emit_span_finished_ok(&span_id).await;
        }
        total_input_tokens = total_input_tokens.saturating_add(resp.input_tokens);
        total_output_tokens = total_output_tokens.saturating_add(resp.output_tokens);
        last_stop_reason = resp.stop_reason;

        let uses = tool_call::tool_uses(&resp.content);

        // Final turn: no tool calls, OR the model signalled EndTurn /
        // MaxTokens (defensive — Anthropic shouldn't emit ToolUse with
        // those stop reasons, but we trust the stop_reason as the
        // authoritative signal).
        if uses.is_empty() || matches!(resp.stop_reason, StopReason::EndTurn | StopReason::MaxTokens) {
            return Ok(LlmResponse {
                content: resp.content,
                stop_reason: resp.stop_reason,
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
            });
        }

        // Append the assistant turn carrying the tool_use blocks so the
        // model sees its own request on the next call.
        messages.push(Message {
            role: "assistant".into(),
            content: resp.content.clone(),
        });

        // Run each tool, build a `ToolResult` block per call. Tool errors
        // surface as strings so the model can recover; we don't abort the
        // whole slot on a single bad tool call. `is_error: Some(true)`
        // marks the failure on the Anthropic native shape and prepends an
        // `[is_error: true]` marker on the OpenAI shape, so the model
        // sees the prior tool call failed instead of trusting the
        // content as a normal result. (agent-error-feedback-self-healing.)
        let mut results = Vec::with_capacity(uses.len());
        for (tu_id, tu_name, tu_input) in uses {
            tool_names_called.push(tu_name.clone());
            // F-4 (`harness-span-taxonomy-extension`): bracket each
            // tool invocation with `tool.validate_input` and
            // `tool.validate_output` instantaneous spans. The body
            // of each is a no-op today — F-6
            // (`harness-typed-mechanical-params`) replaces the
            // no-ops with the actual schema validators. We emit the
            // spans now so the wire format / ordering /
            // `tool_name` attribute are pinned before F-6 starts.
            //
            // `validate_output` MUST emit even when the tool call
            // errored — the post-state record is exactly when an
            // operator needs visibility most. parent_span_id is
            // None because the engine eval path does not currently
            // emit `tool.call` spans (that gap is tracked
            // separately); when it starts to, the parent here can
            // be wired without changing the validate-span shape.
            if let Some(obs) = input.obs.as_ref() {
                obs.emit_tool_validate_input(&fresh_span_id(), None, &tu_name)
                    .await;
            }
            let (content, is_error) = match tool_call::invoke(&tu_name, tu_input, input.tools.clone()).await {
                Ok(s) => (s, None),
                Err(e) => (format!("tool error: {e}"), Some(true)),
            };
            if let Some(obs) = input.obs.as_ref() {
                obs.emit_tool_validate_output(&fresh_span_id(), None, &tu_name)
                    .await;
            }
            results.push(ContentBlock::ToolResult {
                tool_use_id: tu_id,
                content,
                is_error,
            });
        }
        messages.push(Message {
            role: "user".into(),
            content: results,
        });

        iterations += 1;
    }
}

pub(crate) fn response_schema_for_slot(slot: &LLMSlot) -> Option<ResponseSchema> {
    if slot.role.eq_ignore_ascii_case("trader") {
        Some(ResponseSchema::trader_output())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{LlmRequest, LlmResponse};
    use crate::tools::ToolRegistry;
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn slot(role: &str) -> LLMSlot {
        LLMSlot {
            role: role.to_string(),
            prompt: "system".into(),
            model_requirement: "test.model".into(),
            allowed_tools: Vec::new(),
            provider: Some("test".into()),
            model: Some("model".into()),
        }
    }

    #[test]
    fn trader_slots_request_the_trader_output_schema() {
        let schema = response_schema_for_slot(&slot("trader")).expect("trader schema");
        assert_eq!(schema.name, "trader_output");
        assert!(schema
            .schema
            .get("required")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("action")));
    }

    #[test]
    fn non_trader_slots_do_not_force_the_trader_schema() {
        assert!(response_schema_for_slot(&slot("intern")).is_none());
    }

    /// Dispatch double that captures the last `LlmRequest` it saw so we
    /// can assert what `execute_slot` handed downstream.
    struct RecordingDispatch {
        seen: Mutex<Vec<LlmRequest>>,
        response: LlmResponse,
    }

    impl RecordingDispatch {
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

        fn last_request(&self) -> LlmRequest {
            self.seen.lock().unwrap().last().cloned().unwrap()
        }
    }

    #[async_trait]
    impl crate::agent::llm::LlmDispatch for RecordingDispatch {
        async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
            self.seen.lock().unwrap().push(req);
            Ok(self.response.clone())
        }
    }

    /// Acceptance test for the 2026-05-17 `qa-remove-agent-max-tokens`
    /// track: `execute_slot` MUST hand the dispatcher `max_tokens: None`
    /// regardless of what `SlotInput.max_tokens` carries, so the
    /// downstream Anthropic dispatcher falls back to
    /// `lookup_model(model).auto_max_tokens()` (the model-library cap)
    /// and the OpenAI-compat dispatcher omits the field. Operators
    /// previously setting `4096` on a 384k-output model used to silently
    /// cap real production runs; that override is now ignored.
    #[tokio::test]
    async fn execute_slot_ignores_persisted_max_tokens_and_hands_dispatcher_none() {
        let slot = LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: Vec::new(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        };
        let dispatch = std::sync::Arc::new(RecordingDispatch::new(
            r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
        ));
        let tools = std::sync::Arc::new(ToolRegistry::default_with_builtins());

        // Operator persisted a stale 4096 override on this agent slot.
        let out = execute_slot(SlotInput {
            slot: &slot,
            upstream_inputs: serde_json::json!({}),
            dispatch: dispatch.clone(),
            tools,
            response_schema: None,
            max_tokens: Some(4096),
            obs: None,
        })
        .await
        .unwrap();

        assert!(out.text().contains("hold"));
        let req = dispatch.last_request();
        assert_eq!(
            req.max_tokens, None,
            "execute_slot must drop persisted max_tokens so llm.rs resolves \
             the cap from the model library; got {:?}",
            req.max_tokens,
        );
    }

    /// Companion test: `None` on `SlotInput` also flows through as
    /// `None` on the dispatcher request. Together with the test above,
    /// this pins the "always None at the dispatcher boundary" contract.
    #[tokio::test]
    async fn execute_slot_with_unset_max_tokens_hands_dispatcher_none() {
        let slot = LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: Vec::new(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        };
        let dispatch = std::sync::Arc::new(RecordingDispatch::new(
            r#"{"action":"hold","conviction":0.5,"justification":"test"}"#,
        ));
        let tools = std::sync::Arc::new(ToolRegistry::default_with_builtins());

        execute_slot(SlotInput {
            slot: &slot,
            upstream_inputs: serde_json::json!({}),
            dispatch: dispatch.clone(),
            tools,
            response_schema: None,
            max_tokens: None,
            obs: None,
        })
        .await
        .unwrap();

        let req = dispatch.last_request();
        assert_eq!(req.max_tokens, None);
    }
}
