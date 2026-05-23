use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::agent::dispatch_capability::{dispatch_capability, resolve_activates, AgentOutput, DispatchInput};
use crate::agent::edge_predicate::evaluate_predicate;
use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, ResponseSchema, StopReason};
use crate::agent::observability::ObsEmitter;
use crate::agents::{default_capabilities, AgentSlot, Capability, InputsPolicy};
use crate::strategies::agent_ref::canonical_role;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineKind, Strategy};
use crate::tools::ToolRegistry;
use std::collections::BTreeSet;
use xvision_core::providers::{lookup_model, Catalog};

#[derive(Debug, Clone)]
pub struct ResolvedAgentSlot {
    pub role: String,
    pub slot: LLMSlot,
    /// The system prompt for this slot, snapshotted from the bound
    /// agent's `AgentSlot.system_prompt`. The agent-side field is the
    /// single source of truth for prompt text; `LLMSlot` no longer
    /// carries one.
    pub system_prompt: String,
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
    /// Snapshotted `AgentSlot.capabilities` (Phase A) at
    /// strategy-resolution time. Read by `resolve_activates` in the
    /// Phase B dispatcher to pick the slot's primary capability when
    /// `AgentRef.activates` is `None`. Phase E populates non-Trader
    /// sets on starter templates; without this field the dispatcher
    /// would mis-route those slots to the Trader handler.
    ///
    /// Empty set falls back to `{Trader}` in `resolve_activates` so
    /// legacy pre-033 strategies (whose `serde(default)` for
    /// `AgentSlot.capabilities` is `{Trader}`) keep byte-identical
    /// behaviour.
    pub capabilities: BTreeSet<Capability>,
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
    /// executors opt in via `Executor::with_observability_bus`.
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
    /// Provider catalogs loaded for this eval run. Used by the
    /// context-overflow recovery path to select the cheapest model for
    /// history summarization. Empty map preserves the legacy no-recovery
    /// behavior for tests and non-eval callers.
    pub provider_catalogs: HashMap<String, Arc<Catalog>>,
    /// Phase C — optional Filter-capability runtime context. Carries
    /// the per-eval-run signal cache, the bar period (for the
    /// `granularity_fallback` event), the multi-Filter cardinality
    /// config, and the current bar timestamp. `None` preserves the
    /// legacy "no cache / single-fire / no fallback" behavior — every
    /// existing call site inherits it without code changes. The eval
    /// executors opt in via `with_filter_ctx`.
    pub filter_ctx: Option<FilterPipelineCtx<'a>>,
    /// Phase D — unified `Recorder` threaded from the entry point
    /// (harness path constructs a `HarnessRecorder`; eval-executor path
    /// constructs an `EvalRecorder`). Each capability handler in
    /// `dispatch_capability` emits row-typed writes via this trait so
    /// the harness + eval surfaces produce symmetric recorder rows
    /// (F-11(f) closure). `None` is the back-compat default — every
    /// existing call site keeps working until its entry point is
    /// migrated to construct one of the two implementors.
    pub recorder: Option<&'a (dyn xvision_observability::Recorder + 'a)>,
}

/// Phase C — runtime context owned by the executor for the duration
/// of one eval run and threaded into each per-cycle `run_pipeline`
/// invocation. See [`PipelineInputs::filter_ctx`].
pub struct FilterPipelineCtx<'a> {
    /// Mutable per-run signal cache. Lifetime equals the executor's
    /// run loop; dropped when the run completes.
    pub signal_cache: &'a mut crate::agent::signal_cache::SignalCache,
    /// Bar period of the current scenario / live feed, in minutes.
    /// Drives the `granularity_fallback` event and the multi-Filter
    /// cardinality threshold.
    pub bar_period_minutes: u32,
    /// Multi-Filter cardinality config. Built once at executor startup
    /// (default: threshold 30 minutes — operator Q3 resolution
    /// 2026-05-22).
    pub multi_filter_config: crate::agent::filter_dispatch::MultiFilterConfig,
    /// Current bar timestamp. Used to populate `FilterSignal.ts` and
    /// to drive Minute-granularity freshness comparisons.
    pub bar_ts: chrono::DateTime<chrono::Utc>,
    /// Canonicalised strategy id used as the first component of the
    /// `SignalCacheKey`. Caller typically passes
    /// `strategy.manifest.id.clone()`.
    pub strategy_id: String,
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
            system_prompt: String::new(),
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
            catalog: catalog_for_slot(slot, &input.provider_catalogs),
            delta_briefing: false,
            prev_briefing: None,
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
            system_prompt: String::new(),
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
            catalog: catalog_for_slot(slot, &input.provider_catalogs),
            delta_briefing: false,
            prev_briefing: None,
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
            system_prompt: String::new(),
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
            catalog: catalog_for_slot(slot, &input.provider_catalogs),
            delta_briefing: false,
            prev_briefing: None,
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

