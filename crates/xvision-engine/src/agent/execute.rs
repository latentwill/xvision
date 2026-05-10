//! `execute_slot` — drives one LLM slot through a tool-use loop.
//!
//! The slot's `allowed_tools` list is converted into `ToolDefinition`s and
//! advertised to the model each turn. When the model emits
//! `ContentBlock::ToolUse` blocks we route them through the slot's
//! `ToolRegistry`, append `ToolResult` blocks to the conversation, and
//! re-call until the model emits a text-only `EndTurn` (or hits the
//! iteration cap, which fails the slot rather than returning a partial
//! decision).

use std::sync::Arc;

use crate::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason,
};
use crate::agent::tool_call;
use crate::bundle::slot::LLMSlot;
use crate::tools::ToolRegistry;

/// Hard cap on tool-use turns per slot. Real Stage-1 Intern reasoning
/// tops out at 3-4 round trips; 8 absorbs surprise without letting a
/// runaway loop bill arbitrary tokens.
const MAX_TOOL_USE_ITERATIONS: u32 = 8;

pub struct SlotInput<'a> {
    pub slot: &'a LLMSlot,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let initial_user = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data; emit your final decision as JSON.",
        serde_json::to_string_pretty(&input.upstream_inputs)?
    );

    let tool_defs =
        tool_call::definitions_for_slot(&input.slot.allowed_tools, &input.tools);

    let mut messages = vec![Message {
        role: "user".into(),
        content: vec![ContentBlock::Text {
            text: initial_user,
        }],
    }];

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;

    for _iter in 0..MAX_TOOL_USE_ITERATIONS {
        let req = LlmRequest {
            model: input.slot.model_requirement.clone(),
            system_prompt: input.slot.prompt.clone(),
            messages: messages.clone(),
            max_tokens: 1000,
            tools: tool_defs.clone(),
        };
        let resp = input.dispatch.complete(req).await?;
        total_input_tokens += resp.input_tokens;
        total_output_tokens += resp.output_tokens;

        let uses = tool_call::tool_uses(&resp.content);

        // Final turn: no tool calls, OR the model signalled EndTurn /
        // MaxTokens (defensive — Anthropic shouldn't emit ToolUse with
        // those stop reasons, but we trust the stop_reason as the
        // authoritative signal).
        if uses.is_empty()
            || matches!(
                resp.stop_reason,
                StopReason::EndTurn | StopReason::MaxTokens
            )
        {
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
            let content =
                match tool_call::invoke(&tu_name, tu_input, input.tools.clone()).await {
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

    anyhow::bail!(
        "execute_slot exceeded {MAX_TOOL_USE_ITERATIONS} tool-use iterations \
         — the model is stuck calling tools without producing a final decision"
    )
}
