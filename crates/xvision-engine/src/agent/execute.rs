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
use crate::agent::tool_call;
use crate::strategies::slot::LLMSlot;
use crate::tools::ToolRegistry;

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
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let initial_user = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data; emit your final decision as JSON.",
        serde_json::to_string_pretty(&input.upstream_inputs)?
    );

    let tool_defs = tool_call::definitions_for_slot(&input.slot.allowed_tools, &input.tools);

    let mut messages = vec![Message {
        role: "user".into(),
        content: vec![ContentBlock::Text { text: initial_user }],
    }];

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;

    // Per qa-remove-agent-max-tokens (2026-05-17): always hand the
    // dispatcher `None`. `input.max_tokens` (whether it came from a
    // legacy persisted `AgentSlot.max_tokens` or a caller that
    // hand-built a `SlotInput`) is intentionally ignored so the cap
    // resolves from the model library, not from operator config.
    let dispatcher_max_tokens: Option<u32> = None;

    loop {
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
        let resp = input.dispatch.complete(req).await?;
        total_input_tokens = total_input_tokens.saturating_add(resp.input_tokens);
        total_output_tokens = total_output_tokens.saturating_add(resp.output_tokens);

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
        // whole slot on a single bad tool call.
        let mut results = Vec::with_capacity(uses.len());
        for (tu_id, tu_name, tu_input) in uses {
            let content = match tool_call::invoke(&tu_name, tu_input, input.tools.clone()).await {
                Ok(s) => s,
                Err(e) => format!("tool error: {e}"),
            };
            results.push(ContentBlock::ToolResult {
                tool_use_id: tu_id,
                content,
            });
        }
        messages.push(Message {
            role: "user".into(),
            content: results,
        });
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
        })
        .await
        .unwrap();

        let req = dispatch.last_request();
        assert_eq!(req.max_tokens, None);
    }
}