fn graph_skip_response() -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: r#"{"action":"hold","conviction":0.0,"justification":"trader_skipped_by_graph: all Trader agents were gated out by graph edge predicates"}"#.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    }
}

async fn run_agent_pipeline<'a>(mut input: PipelineInputs<'a>) -> anyhow::Result<PipelineOutputs> {
    let mut accumulated = input.seed_inputs.clone();
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    let mut regime = None;
    let mut intern = None;
    let mut trader = None;

    // `indicator-tool-wiring` (2026-05-22): see the long comment kept
    // here for historical context — strategy-level required_tools must
    // bridge into each per-iteration `LLMSlot.allowed_tools` because
    // `AgentSlot` itself has no tool list yet. We continue to apply
    // this fallback to every dispatch under the Phase B seam.
    let strategy_tools: Vec<String> = input.strategy.manifest.required_tools.clone();

    // Phase B: track the previous capability output so Router can
    // redirect the next dispatch. Graph edge predicates read from
    // `outputs_by_role` instead because they gate targets by incoming
    // edge, not by the immediately previous agent.
    let mut prev_output: Option<AgentOutput> = None;
    let _ = &prev_output;
    let mut outputs_by_role: BTreeMap<String, AgentOutput> = BTreeMap::new();

    // Phase C — per-cycle ordered map of `(role_key, FilterSignal)` for
    // every Filter that emitted on this cycle. Threaded into the
    // briefing under `filter_signals` so downstream Traders read a
    // stable shape regardless of multi-fire mode.
    let mut filter_signals: std::collections::BTreeMap<
        String,
        crate::agent::dispatch_capability::FilterSignal,
    > = std::collections::BTreeMap::new();
    // Emission order — used by the multi-fire path to invoke the
    // Trader once per emitting Filter in strategy-declaration order.
    let mut filter_emit_order: Vec<String> = Vec::new();

    // Index-driven loop so Router's `RouteSelection.target_agent_ref_index`
    // can jump forward, and so DAG-strict acceptance of `target > current`
    // is enforced at runtime. Sequential pipelines never set `activates =
    // Router` so the loop walks 0..n exactly as it did pre-Phase-B.
    let n = input.agent_slots.len();
    let mut i: usize = 0;
    while i < n {
        let resolved = &input.agent_slots[i];

        // Single canonical comparison key (trim + lowercase) so the
        // trader-output schema selection and the output-assignment
        // match arm never disagree (QA #5).
        let role_key = canonical_role(&resolved.role);

        // Resolve which capability this `AgentRef` activates. Spec
        // Decision 1: prefer `AgentRef.activates`; fall back to the
        // slot's first capability in `BTreeSet` iteration order. Phase E
        // ships starter templates with non-Trader capabilities (Critic /
        // Filter / Intern / Router) so the fallback can be load-bearing
        // — we read the snapshotted `resolved.capabilities` set (Phase A
        // field, threaded by `resolve_agent_slot`). Empty sets fall back
        // to `{Trader}` inside `resolve_activates`, which keeps every
        // legacy pre-033 strategy on its previous dispatch path.
        let activates_field = input.strategy.agents.get(i).and_then(|a| a.activates);
        let capabilities_for_fallback = if resolved.capabilities.is_empty() {
            default_capabilities()
        } else {
            resolved.capabilities.clone()
        };
        let capability = resolve_activates(activates_field, &capabilities_for_fallback);

        // Graph pipelines gate an agent on its incoming conditioned
        // edges. A missing or non-matching upstream FilterSignal skips
        // the target for this cycle instead of silently falling through
        // to the default sequential order.
        if graph_agent_is_gated_out(input.strategy, &outputs_by_role, i) {
            tracing::debug!(
                event = "graph_agent_gated_out",
                role = %resolved.role,
                "graph edge predicate skipped agent dispatch",
            );
            if let Some(obs) = input.obs.as_ref() {
                let payload = serde_json::json!({
                    "role": resolved.role,
                    "cycle_idx": input.cycle_idx,
                    "reason": "incoming graph edge predicate evaluated false",
                });
                obs.emit_engine_event("graph_agent_gated_out", None, Some(payload.to_string()))
                    .await;
            }
            i += 1;
            continue;
        }

        // trader-noop-skip: only fires on Trader-capable agents AND
        // when the seed has zero legal open actions. Keeping the gate
        // here (above `dispatch_capability`) so the synthesized
        // `noop_skip` LlmResponse is byte-identical to the pre-Phase-B
        // path — the dispatch seam never sees the skipped call.
        if capability == Capability::Trader && resolved.noop_skip && seed_has_zero_legal_opens(&accumulated) {
            tracing::debug!(
                event = "noop_skip",
                role = %resolved.role,
                "trader-noop-skip: portfolio already carries a position — skipping LLM call",
            );
            if let Some(obs) = input.obs.as_ref() {
                let payload = serde_json::json!({
                    "role": resolved.role,
                    "cycle_idx": input.cycle_idx,
                    "reason": "noop_skip: portfolio already carries a position",
                });
                obs.emit_engine_event("flat_skip_fired", None, Some(payload.to_string()))
                    .await;
                obs.emit_supervisor_note(
                    "guard",
                    "info",
                    &format!(
                        "trader-noop-skip fired at cycle {} — portfolio already \
                         carries a position; the LLM call was skipped and a \
                         hold decision was synthesized",
                        input.cycle_idx
                    ),
                )
                .await;
            }
            let skip_out = noop_skip_response();
            accumulated[format!("{role_key}_output")] = serde_json::Value::String(skip_out.text());
            trader = Some(skip_out.clone());
            let skip_output =
                AgentOutput::Trader(crate::agent::dispatch_capability::TraderDecision { response: skip_out });
            outputs_by_role.insert(role_key.clone(), skip_output.clone());
            prev_output = Some(skip_output);
            i = next_index(&prev_output, i);
            continue;
        }

        // `indicator-tool-wiring`: stamp the strategy's tool surface
        // onto a per-iteration clone of the resolved slot when the slot
        // itself carries no explicit tool list.
        let mut slot_for_exec = resolved.slot.clone();
        if slot_for_exec.allowed_tools.is_empty() && !strategy_tools.is_empty() {
            slot_for_exec.allowed_tools = strategy_tools.clone();
        }

        // Phase C — Filter cache lookup. If the cache has a fresh
        // signal for this `(strategy, role)`, the dispatcher's LLM
        // call is replaced with a re-fire of the cached payload — no
        // tokens charged, no provider hit. Cache-hit / cache-miss
        // policy depends on the Filter's declared granularity:
        //
        // * `Bar`    — always re-evaluate (no cache lookup).
        // * `Minute` — re-fire when `truncate_to_minute(now) <=
        //              cached_ts.truncate_to_minute()`.
        // * `Decision` — re-fire when no Trader is reachable in
        //              `agents[i+1..]`; otherwise re-evaluate.
        //
        // When the runtime degrades a Minute-granularity Filter on a
        // multi-minute bar to `Bar`, the cache lookup is skipped and
        // we emit `granularity_fallback`.
        let mut cached_outcome: Option<AgentOutput> = None;
        if capability == Capability::Filter {
            // To pick the cache decision we need the prior cached
            // signal (granularity comes from that signal). If no prior
            // signal exists, we must evaluate.
            if let Some(filter_ctx) = input.filter_ctx.as_ref() {
                let key = crate::agent::signal_cache::SignalCacheKey::new(
                    filter_ctx.strategy_id.clone(),
                    role_key.clone(),
                );
                if let Some(cached) = filter_ctx.signal_cache.get(&key) {
                    let reuse = match cached.signal.granularity {
                        crate::agent::dispatch_capability::FilterGranularity::Bar => false,
                        crate::agent::dispatch_capability::FilterGranularity::Minute => {
                            if filter_ctx.bar_period_minutes > 1 {
                                // Granularity fallback — emit once
                                // per cache lookup so the trace
                                // records the demotion.
                                crate::agent::filter_dispatch::emit_granularity_fallback(
                                    input.obs.as_ref(),
                                    &role_key,
                                    filter_ctx.bar_period_minutes,
                                )
                                .await;
                                false
                            } else {
                                crate::agent::signal_cache::minute_cache_is_fresh(
                                    cached.last_evaluated_ts,
                                    filter_ctx.bar_ts,
                                )
                            }
                        }
                        crate::agent::dispatch_capability::FilterGranularity::Decision => {
                            !trader_reachable_after(input.strategy, input.agent_slots, i)
                        }
                    };
                    if reuse {
                        cached_outcome = Some(AgentOutput::Filter(cached.signal.clone()));
                    }
                }
            }
        }

        let was_cache_hit = cached_outcome.is_some();
        let outcome = if let Some(cached) = cached_outcome {
            crate::agent::dispatch_capability::DispatchOutcome {
                output: cached,
                input_tokens: 0,
                output_tokens: 0,
                raw_response: None,
            }
        } else {
            dispatch_capability(DispatchInput {
                resolved,
                slot: &slot_for_exec,
                system_prompt: resolved.system_prompt.clone(),
                upstream_inputs: accumulated.clone(),
                dispatch: input.dispatch.clone(),
                tools: input.tools.clone(),
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
                catalog: catalog_for_slot(&resolved.slot, &input.provider_catalogs),
                delta_briefing: false,
                prev_briefing: None,
                current_index: i,
                total_agents: n,
                activates: capability,
                recorder: input.recorder,
            })
            .await?
        };

        total_in += outcome.input_tokens;
        total_out += outcome.output_tokens;

        // Phase C — if this was a Filter, stash the signal in the
        // per-cycle ordered map AND in the per-run cache so
        // downstream Traders + subsequent cycles see it. Stash
        // happens whether the signal was re-evaluated or re-fired
        // from cache; the cache write is idempotent on re-fire (same
        // payload, same ts).
        if let AgentOutput::Filter(ref signal) = outcome.output {
            // The signal's `ts` is set by the dispatcher to either
            // `scenario_start` or `Utc::now()` (LLM Filter path), or
            // to the cached `ts` (re-fire path). Override here with
            // the executor's `bar_ts` when we have one — keeps the
            // cache keyed to the bar that produced the signal so
            // Minute-granularity comparisons make sense.
            let mut signal_for_cache = signal.clone();
            if let Some(filter_ctx) = input.filter_ctx.as_ref() {
                // Only update ts on the fresh-evaluation path —
                // re-fires already carry the cached ts and we don't
                // want to lie about freshness. The Filter path
                // returns `raw_response: None` even on a fresh LLM
                // call (the dispatcher wraps the LlmResponse into a
                // typed `FilterSignal` and drops the raw), so we use
                // the explicit `was_cache_hit` flag instead.
                if !was_cache_hit {
                    signal_for_cache.ts = filter_ctx.bar_ts;
                }
            }
            if !filter_signals.contains_key(&role_key) {
                filter_emit_order.push(role_key.clone());
            }
            filter_signals.insert(role_key.clone(), signal_for_cache.clone());

            if let Some(filter_ctx) = input.filter_ctx.as_mut() {
                let key = crate::agent::signal_cache::SignalCacheKey::new(
                    filter_ctx.strategy_id.clone(),
                    role_key.clone(),
                );
                filter_ctx.signal_cache.insert(key, signal_for_cache);
            }

            // Materialise into `filter_signals[role]` on the briefing.
            // Downstream Traders read this stable shape regardless of
            // whether one or many Filters fired this cycle.
            let entry = accumulated.as_object_mut().and_then(|m| {
                m.entry("filter_signals")
                    .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
                    .as_object_mut()
            });
            if let Some(map) = entry {
                map.insert(
                    role_key.clone(),
                    serde_json::to_value(signal).unwrap_or(serde_json::Value::Null),
                );
            }
        }

        // Materialise the role's text output into the accumulated
        // briefing JSON. For Trader / Router (real LLM calls), use the
        // raw response text. For stub capabilities (Filter / Critic /
        // Intern), serialize the typed output so downstream agents see
        // a predictable shape.
        let text_for_briefing = match outcome.raw_response.as_ref() {
            Some(r) => r.text(),
            None => match &outcome.output {
                AgentOutput::Filter(s) => serde_json::to_string(s).unwrap_or_default(),
                AgentOutput::Critic(c) => serde_json::to_string(c).unwrap_or_default(),
                AgentOutput::Intern(o) => serde_json::to_string(o).unwrap_or_default(),
                _ => String::new(),
            },
        };
        accumulated[format!("{role_key}_output")] = serde_json::Value::String(text_for_briefing);

        // Phase C — Multi-Filter cardinality. The Trader we just ran
        // already saw `filter_signals` in its briefing. If we're in
        // multi-fire mode (`bar_period_minutes >= threshold`) AND the
        // cycle produced 2+ Filter signals, we invoke the Trader once
        // more per remaining Filter — each invocation sees a
        // single-signal `filter_signals` map containing only that
        // Filter. The recorded `trader` is the LAST invocation's
        // output (matches Phase B's "last AgentRef of `activates:
        // Trader` wins" rule).
        if capability == Capability::Trader && filter_signals.len() >= 2 {
            let multi_fire = input
                .filter_ctx
                .as_ref()
                .map(|c| c.multi_filter_config.should_multi_fire(c.bar_period_minutes))
                .unwrap_or(false);
            if multi_fire {
                // The first invocation already saw all signals merged
                // (above). For multi-fire we want each invocation to
                // see ONLY one signal, so re-run the Trader once per
                // emitting Filter in emission order. Last invocation
                // wins — overwrites `trader`.
                let mut last_response: Option<LlmResponse> = outcome.raw_response.clone();
                for role in &filter_emit_order {
                    // Per-Filter briefing: replace `filter_signals`
                    // with a one-key map.
                    let mut briefing = accumulated.clone();
                    if let Some(map) = briefing.as_object_mut() {
                        if let Some(sig) = filter_signals.get(role) {
                            let mut single = serde_json::Map::with_capacity(1);
                            single.insert(
                                role.clone(),
                                serde_json::to_value(sig).unwrap_or(serde_json::Value::Null),
                            );
                            map.insert("filter_signals".to_string(), serde_json::Value::Object(single));
                        }
                    }
                    let outcome2 = dispatch_capability(DispatchInput {
                        resolved,
                        slot: &slot_for_exec,
                        system_prompt: resolved.system_prompt.clone(),
                        upstream_inputs: briefing,
                        dispatch: input.dispatch.clone(),
                        tools: input.tools.clone(),
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
                        catalog: catalog_for_slot(&resolved.slot, &input.provider_catalogs),
                        delta_briefing: false,
                        prev_briefing: None,
                        current_index: i,
                        total_agents: n,
                        activates: capability,
                        recorder: input.recorder,
                    })
                    .await?;
                    total_in += outcome2.input_tokens;
                    total_out += outcome2.output_tokens;
                    last_response = outcome2.raw_response.clone();
                }
                if let Some(raw) = last_response.clone() {
                    accumulated[format!("{role_key}_output")] = serde_json::Value::String(raw.text());
                    trader = Some(raw);
                }
            }
        }

        // Legacy harness shape: surface regime / intern / trader by
        // role name into the `PipelineOutputs` struct for back-compat.
        // Future Phase D refactor will replace the named slots with a
        // typed `Vec<AgentOutput>`, but Phase B keeps the shape stable.
        if let Some(raw) = outcome.raw_response.clone() {
            match role_key.as_str() {
                "regime" => regime = Some(raw),
                "intern" => intern = Some(raw),
                // For Trader, only set if the multi-fire branch above
                // didn't already overwrite `trader` with the
                // last-invocation response.
                "trader" if trader.is_none() => trader = Some(raw),
                _ => {}
            }
        }

        outputs_by_role.insert(role_key.clone(), outcome.output.clone());
        prev_output = Some(outcome.output);

        // Decide which index to visit next: Router output jumps
        // directly; otherwise fall through to `i + 1`. Graph edge
        // predicates are target gates evaluated before dispatch.
        i = next_index(&prev_output, i);
    }

    if input.strategy.pipeline.kind == PipelineKind::Graph && trader.is_none() {
        trader = Some(graph_skip_response());
    }

    Ok(PipelineOutputs {
        regime,
        intern,
        trader,
        total_input_tokens: total_in,
        total_output_tokens: total_out,
    })
}

