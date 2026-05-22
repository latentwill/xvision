//! Phase B — unified capability-typed dispatch seam.
//!
//! Single entry point [`dispatch_capability`] replaces the role-string
//! switch that used to live in [`crate::agent::pipeline::run_agent_pipeline`].
//! Each [`crate::agents::Capability`] variant routes to a typed handler
//! that returns an [`AgentOutput`]:
//!
//! * `Trader`  → unchanged LLM call wrapped as `AgentOutput::Trader`. The
//!   inputs are byte-identical to the pre-Phase-B path so existing eval
//!   runs (and the A/B cache pairing keyed on `(cycle_id, scenario_id)`)
//!   keep producing the same outputs.
//! * `Filter`  → stub handler returning a placeholder [`FilterSignal`].
//!   Phase C wires the real Filter LLM call + predicate-payload schema.
//! * `Critic`  → stub handler returning a placeholder [`Critique`].
//!   Phase D wires the real Critic semantics.
//! * `Intern`  → stub handler returning a placeholder [`InternObservation`].
//!   Phase D wires the real Intern semantics.
//! * `Router`  → fully implemented in v1 per operator Decision 2. Runs
//!   the slot's LLM with a JSON response schema enforcing the
//!   `{ "target_agent_ref_index": <usize> }` shape, then validates the
//!   index against the current position in the pipeline (must be strictly
//!   greater than the current index — DAG-strict, no cycles per spec
//!   Decision 8).
//!
//! The seam is intentionally narrow: callers pass a small typed input
//! and receive an `AgentOutput`. The pipeline owns the iteration logic
//! (which agent to call next, edge-predicate evaluation, signal merge)
//! so the dispatch seam stays oblivious to graph topology.
//!
//! See `team/contracts/agent-graph-capability-dispatch.md` and
//! `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{LlmDispatch, LlmResponse, ResponseSchema};
use crate::agent::memory_recorder::MemoryRecorder;
use crate::agent::observability::ObsEmitter;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::agents::Capability;
use crate::strategies::slot::LLMSlot;
use crate::tools::ToolRegistry;
use xvision_core::providers::Catalog;
use xvision_observability::Recorder;

/// Trader's typed decision output. Phase B wraps the existing `LlmResponse`
/// so the pre-Phase-B trader path is byte-identical — the trader still
/// returns a full `LlmResponse` to the eval executor; the eval executor's
/// downstream `TraderOutput::parse_response` keeps parsing it.
///
/// The wrapper exists so the typed `AgentOutput` sum can carry a Trader
/// variant without erasing the underlying response. Phase D's recorder
/// unification will refine this further.
#[derive(Debug, Clone)]
pub struct TraderDecision {
    /// Raw LLM response. Eval executors call `TraderOutput::parse_response`
    /// to turn this into a structured `TraderOutput`.
    pub response: LlmResponse,
}

/// Phase B stub shape for a Filter signal. The real Filter LLM call and
/// per-Filter granularity cache land in Phase C. `payload` is the JSON
/// blob downstream `EdgePredicate` evaluators read via `signal_field`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSignal {
    /// Producer's `AgentRef.role` (the briefing surface uses this as the
    /// key under `filter_signals[name]`).
    pub name: String,
    /// Predicate payload. Phase B stubs emit `Value::Null`; Phase C will
    /// populate this with the Filter's structured output.
    pub payload: serde_json::Value,
    /// Granularity at which this signal was produced. Phase B stubs
    /// always emit `Bar`.
    pub granularity: FilterGranularity,
    /// Bar timestamp the signal was computed on. Used for stale-signal
    /// detection in Phase C's per-granularity cache.
    pub ts: DateTime<Utc>,
}

/// Phase A's spec defines three granularities. Phase B carries the
/// type so the `FilterSignal` shape is stable; Phase C wires the
/// runtime cache that respects each granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FilterGranularity {
    /// Re-evaluate on every new bar from the scenario / live feed.
    #[default]
    Bar,
    /// Re-evaluate on a fixed 1-minute tick (bar-aligned by truncation).
    Minute,
    /// Re-evaluate only when a downstream Trader is about to be invoked.
    Decision,
}

/// Critic verdict — Phase D wires the actual model call. Phase B stubs
/// emit `Info` severity with placeholder text so downstream consumers
/// don't crash on the stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CritiqueSeverity {
    Info,
    Warning,
    Reject,
}

