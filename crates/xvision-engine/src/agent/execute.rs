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

    loop {
        let req = LlmRequest {
            model: input.slot.effective_model(),
            system_prompt: input.slot.prompt.clone(),
            messages: messages.clone(),
            max_tokens: 1000,
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
}