/// Pick the next `Strategy.agents` index to dispatch.
///
/// Router output may jump forward; every other capability walks to the
/// next strategy-order agent. `PipelineKind::Graph` predicates are
/// evaluated as incoming target gates in `graph_agent_is_gated_out`.
fn next_index(prev_output: &Option<AgentOutput>, current_index: usize) -> usize {
    if let Some(AgentOutput::Router(sel)) = prev_output.as_ref() {
        return sel.target_agent_ref_index;
    }

    current_index + 1
}

fn graph_agent_is_gated_out(
    strategy: &Strategy,
    outputs_by_role: &BTreeMap<String, AgentOutput>,
    current_index: usize,
) -> bool {
    if strategy.pipeline.kind != PipelineKind::Graph {
        return false;
    }
    let Some(agent) = strategy.agents.get(current_index) else {
        return false;
    };
    let role = canonical_role(&agent.role);
    for edge in &strategy.pipeline.edges {
        if canonical_role(&edge.to_role) != role {
            continue;
        }
        let Some(predicate) = edge.condition.as_ref() else {
            continue;
        };
        let from_role = canonical_role(&edge.from_role);
        let Some(upstream) = outputs_by_role.get(&from_role) else {
            return true;
        };
        if !evaluate_predicate(predicate, upstream) {
            return true;
        }
    }
    false
}