/// Phase B stub for a Critic's output. Phase D replaces the body with
/// the real verdict (Approve / Reject / SuggestModification) plus a
/// structured rationale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Critique {
    pub severity: CritiqueSeverity,
    pub text: String,
}

/// Phase B stub for an Intern's structured observation. Phase D replaces
/// the body with the real free-form JSON note merged into the trader's
/// briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternObservation {
    pub text: String,
}

/// Router's typed output. Phase B ships this fully — the dispatcher
/// validates `target_agent_ref_index > current_index` AND
/// `target_agent_ref_index < agents.len()` at runtime. The strategy
/// validator additionally rejects backward targets at draft time so
/// malformed graphs fail before eval-launch (spec Decision 8 — DAG-strict).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSelection {
    /// Index into `Strategy.agents` of the next `AgentRef` to invoke.
    /// Must be strictly greater than the Router's own index — backward
    /// targets are rejected as cycle introductions.
    pub target_agent_ref_index: usize,
}

/// Typed sum of every capability-handler return value. Phase B wires
/// all five variants; only Trader has its real shape — the other four
/// are stub-shaped per the contract and gain semantics in Phases C–E.
#[derive(Debug, Clone)]
pub enum AgentOutput {
    Trader(TraderDecision),
    Filter(FilterSignal),
    Critic(Critique),
    Intern(InternObservation),
    Router(RouteSelection),
}

impl AgentOutput {
    /// Convenience: extract a reference to the inner `FilterSignal` for
    /// edge-predicate evaluation. Returns `None` for any non-Filter
    /// output (the predicate evaluator treats this as "predicate fails"
    /// — a Critic / Trader output never satisfies a `FilterSignal`
    /// predicate).
    pub fn as_filter_signal(&self) -> Option<&FilterSignal> {
        match self {
            AgentOutput::Filter(s) => Some(s),
            _ => None,
        }
    }
}

/// Inputs to a single `dispatch_capability` invocation. Carries
/// everything the inner Trader path's `SlotInput` needs plus the
/// capability-specific context (current index, total agent count) the
/// Router needs to bound its output.
pub struct DispatchInput<'a> {
    pub resolved: &'a ResolvedAgentSlot,
    /// The slot to dispatch with — usually `resolved.slot` but the
    /// pipeline may stamp in strategy-level `allowed_tools` first
    /// (`indicator-tool-wiring`). Borrowed so the caller controls the
    /// lifetime of any per-iteration clone.
    pub slot: &'a LLMSlot,
    pub system_prompt: String,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub obs: Option<ObsEmitter>,
    pub memory: Option<Arc<MemoryRecorder>>,
    pub memory_mode: xvision_memory::types::MemoryMode,
    pub agent_id: String,
    pub scenario_start: Option<DateTime<Utc>>,
    pub run_id: String,
    pub scenario_id: String,
    pub cycle_idx: i64,
    pub catalog: Option<Arc<Catalog>>,
    pub delta_briefing: bool,
    pub prev_briefing: Option<serde_json::Value>,
    /// Position of this agent in `Strategy.agents`. Router uses this to
    /// bound `target_agent_ref_index > current_index`.
    pub current_index: usize,
    /// `Strategy.agents.len()`. Router uses this to bound
    /// `target_agent_ref_index < total_agents`.
    pub total_agents: usize,
    /// The capability this dispatch invocation should activate, resolved
    /// once by the pipeline per spec Decision 1 (explicit `activates` or
    /// the first capability in the slot's `BTreeSet`).
    pub activates: Capability,
    /// Phase D — unified recorder threaded from the pipeline / eval
    /// executor entry point. Each capability handler emits through this
    /// trait, never directly to `ObsEmitter` or a trace buffer. The
    /// harness path constructs a `HarnessRecorder`; the eval-executor
    /// path constructs an `EvalRecorder`; both implement `&dyn Recorder`
    /// so the dispatcher stays oblivious to which surface it's on.
    ///
    /// `None` is the back-compat default — existing call sites that
    /// haven't been migrated yet inherit a `NullRecorder`-shaped no-op
    /// without code changes. Phase D's pipeline + executor wiring sets
    /// this explicitly.
    pub recorder: Option<&'a dyn Recorder>,
}

