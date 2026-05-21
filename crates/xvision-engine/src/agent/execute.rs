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
use crate::agent::memory_recorder::{render_recalled_patterns, RecallResult};
use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::agent::tool_call;
use crate::strategies::slot::LLMSlot;
use crate::tools::ToolRegistry;
use xvision_memory::types::Namespace;

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
    /// Operator's per-request output-token budget. Threaded directly
    /// into the `LlmRequest.max_tokens` field on every dispatch
    /// iteration. `None` lets each provider decide: Anthropic falls
    /// back to the per-model auto value at the wire boundary (the API
    /// requires the field), OpenAI-compat omits the field entirely
    /// (so the provider applies its own — usually much larger —
    /// default). Explicit `Some(n)` values pass through verbatim —
    /// no clamping.
    ///
    /// History: an earlier `qa-remove-agent-max-tokens` (2026-05-17)
    /// track temporarily hard-coded this to `None` because the
    /// persisted `AgentSlot.max_tokens` shape had no way to *unset* a
    /// previously-saved cap. That footgun has been replaced by the
    /// `Option<u32>` shape (SQLite sentinel `0` round-trips back to
    /// `None`), so harness audit F-4
    /// (`agent-config-asset-coherence-and-token-forward`, 2026-05-19)
    /// re-enables forwarding.
    pub max_tokens: Option<u32>,
    /// Operator's per-request sampling temperature. Threaded through
    /// from `ResolvedAgentSlot.temperature` so the outbound dispatch
    /// body carries the operator's intent. `None` lets the provider
    /// apply its own default — the OpenAI-compat and Anthropic body
    /// builders both omit `temperature` from the JSON when this is
    /// `None`, so legacy callers (the agent-loop pipeline, in-tree
    /// integration tests) opt out trivially.
    pub temperature: Option<f64>,
    /// Observability emitter (`qa-eval-observability-wiring`, 2026-05-17).
    /// When `Some`, every LLM dispatch inside this slot emits a
    /// `ModelCall` span + `ModelCallFinished` (success) or
    /// `SpanFinished{Error}` (failure) on the observability bus, so
    /// eval runs surface in `/api/agent-runs/<run_id>` and the trace
    /// dock renders failures (PR #238). `None` is the default —
    /// existing call sites (legacy pipeline, unit tests) opt out
    /// trivially and the emit code becomes a no-op.
    pub obs: Option<ObsEmitter>,
    /// Optional V2D memory recorder. `Some` enables auto-recall before
    /// the first dispatch iteration and auto-write after the final
    /// `EndTurn`. `None` (or a recorder whose mode is Off) is a no-op.
    pub memory: Option<Arc<crate::agent::memory_recorder::MemoryRecorder>>,
    /// The slot's resolved memory mode (snapshotted from
    /// `AgentSlot.memory_mode` at dispatch time). Combined with
    /// `agent_id`, the recorder derives the namespace at recall/record
    /// time. Two scalars instead of one because `execute_slot` is one
    /// level below where the slot + agent are joined and we don't want
    /// to plumb the Agent through.
    pub memory_mode: xvision_memory::types::MemoryMode,
    /// Owning agent id for memory namespacing. Empty string when the
    /// slot has no associated agent (legacy `LLMSlot` pipeline, unit
    /// tests). With `memory: None` or `memory_mode: Off` this is
    /// ignored.
    pub agent_id: String,
    /// V2D Phase 1.5 — current scenario start. Forwarded to
    /// `MemoryRecorder::recall` so the store can exclude Patterns
    /// whose `training_window_end` overlaps the scenario. `None` is
    /// the safe default for live/paper mode (no replay risk) and for
    /// every non-eval call site (unit tests, legacy `LLMSlot` pipeline).
    pub scenario_start: Option<chrono::DateTime<chrono::Utc>>,
    /// V2D Phase 1.5 — current run id. Plumbed into Observation
    /// provenance on write. Empty string when memory is off / the
    /// slot has no associated run (the recorder will no-op).
    pub run_id: String,
    /// V2D Phase 1.5 — current scenario id. Plumbed into Observation
    /// provenance on write. Empty string when memory is off.
    pub scenario_id: String,
    /// V2D Phase 1.5 — current decision-cycle index. Plumbed into
    /// Observation provenance on write. `0` when memory is off.
    pub cycle_idx: i64,
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

    // V2D: recall before dispatch. The recall is bounded by the slot's
    // `memory_mode` (Off => Skipped) and the engine's configured
    // embedder (`NoEmbedder` => log + no-op). When hits land, prepend
    // a `<prior_observations>` block to the system prompt so the model
    // sees a stable summary of related earlier decisions.
    let prior_block = if let Some(recorder) = &input.memory {
        let query_text = serde_json::to_string(&input.upstream_inputs).unwrap_or_default();
        match recorder
            .recall(
                input.memory_mode,
                &input.agent_id,
                &query_text,
                5,
                input.scenario_start,
            )
            .await?
        {
            RecallResult::Skipped => None,
            RecallResult::NoEmbedder { namespace } => {
                tracing::info!(
                    event = "memory_disabled_no_embedder",
                    namespace = %namespace,
                    "V2D memory recall skipped: no embedder configured",
                );
                None
            }
            RecallResult::Hits { namespace, matches } => {
                tracing::info!(
                    event = "memory_recall",
                    namespace = %namespace,
                    k = matches.len(),
                    "V2D memory recall hits",
                );
                // Zero hits → no block. An empty `<prior_observations>`
                // shell would just waste tokens and trip the leakage
                // T-filter tests that assert absence on suppression.
                if matches.is_empty() {
                    None
                } else {
                    Some(render_recalled_patterns(&matches))
                }
            }
        }
    } else {
        None
    };

    let assembled_system_prompt = match prior_block {
        Some(block) => format!("{block}\n\n{}", input.slot.prompt),
        None => input.slot.prompt.clone(),
    };

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;

    // Per harness audit F-4 (`agent-config-asset-coherence-and-token-
    // forward`, 2026-05-19): the operator's `AgentSlot.max_tokens` is
    // now forwarded to the dispatcher. The earlier
    // `qa-remove-agent-max-tokens` (2026-05-17) hard-coded `None` here
    // because the API surface offered no way to *unset* a previously-
    // saved cap; that footgun has been replaced by the
    // `AgentSlot.max_tokens: Option<u32>` shape (the SQLite sentinel
    // `0` round-trips back to `None`), so explicit operator values can
    // safely flow through. `None` still means "let the dispatcher
    // decide" — Anthropic falls back to the per-model auto value at
    // the wire boundary; OpenAI-compat omits the field entirely.
    let dispatcher_max_tokens: Option<u32> = input.max_tokens;
    let dispatcher_temperature: Option<f64> = input.temperature;

    // Cap on tool-use round-trips (qa-execute-slot-cap, 2026-05-17). A
    // misbehaving model that always emits `ToolUse` would otherwise
    // loop until upstream budget exhaustion. The cap counts iterations
    // BEFORE the dispatch call; the final EndTurn/empty-uses turn does
    // NOT consume an iteration since it short-circuits below.
    let mut iterations: usize = 0;
    let mut tool_names_called: Vec<String> = Vec::new();
    let mut last_stop_reason: StopReason = StopReason::EndTurn;
    // F-5 (`harness-recovery-state-machine`): per-slot block-list for
    // repeated `(tool_name, input_hash)` failures. The first
    // two failures of a given pair pass through (the model gets the
    // `is_error: true` tool_result and self-heals via the
    // `agent-error-feedback-self-healing` path). The third failure
    // trips the block and emits `recovery.failed`; later attempts of
    // the same pair are short-circuited with a typed
    // `repeated_tool_failure` error injected as the tool_result so the
    // model sees the block instead of looping.
    let mut repeated_failures = crate::agent::recovery::RepeatedToolFailureTracker::new();

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
            system_prompt: assembled_system_prompt.clone(),
            messages: messages.clone(),
            max_tokens: dispatcher_max_tokens,
            tools: tool_defs.clone(),
            temperature: dispatcher_temperature,
            response_schema: input
                .response_schema
                .clone()
                .or_else(|| response_schema_for_slot(input.slot)),
            // F-8: the dispatcher evaluates the prompt-cache trigger
            // (env XVN_PROMPT_CACHE=1 + non-empty system_prompt +
            // bar_history > 1 entry) and emits cache_control on the
            // wire when appropriate. Callers that want to force the
            // hint set this directly; `execute_slot` leaves it None.
            cache_control: None,
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
            // V2D: record the final decision text into the slot's
            // namespace. No-op when memory is None / Off / no embedder.
            if let Some(recorder) = &input.memory {
                if !assistant_text.is_empty() {
                    match recorder
                        .record(
                            input.memory_mode,
                            &input.agent_id,
                            &assistant_text,
                            input.run_id.clone(),
                            input.scenario_id.clone(),
                            input.cycle_idx,
                        )
                        .await
                    {
                        Ok(Some(id)) => {
                            let ns = Namespace::for_mode(input.memory_mode, &input.agent_id);
                            tracing::info!(
                                event = "memory_write",
                                namespace = %ns.as_str(),
                                id = %id,
                                "V2D memory write",
                            );
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(
                                event = "memory_write_error",
                                error = %e,
                                "V2D memory write failed; continuing",
                            );
                        }
                    }
                }
            }
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
            // F-5: short-circuit if the block-list says this exact
            // (tool_name, input_hash) pair has already tripped the
            // repeated-failure block. The model receives a structured
            // `repeated_tool_failure` tool_result.
            let blocked = repeated_failures.is_blocked(&tu_name, &tu_input);
            let (content, is_error) = if blocked {
                (repeated_tool_failure_result(&tu_name), Some(true))
            } else {
                match tool_call::invoke(&tu_name, tu_input.clone(), input.tools.clone()).await {
                    Ok(s) => (s, None),
                    Err(e) => {
                        let count = repeated_failures.record_failure(&tu_name, &tu_input);
                        if count >= crate::agent::recovery::MAX_TOOL_RETRIES_PER_PAIR {
                            let input_hash =
                                crate::agent::recovery::RepeatedToolFailureTracker::input_hash(&tu_input);
                            if let Some(obs) = input.obs.as_ref() {
                                obs.emit_recovery_failed(
                                    &fresh_span_id(),
                                    None,
                                    "repeated_tool_failure",
                                    count as u32,
                                    &format!(
                                        "tool '{tu_name}' input_hash={input_hash} failed {count} \
                                         times; further calls with this exact input are blocked"
                                    ),
                                )
                                .await;
                            }
                            (repeated_tool_failure_result(&tu_name), Some(true))
                        } else {
                            // First failure of a pair is normal
                            // self-healing territory (the agent gets
                            // `is_error: true` and re-decides). The
                            // second failure is the recovery seam —
                            // emit `recovery.attempt` so the trace dock
                            // surfaces the retry pressure before the
                            // third failure trips the block.
                            if count >= 2 {
                                if let Some(obs) = input.obs.as_ref() {
                                    obs.emit_recovery_attempt(
                                        &fresh_span_id(),
                                        None,
                                        "repeated_tool_failure",
                                        (count - 1) as u32,
                                    )
                                    .await;
                                }
                            }
                            (format!("tool error: {e}"), Some(true))
                        }
                    }
                }
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

fn repeated_tool_failure_result(tool_name: &str) -> String {
    format!(
        "repeated_tool_failure: tool '{tool_name}' with this exact \
         input has failed {} times in this slot execution. The \
         input is blocked for the remainder of this run. Retry \
         with a different input or choose a different tool.",
        crate::agent::recovery::MAX_TOOL_RETRIES_PER_PAIR
    )
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

    /// Acceptance test for the 2026-05-19 F-4 carve
    /// (`agent-config-asset-coherence-and-token-forward`): operator-
    /// persisted `AgentSlot.max_tokens` IS now forwarded to the
    /// dispatcher. The earlier 2026-05-17 `qa-remove-agent-max-tokens`
    /// track had hard-coded `None` here because the persisted shape
    /// offered no way to *unset* a previously-saved cap; the
    /// `Option<u32>` shape (SQLite sentinel `0` round-trips to `None`)
    /// fixes that footgun so explicit operator values flow through
    /// safely. See the F-4 audit (`3 agent_slots carry max_tokens=0,
    /// but the actual outbound prompt blob has max_tokens: None`) for
    /// the motivating regression.
    #[tokio::test]
    async fn execute_slot_forwards_persisted_max_tokens_to_dispatcher() {
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

        let out = execute_slot(SlotInput {
            slot: &slot,
            upstream_inputs: serde_json::json!({}),
            dispatch: dispatch.clone(),
            tools,
            response_schema: None,
            max_tokens: Some(4096),
            temperature: Some(0.2),
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
        .unwrap();

        assert!(out.text().contains("hold"));
        let req = dispatch.last_request();
        assert_eq!(
            req.max_tokens,
            Some(4096),
            "execute_slot must forward SlotInput.max_tokens verbatim; got {:?}",
            req.max_tokens,
        );
        assert_eq!(
            req.temperature,
            Some(0.2),
            "execute_slot must forward SlotInput.temperature verbatim; got {:?}",
            req.temperature,
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
        .unwrap();

        let req = dispatch.last_request();
        assert_eq!(req.max_tokens, None);
    }
}
