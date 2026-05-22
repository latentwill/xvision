use std::sync::Arc;

use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, ResponseSchema, StopReason};
use crate::agent::observability::ObsEmitter;
use crate::agents::{AgentSlot, InputsPolicy};
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
    /// Operator's per-request sampling temperature. `None` lets the
    /// provider apply its own default. `Some(t)` is passed through to
    /// the outbound request body verbatim — Anthropic's
    /// `anthropic_request_body` and the OpenAI-compat
    /// `openai_compat_request_body` both omit `temperature` when the
    /// `LlmRequest` field is `None`, so callers that don't set it
    /// stay on legacy behaviour.
    ///
    /// Wired from `AgentSlot.temperature` at strategy-resolution time
    /// via `resolve_agent_slot`; see `crates/xvision-engine/src/eval/
    /// executor/{paper,backtest}.rs` for the dispatch call sites.
    pub temperature: Option<f64>,
    /// Per-slot seed-sanitization policy (F-6). The eval executor reads
    /// this off the trader-role slot before constructing the seed JSON
    /// — `Causal` strips `timestamp` from `bar_history` (replacing it
    /// with `bar_index`) and drops `decision_index` from the top-level
    /// seed. `Raw` (the default) and `Oracle` produce byte-identical
    /// JSON. See harness audit F-6.
    pub inputs_policy: InputsPolicy,
    /// Optional cap on the number of `bar_history` entries surfaced to
    /// the trader LLM at each decision (F-8). `None` preserves today's
    /// behavior (no cap — the full `warmup_bars`-sized slice). `Some(n)`
    /// trims the slice to its most-recent `n` entries so the prompt
    /// prefix stays stable across many decisions and provider prompt
    /// caching (Anthropic) can land a hit on the static portion.
    pub bar_history_limit: Option<u32>,
    /// V2D: snapshotted memory mode for this slot. Threaded into
    /// `SlotInput.memory_mode` so the dispatcher's recall/write seam
    /// can derive the namespace via `xvision_memory::Namespace::for_mode`.
    /// `Off` (the default) keeps the recorder dormant — legacy callers
    /// that don't set this opt out trivially.
    pub memory_mode: xvision_memory::types::MemoryMode,
    /// V2D: the owning agent's id, populated from the parent `Agent`
    /// row at strategy-resolution time so the recorder can scope memory
    /// per agent (`agent:<agent_id>`). Empty string when the slot has
    /// no associated agent (legacy regime/intern/trader `LLMSlot` path,
    /// unit tests). With `memory_mode = Off` this field is ignored.
    pub agent_id: String,
    /// Snapshotted `AgentSlot.noop_skip` at strategy-resolution time.
    /// `true` (the effective default for both `None` and `Some(true)`)
    /// enables the pre-LLM zero-legal-actions gate on trader-role slots;
    /// `false` disables it so the LLM runs even in a corner.
    /// Non-trader roles always run regardless of this flag.
    pub noop_skip: bool,
}