/// Result of `dispatch_capability`: the typed `AgentOutput` AND the
/// accumulated input/output token counts from the underlying LLM call(s).
/// Stub handlers report `(0, 0)`; Trader and Router report whatever the
/// dispatcher returned.
#[derive(Debug)]
pub struct DispatchOutcome {
    pub output: AgentOutput,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// The raw `LlmResponse` from `execute_slot`, when a real LLM call
    /// happened. Trader and Router carry this; stub handlers (Filter /
    /// Critic / Intern in Phase B) carry `None`.
    ///
    /// The pipeline reads this for two reasons: (1) eval executors still
    /// inspect the raw trader response, (2) the legacy
    /// `PipelineOutputs { regime, intern, trader }` shape needs the
    /// trader's `LlmResponse` until the post-v1 cleanup deletes those
    /// fields.
    pub raw_response: Option<LlmResponse>,
}

/// Phase B unified dispatch seam. Routes a single `AgentRef`'s invocation
/// to the right capability handler.
///
/// Trader is byte-identical to the pre-Phase-B path: same `SlotInput`,
/// same `execute_slot` call, same `ResponseSchema::trader_output()`.
/// The A/B-cache key `(cycle_id, scenario_id)` flows through unchanged
/// — see the fixture test `dispatches_trader_with_unchanged_cycle_id`.
pub async fn dispatch_capability(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    match input.capability_to_dispatch() {
        Capability::Trader => dispatch_trader(input).await,
        Capability::Filter => dispatch_filter(input).await,
        Capability::Critic => Ok(dispatch_critic_stub()),
        Capability::Intern => Ok(dispatch_intern_stub()),
        Capability::Router => dispatch_router(input).await,
    }
}

/// Phase C — Filter dispatcher. DSL-backed slots (slot `provider ==
/// "dsl"`) route through the `xvision-filters` bridge for a thin
/// `RuntimeFilter → FilterSignal` adapter; everything else runs the
/// LLM Filter dispatcher in `filter_dispatch::run_llm_filter`.
///
/// Edge predicates work identically on both — they read
/// `FilterSignal.payload` regardless of the producer.
///
/// On LLM parse failure, the dispatcher records the
/// `filter_parse_error` event (inside `filter_dispatch`) and returns a
/// `FilterSignal` whose `payload` is `Value::Null`. Downstream edge
/// predicates that depend on a missing field then evaluate to `false`
/// per the contract — edges do not fire, the cycle falls through.
async fn dispatch_filter(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    let role = input.resolved.role.clone();
    if slot_is_dsl(input.slot) {
        // The DSL bridge runs synchronously — no LLM call, no token
        // accounting. The eval executor populates the bridge via
        // `xvision_filters::runtime::dsl_to_filter_signal` ahead of
        // the dispatch loop (Phase C). Reaching this branch without an
        // upstream-provided signal is a programmer error: we surface
        // it as a parse-error-shaped null signal so the trace records
        // the drift rather than silently producing a bogus payload.
        let signal = FilterSignal {
            name: role,
            payload: serde_json::Value::Null,
            granularity: FilterGranularity::Bar,
            ts: input.scenario_start.unwrap_or_else(Utc::now),
        };
        return Ok(DispatchOutcome {
            output: AgentOutput::Filter(signal),
            input_tokens: 0,
            output_tokens: 0,
            raw_response: None,
        });
    }

    match crate::agent::filter_dispatch::run_llm_filter(input).await {
        Ok(signal) => Ok(DispatchOutcome {
            output: AgentOutput::Filter(signal),
            input_tokens: 0,
            output_tokens: 0,
            raw_response: None,
        }),
        Err(_) => {
            // Parse error already emitted as `filter_parse_error`
            // (filter_dispatch::run_llm_filter). Surface a `null`
            // payload so the pipeline can keep walking — predicates
            // resolve to `false` per the edge-predicate "unknown
            // field" rule (see `agent::edge_predicate`).
            Ok(DispatchOutcome {
                output: AgentOutput::Filter(FilterSignal {
                    name: role,
                    payload: serde_json::Value::Null,
                    granularity: FilterGranularity::Bar,
                    ts: Utc::now(),
                }),
                input_tokens: 0,
                output_tokens: 0,
                raw_response: None,
            })
        }
    }
}

/// DSL-backed slot detection. The marker is `slot.provider == "dsl"`
/// (case-insensitive) — same convention the rest of the engine uses
/// for the existing `xvision-filters` DSL substrate. Other markers
/// are reserved for future provider-style integrations.
fn slot_is_dsl(slot: &LLMSlot) -> bool {
    slot.provider
        .as_deref()
        .map(|p| p.trim().eq_ignore_ascii_case("dsl"))
        .unwrap_or(false)
}

