use std::sync::Arc;

use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{LlmDispatch, LlmResponse, ResponseSchema};
use crate::agents::AgentSlot;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineKind, Strategy};
use crate::tools::ToolRegistry;

#[derive(Debug, Clone)]
pub struct ResolvedAgentSlot {
    pub role: String,
    pub slot: LLMSlot,
}

pub struct PipelineInputs<'a> {
    pub strategy: &'a Strategy,
    pub agent_slots: &'a [ResolvedAgentSlot],
    pub seed_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

#[derive(Debug)]
pub struct PipelineOutputs {
    pub regime: Option<LlmResponse>,
    pub intern: Option<LlmResponse>,
    pub trader: Option<LlmResponse>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

pub async fn run_pipeline<'a>(input: PipelineInputs<'a>) -> anyhow::Result<PipelineOutputs> {
    if !input.agent_slots.is_empty() {
        return run_agent_pipeline(input).await;
    }

    let mut accumulated = input.seed_inputs.clone();
    let mut total_in = 0u32;
    let mut total_out = 0u32;

    let regime = if let Some(slot) = &input.strategy.regime_slot {
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: None,
        })
        .await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated["regime_output"] = serde_json::Value::String(out.text());
        Some(out)
    } else {
        None
    };

    let intern = if let Some(slot) = &input.strategy.intern_slot {
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: None,
        })
        .await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated["intern_output"] = serde_json::Value::String(out.text());
        Some(out)
    } else {
        None
    };

    let trader = if let Some(slot) = &input.strategy.trader_slot {
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: Some(ResponseSchema::trader_output()),
        })
        .await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        Some(out)
    } else {
        None
    };

    Ok(PipelineOutputs {
        regime,
        intern,
        trader,
        total_input_tokens: total_in,
        total_output_tokens: total_out,
    })
}

async fn run_agent_pipeline<'a>(input: PipelineInputs<'a>) -> anyhow::Result<PipelineOutputs> {
    if input.strategy.pipeline.kind == PipelineKind::Graph {
        anyhow::bail!("graph agent pipelines are not executable yet");
    }

    let mut accumulated = input.seed_inputs.clone();
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    let mut regime = None;
    let mut intern = None;
    let mut trader = None;
    let mut last = None;

    let last_index = input.agent_slots.len().saturating_sub(1);
    for (index, resolved) in input.agent_slots.iter().enumerate() {
        let is_trader_output = resolved.role.eq_ignore_ascii_case("trader") || index == last_index;
        let out = execute_slot(SlotInput {
            slot: &resolved.slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: if is_trader_output {
                Some(ResponseSchema::trader_output())
            } else {
                None
            },
        })
        .await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated[format!("{}_output", resolved.role)] = serde_json::Value::String(out.text());

        match resolved.role.as_str() {
            "regime" => regime = Some(out.clone()),
            "intern" => intern = Some(out.clone()),
            "trader" => trader = Some(out.clone()),
            _ => {}
        }
        last = Some(out);
    }

    Ok(PipelineOutputs {
        regime,
        intern,
        trader: trader.or(last),
        total_input_tokens: total_in,
        total_output_tokens: total_out,
    })
}

pub fn agent_slot_to_llm_slot(role: &str, slot: &AgentSlot) -> LLMSlot {
    LLMSlot {
        role: role.to_string(),
        prompt: slot.system_prompt.clone(),
        model_requirement: if slot.provider.trim().is_empty() {
            slot.model.clone()
        } else {
            format!("{}.{}", slot.provider, slot.model)
        },
        allowed_tools: Vec::new(),
        provider: if slot.provider.trim().is_empty() {
            None
        } else {
            Some(slot.provider.clone())
        },
        model: if slot.model.trim().is_empty() {
            None
        } else {
            Some(slot.model.clone())
        },
    }
}
