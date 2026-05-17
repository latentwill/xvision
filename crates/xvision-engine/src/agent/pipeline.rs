use std::sync::Arc;

use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{LlmDispatch, LlmResponse, ResponseSchema};
use crate::agents::AgentSlot;
use crate::strategies::agent_ref::canonical_role;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineKind, Strategy};
use crate::tools::ToolRegistry;
use xvision_core::providers::lookup_model;

#[derive(Debug, Clone)]
pub struct ResolvedAgentSlot {
    pub role: String,
    pub slot: LLMSlot,
    /// Operator's per-request output-token budget. `None` lets the
    /// dispatcher decide: OpenAI-compat omits the field entirely (the
    /// provider applies its own default); Anthropic falls back to the
    /// per-model auto value because the API requires the field. Explicit
    /// values pass through verbatim — no clamping.
    pub max_tokens: Option<u32>,
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
        let max_tokens = default_max_tokens_for(slot);
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: None,
            max_tokens,
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
        let max_tokens = default_max_tokens_for(slot);
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: None,
            max_tokens,
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
        let max_tokens = default_max_tokens_for(slot);
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: Some(ResponseSchema::trader_output()),
            max_tokens,
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

    for resolved in input.agent_slots.iter() {
        // Single canonical comparison key (trim + lowercase) so the
        // trader-output schema selection and the output-assignment
        // match arm can never disagree. Pre-canonicalization, the
        // schema check was case-insensitive but the match was
        // case-sensitive — so an attached `Trader` slot ran with the
        // right schema and then silently dropped its result (QA #5).
        let role_key = canonical_role(&resolved.role);
        let is_trader_output = role_key == "trader";
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
            max_tokens: resolved.max_tokens,
        })
        .await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated[format!("{role_key}_output")] = serde_json::Value::String(out.text());

        match role_key.as_str() {
            "regime" => regime = Some(out.clone()),
            "intern" => intern = Some(out.clone()),
            "trader" => trader = Some(out.clone()),
            _ => {}
        }
    }

    Ok(PipelineOutputs {
        regime,
        intern,
        trader,
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

/// Build a `ResolvedAgentSlot` from an `AgentSlot`, resolving the
/// effective `max_tokens` once at strategy-construction time. Callers in
/// `api/eval.rs` use this so the eval executor never has to look at
/// `AgentSlot` directly.
pub fn resolve_agent_slot(role: &str, slot: &AgentSlot) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.to_string(),
        slot: agent_slot_to_llm_slot(role, slot),
        max_tokens: slot.resolve_max_tokens(),
    }
}

/// Legacy `LLMSlot` path (regime/intern/trader slots on the older
/// `Strategy` shape) has no operator-side `max_tokens` field. To keep
/// existing legacy strategies on their previous budget after the q15
/// `Option<u32>` rework, we auto-derive from the slot's model metadata
/// so the dispatcher sees a concrete value — matching the pre-change
/// behaviour exactly. (The agent-slot path, by contrast, exposes the
/// `Option<u32>` to the operator and only fills in a fallback inside
/// the Anthropic dispatcher where the API requires the field.)
fn default_max_tokens_for(slot: &LLMSlot) -> Option<u32> {
    let model = slot.effective_model();
    let model = model.trim();
    if model.is_empty() {
        // No resolvable model id — fall back to the unknown-model auto
        // (4096), which is what the legacy path used to return for
        // empty/unrecognised slots.
        return Some(xvision_core::providers::ModelMetadata::unknown_default("").auto_max_tokens());
    }
    Some(lookup_model(model).auto_max_tokens())
}

#[cfg(test)]
mod legacy_max_tokens_tests {
    use super::*;

    fn slot_with_model(model: &str) -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            prompt: "p".into(),
            model_requirement: model.to_string(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some(model.to_string()),
        }
    }

    #[test]
    fn legacy_slot_with_known_model_returns_per_model_auto() {
        let slot = slot_with_model("claude-sonnet-4-6");
        let meta = lookup_model("claude-sonnet-4-6");
        assert_eq!(
            default_max_tokens_for(&slot),
            Some(meta.auto_max_tokens()),
            "legacy slots must keep producing the per-model auto so existing OpenAI-compat \
             strategies don't silently shift to the provider's own default",
        );
    }

    #[test]
    fn legacy_slot_with_unknown_model_returns_unknown_default_auto() {
        let slot = slot_with_model("acme-private-model-9000");
        assert_eq!(default_max_tokens_for(&slot), Some(4096));
    }

    #[test]
    fn legacy_slot_with_no_resolvable_model_returns_unknown_default_auto() {
        let mut slot = slot_with_model("");
        slot.model = None;
        slot.model_requirement = "".into();
        assert_eq!(default_max_tokens_for(&slot), Some(4096));
    }
}