impl DispatchInput<'_> {
    /// Resolved capability for this dispatch invocation. The pipeline
    /// computed it once via [`resolve_activates`] before building the
    /// input, so the dispatcher's match arm is a single field read.
    fn capability_to_dispatch(&self) -> Capability {
        self.activates
    }
}

/// Trader handler — byte-identical to the pre-Phase-B path. The
/// surrounding pipeline still handles the `noop_skip` short-circuit
/// before reaching this seam, so the LLM call below is unconditional.
async fn dispatch_trader(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    let resp = execute_slot(SlotInput {
        slot: input.slot,
        system_prompt: input.system_prompt,
        upstream_inputs: input.upstream_inputs,
        dispatch: input.dispatch,
        tools: input.tools,
        response_schema: Some(ResponseSchema::trader_output()),
        max_tokens: input.max_tokens,
        temperature: input.temperature,
        obs: input.obs,
        memory: input.memory,
        memory_mode: input.memory_mode,
        agent_id: input.agent_id,
        scenario_start: input.scenario_start,
        run_id: input.run_id,
        scenario_id: input.scenario_id,
        cycle_idx: input.cycle_idx,
        catalog: input.catalog,
        delta_briefing: input.delta_briefing,
        prev_briefing: input.prev_briefing,
    })
    .await?;

    let input_tokens = resp.input_tokens;
    let output_tokens = resp.output_tokens;
    Ok(DispatchOutcome {
        output: AgentOutput::Trader(TraderDecision {
            response: resp.clone(),
        }),
        input_tokens,
        output_tokens,
        raw_response: Some(resp),
    })
}

/// Phase B Critic stub. Phase D wires the real verdict + rationale.
fn dispatch_critic_stub() -> DispatchOutcome {
    DispatchOutcome {
        output: AgentOutput::Critic(Critique {
            severity: CritiqueSeverity::Info,
            text: "stub critique".to_string(),
        }),
        input_tokens: 0,
        output_tokens: 0,
        raw_response: None,
    }
}

/// Phase B Intern stub. Phase D wires the real free-form note.
fn dispatch_intern_stub() -> DispatchOutcome {
    DispatchOutcome {
        output: AgentOutput::Intern(InternObservation {
            text: "stub intern".to_string(),
        }),
        input_tokens: 0,
        output_tokens: 0,
        raw_response: None,
    }
}

/// Router handler. Runs the slot's LLM with a strict JSON response
/// schema requiring `{ "target_agent_ref_index": <usize> }`, parses the
/// output, and validates the index is strictly greater than the Router's
/// own position. Backward / out-of-range targets are rejected as errors
/// so the operator sees the violation rather than a silent fall-through.
async fn dispatch_router(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    let current_index = input.current_index;
    let total_agents = input.total_agents;
    let resp = execute_slot(SlotInput {
        slot: input.slot,
        system_prompt: input.system_prompt,
        upstream_inputs: input.upstream_inputs,
        dispatch: input.dispatch,
        tools: input.tools,
        response_schema: Some(router_response_schema()),
        max_tokens: input.max_tokens,
        temperature: input.temperature,
        obs: input.obs,
        memory: input.memory,
        memory_mode: input.memory_mode,
        agent_id: input.agent_id,
        scenario_start: input.scenario_start,
        run_id: input.run_id,
        scenario_id: input.scenario_id,
        cycle_idx: input.cycle_idx,
        catalog: input.catalog,
        delta_briefing: input.delta_briefing,
        prev_briefing: input.prev_briefing,
    })
    .await?;

    let input_tokens = resp.input_tokens;
    let output_tokens = resp.output_tokens;
    let selection = parse_router_response(&resp, current_index, total_agents)?;

    Ok(DispatchOutcome {
        output: AgentOutput::Router(selection),
        input_tokens,
        output_tokens,
        raw_response: Some(resp),
    })
}

/// Pin the Router's response schema in one place so the dispatcher and
/// any future Phase F UI agree on the wire shape.
fn router_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "router_output".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["target_agent_ref_index"],
            "properties": {
                "target_agent_ref_index": {
                    "type": "integer",
                    "minimum": 0,
                }
            }
        }),
    }
}