/// Phase C — graph-topology check for `Decision`-granularity Filter
/// re-evaluation. Returns `true` when any `AgentRef` at index
/// `> current_index` activates [`Capability::Trader`] — meaning the
/// downstream of this Filter has a Trader the runtime is about to
/// invoke, so the Decision-cadence cache MUST be refreshed.
///
/// We accept slight false-positives here (e.g. a Trader that the
/// Router will skip over). The conservative call is to re-evaluate
/// when in doubt — a stale Decision-granularity signal feeding a
/// Trader is the failure mode the contract is built to prevent.
fn trader_reachable_after(
    strategy: &Strategy,
    _agent_slots: &[ResolvedAgentSlot],
    current_index: usize,
) -> bool {
    strategy
        .agents
        .iter()
        .enumerate()
        .skip(current_index + 1)
        .any(|(_, a)| {
            // Resolve capability: explicit `activates`, falling back
            // to the default-capability heuristic. Matches the main
            // loop's resolution so cache freshness aligns with what
            // the loop is about to dispatch.
            let cap = resolve_activates(a.activates, &default_capabilities());
            cap == Capability::Trader
        })
}

fn catalog_for_slot(slot: &LLMSlot, catalogs: &HashMap<String, Arc<Catalog>>) -> Option<Arc<Catalog>> {
    let provider = slot.provider.as_deref()?.trim();
    if provider.is_empty() {
        None
    } else {
        catalogs.get(provider).cloned()
    }
}

pub fn agent_slot_to_llm_slot(role: &str, slot: &AgentSlot) -> LLMSlot {
    LLMSlot {
        role: role.to_string(),
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
        system_prompt: slot.system_prompt.clone(),
        max_tokens: slot.resolve_max_tokens(),
        temperature: slot.temperature,
        inputs_policy: slot.inputs_policy,
        bar_history_limit: slot.bar_history_limit,
        memory_mode: slot.memory_mode,
        agent_id: agent_id.to_string(),
        // Phase A field — snapshot the slot's full capabilities set so
        // the Phase B dispatcher's `resolve_activates` picks the right
        // primary capability when `AgentRef.activates` is `None`. Phase E
        // ships templates with non-Trader sets (Critic / Filter / Intern
        // / Router); without this snapshot the dispatcher would default
        // every such slot to Trader.
        capabilities: slot.capabilities.clone(),
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
