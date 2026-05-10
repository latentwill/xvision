use std::sync::Arc;

use crate::agent::llm::{LlmDispatch, LlmRequest, LlmResponse, Message};
use crate::bundle::slot::LLMSlot;
use crate::tools::ToolRegistry;

pub struct SlotInput<'a> {
    pub slot: &'a LLMSlot,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let user_prompt = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions and emit JSON.",
        serde_json::to_string_pretty(&input.upstream_inputs)?
    );
    let req = LlmRequest {
        model: input.slot.model_requirement.clone(),
        system_prompt: input.slot.prompt.clone(),
        messages: vec![Message::user_text(user_prompt)],
        max_tokens: 1000,
        tools: vec![],
    };
    let resp = input.dispatch.complete(req).await?;
    // Tool allowlist enforcement deferred to Plan 2a Phase 2A.C T11 — when
    // the LLM emits a tool_use block we will route through input.tools.
    let _ = input.tools;
    Ok(resp)
}