/// Parse + validate a Router's LLM response.
///
/// Validation rules (Phase B Decision 8 — DAG-strict):
/// 1. The text must parse as JSON with a top-level
///    `target_agent_ref_index` integer.
/// 2. The index must be strictly greater than the Router's `current_index`
///    (no self-loops, no backward jumps).
/// 3. The index must be strictly less than `total_agents` (in-range).
fn parse_router_response(
    resp: &LlmResponse,
    current_index: usize,
    total_agents: usize,
) -> anyhow::Result<RouteSelection> {
    let text = resp.text();
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("router output is not valid JSON: {e} (raw: {text:.200})"))?;
    let idx = parsed
        .get("target_agent_ref_index")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("router output missing `target_agent_ref_index` (raw: {text:.200})"))?
        as usize;
    if idx <= current_index {
        anyhow::bail!(
            "router target_agent_ref_index={idx} is not strictly greater than current_index={current_index} \
             (DAG-strict: no self-loops or backward jumps)"
        );
    }
    if idx >= total_agents {
        anyhow::bail!("router target_agent_ref_index={idx} is out of range (total_agents={total_agents})");
    }
    Ok(RouteSelection {
        target_agent_ref_index: idx,
    })
}

/// Resolve `AgentRef.activates` → `Capability` per spec Decision 1.
///
/// * `Some(c)` → `c`.
/// * `None` → first capability in the slot's `BTreeSet` iteration order
///   (the enum declaration order: Trader, Filter, Critic, Intern, Router).
///   The default capability set is `{Trader}` so legacy/pre-033 slots
///   resolve to `Trader` and behave identically to the pre-Phase-B path.
/// * Empty set (defensive) → `Trader`.
pub fn resolve_activates(
    activates: Option<Capability>,
    capabilities: &std::collections::BTreeSet<Capability>,
) -> Capability {
    if let Some(c) = activates {
        return c;
    }
    capabilities.iter().next().copied().unwrap_or(Capability::Trader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{ContentBlock, StopReason};
    use std::collections::BTreeSet;

    #[test]
    fn resolve_activates_prefers_explicit_field() {
        let caps: BTreeSet<Capability> = [Capability::Trader, Capability::Critic].into_iter().collect();
        assert_eq!(
            resolve_activates(Some(Capability::Critic), &caps),
            Capability::Critic,
        );
    }

    #[test]
    fn resolve_activates_falls_back_to_first_capability_in_btreeset_order() {
        // BTreeSet iteration is enum-declaration order: Trader < Filter < Critic < Intern < Router.
        let caps: BTreeSet<Capability> = [Capability::Critic, Capability::Trader].into_iter().collect();
        assert_eq!(resolve_activates(None, &caps), Capability::Trader);

        // No Trader present — the first non-Trader wins.
        let caps: BTreeSet<Capability> = [Capability::Critic, Capability::Filter].into_iter().collect();
        assert_eq!(resolve_activates(None, &caps), Capability::Filter);
    }

    #[test]
    fn resolve_activates_handles_empty_set_with_trader_default() {
        let caps: BTreeSet<Capability> = BTreeSet::new();
        assert_eq!(resolve_activates(None, &caps), Capability::Trader);
    }

    fn router_resp(text: &str) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn parse_router_response_accepts_forward_target() {
        let resp = router_resp(r#"{"target_agent_ref_index": 2}"#);
        let sel = parse_router_response(&resp, 0, 3).unwrap();
        assert_eq!(sel.target_agent_ref_index, 2);
    }

    #[test]
    fn parse_router_response_rejects_same_index() {
        let resp = router_resp(r#"{"target_agent_ref_index": 1}"#);
        let err = parse_router_response(&resp, 1, 3).unwrap_err();
        assert!(err.to_string().contains("not strictly greater"), "got: {err}");
    }

    #[test]
    fn parse_router_response_rejects_backward_target() {
        let resp = router_resp(r#"{"target_agent_ref_index": 0}"#);
        let err = parse_router_response(&resp, 1, 3).unwrap_err();
        assert!(err.to_string().contains("not strictly greater"), "got: {err}");
    }

    #[test]
    fn parse_router_response_rejects_out_of_range_target() {
        let resp = router_resp(r#"{"target_agent_ref_index": 5}"#);
        let err = parse_router_response(&resp, 0, 3).unwrap_err();
        assert!(err.to_string().contains("out of range"), "got: {err}");
    }

    #[test]
    fn parse_router_response_rejects_invalid_json() {
        let resp = router_resp("not json");
        let err = parse_router_response(&resp, 0, 3).unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "got: {err}");
    }

    #[test]
    fn parse_router_response_rejects_missing_field() {
        let resp = router_resp(r#"{"other": 1}"#);
        let err = parse_router_response(&resp, 0, 3).unwrap_err();
        assert!(
            err.to_string().contains("missing `target_agent_ref_index`"),
            "got: {err}"
        );
    }
}