pub struct PipelineInputs<'a> {
    pub strategy: &'a Strategy,
    pub agent_slots: &'a [ResolvedAgentSlot],
    pub seed_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
    /// Optional observability emitter threaded down into every
    /// `execute_slot` call (`qa-eval-observability-wiring`,
    /// 2026-05-17). `None` is the default — every existing call site
    /// inherits the no-op path without code changes, and the eval
    /// executors opt in via `BacktestExecutor::with_observability_bus`.
    pub obs: Option<ObsEmitter>,
    /// V2D: optional cortex-memory recorder. Threaded into every
    /// `execute_slot` call so per-slot `memory_mode = AgentScoped`
    /// (or future modes) triggers a recall before dispatch and a
    /// write after the final EndTurn. `None` is the safe default —
    /// legacy callers (unit tests, CLI rehearsal, non-eval paths)
    /// stay on the no-op recall path. The eval executors thread
    /// `ApiContext.memory_recorder` here when the server has a
    /// store + embedder configured.
    pub memory_recorder: Option<std::sync::Arc<crate::agent::memory_recorder::MemoryRecorder>>,
    /// V2D Phase 1.5 — current scenario start, forwarded into
    /// `SlotInput.scenario_start` so the recorder's recall path can
    /// exclude Patterns whose `training_window_end` overlaps the
    /// scenario. `None` is the safe default (live/paper mode or
    /// non-eval call sites — no temporal filter applied).
    pub scenario_start: Option<chrono::DateTime<chrono::Utc>>,
    /// V2D Phase 1.5 — current run id, forwarded into Observation
    /// provenance on memory write. Empty string when no run is
    /// associated.
    pub run_id: String,
    /// V2D Phase 1.5 — current scenario id, forwarded into Observation
    /// provenance on memory write. Empty string when no scenario is
    /// associated.
    pub scenario_id: String,
    /// V2D Phase 1.5 — current decision-cycle index, forwarded into
    /// Observation provenance on memory write. `0` is the safe default.
    pub cycle_idx: i64,
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
        // Legacy `LLMSlot` path has no associated `AgentSlot.memory_mode`
        // or owning `Agent.agent_id`, so the recorder stays off here even
        // if a recorder was provided. Only the agent-slot path opts in.
        let out = execute_slot(SlotInput {
            slot,
            upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(),
            tools: input.tools.clone(),
            response_schema: None,
            max_tokens,
            temperature: None,
            obs: input.obs.clone(),
            memory: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            scenario_start: None,
            run_id: String::new(),
            scenario_id: String::new(),
            cycle_idx: 0,
            catalog: None,
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
            temperature: None,
            obs: input.obs.clone(),
            memory: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            scenario_start: None,
            run_id: String::new(),
            scenario_id: String::new(),
            cycle_idx: 0,
            catalog: None,
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
            temperature: None,
            obs: input.obs.clone(),
            memory: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            scenario_start: None,
            run_id: String::new(),
            scenario_id: String::new(),
            cycle_idx: 0,
            catalog: None,
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

/// Returns true when the current seed's `portfolio_state.position_size`
/// indicates that both `long_open` and `short_open` are blocked — i.e. the
/// only legal action for the trader is `hold`. This is the "zero legal open
/// actions" condition defined in the trader-noop-skip intake.
///
/// The check is purely on the seed JSON: a non-zero `position_size` means the
/// portfolio already carries a position on the cycle's asset (long > 0 or
/// short < 0), so the risk gate would block any new open on the same asset
/// before the LLM response even gets there. If `position_size` is absent or
/// cannot be parsed as a float, the gate does NOT fire (conservative default
/// — run the LLM).
fn seed_has_zero_legal_opens(seed: &serde_json::Value) -> bool {
    let ps = match seed.get("portfolio_state") {
        Some(v) => v,
        None => return false,
    };
    let size = match ps.get("position_size").and_then(|v| v.as_f64()) {
        Some(f) => f,
        None => return false,
    };
    size != 0.0
}

/// Synthesize a `TraderDecision`-shaped `LlmResponse` that records the
/// noop-skip without calling the LLM. The JSON body is valid trader output
/// (`action: hold`, `conviction: 0`) with `noop_skip` in the `justification`
/// so the trace/eval review surface can distinguish it from a genuine LLM
/// hold while preserving the strict trader-output schema. Token counts are
/// both 0 — no provider was called.
fn noop_skip_response() -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: r#"{"action":"hold","conviction":0.0,"justification":"noop_skip: portfolio already carries a position — only hold is legal"}"#.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    }
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

        // trader-noop-skip: before paying the LLM tax, check whether this
        // is a trader slot with noop_skip enabled AND the current seed has
        // zero legal open actions (position already held). If so, return a
        // synthesized hold decision with `noop_skip` provenance in the
        // justification so the trace shows the skip. We still record the
        // output key so downstream slots (if any) see a `trader_output`
        // field — matching the normal pipeline shape.
        if is_trader_output && resolved.noop_skip && seed_has_zero_legal_opens(&accumulated) {
            tracing::debug!(
                event = "noop_skip",
                role = %resolved.role,
                "trader-noop-skip: portfolio already carries a position — skipping LLM call",
            );
            let skip_out = noop_skip_response();
            accumulated[format!("{role_key}_output")] = serde_json::Value::String(skip_out.text());
            trader = Some(skip_out);
            continue;
        }

        // V2D: thread the per-slot memory snapshot + the executor's
        // recorder into `execute_slot`. The recorder treats `Off` /
        // empty-namespace as a no-op, so non-eval call sites that don't
        // pass a recorder stay on the legacy path.
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
            temperature: resolved.temperature,
            obs: input.obs.clone(),
            memory: input.memory_recorder.clone(),
            memory_mode: resolved.memory_mode,
            agent_id: resolved.agent_id.clone(),
            scenario_start: input.scenario_start,
            run_id: input.run_id.clone(),
            scenario_id: input.scenario_id.clone(),
            cycle_idx: input.cycle_idx,
            catalog: None,
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
        attested_with: if slot.provider.trim().is_empty() {
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
///
/// `agent_id` is the owning `Agent.agent_id` from the parent row; it is
/// propagated onto the resolved slot so the V2D memory recorder can
/// scope recall/write to `agent:<agent_id>`. Pass an empty string when
/// no agent is associated (legacy tests).
pub fn resolve_agent_slot(role: &str, slot: &AgentSlot, agent_id: &str) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.to_string(),
        slot: agent_slot_to_llm_slot(role, slot),
        max_tokens: slot.resolve_max_tokens(),
        temperature: slot.temperature,
        inputs_policy: slot.inputs_policy,
        bar_history_limit: slot.bar_history_limit,
        memory_mode: slot.memory_mode,
        agent_id: agent_id.to_string(),
        // `None` and `Some(true)` both enable the skip; `Some(false)`
        // explicitly disables it so the LLM runs even in a corner.
        noop_skip: slot.noop_skip.unwrap_or(true),
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
            attested_with: model.to_string(),
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
        slot.attested_with = "".into();
        assert_eq!(default_max_tokens_for(&slot), Some(4096));
    }
}
