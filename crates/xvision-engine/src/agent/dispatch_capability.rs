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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use xvision_core::trading::AssetSymbol;

// SlotInput and execute_slot were removed in WU-6 (LlmDispatch trader retirement).
use crate::agent::execute_cline::{execute_slot_cline, ClineSlotInput};
use crate::agent::llm::{LlmDispatch, LlmResponse, ResponseSchema};
use crate::agent::memory_recorder::MemoryRecorder;
use crate::agent::nano_dispatch::{
    build_nano_request, resolve_nano_filter, run_nano_inference, NanoDirection, NanoInferenceResult,
};
use crate::agent::observability::ObsEmitter;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::agents::Capability;
use crate::strategies::slot::LLMSlot;
use crate::tools::ToolRegistry;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry};
use xvision_core::providers::Catalog;
use xvision_observability::Recorder;

/// The Cline sidecar context for a capability dispatch: the live sidecar
/// client plus the provider identity + key required to start a Cline run.
/// Threaded from `PipelineInputs` so the dispatcher stays oblivious to how
/// the client was spawned. `None` means no sidecar — which since WU-6 is
/// a hard error for the trader (see `execute_slot_for_runtime`).
#[derive(Clone)]
pub struct ClineDispatchCtx {
    /// The shared, already-spawned sidecar client (one per run).
    pub client: Arc<AgentClient>,
    /// Resolved provider config for the run's provider. Mapped to a Cline
    /// gateway selection per slot via `map_provider`.
    pub provider_entry: ProviderEntry,
    /// API key for the provider, resolved from its env var by the eval
    /// entry point. `None`/empty for keyless local endpoints.
    pub api_key: Option<String>,
    /// §2-B: the `slot_role` the recording was keyed by at
    /// `begin_recording`, when this run is recording a trajectory.
    ///
    /// `Some` ⇒ recording is on: the dispatcher sets `StartRunParams.record
    /// = true` and stamps THIS exact `slot_role` so frames are keyed to the
    /// recording's `TrajectoryKey.slot_role` (footgun c — read_frames filters
    /// on slot_role, so a mismatch silently hides frames). The recording was
    /// minted in `spawn_cline_ctx` against the same role.
    ///
    /// `None` ⇒ no recording (live/backtest default): `record = false`,
    /// `slot_role = None` — byte-identical to the pre-§2-B path.
    pub recording_slot_role: Option<String>,
    /// Multi-asset (B4) tool-asset guard. When `Some`, the sidecar callback
    /// dispatcher reads this value to validate that tool calls reference the
    /// current decision asset. Updated by the executor per decision cycle.
    /// `None` for single-asset and non-sidecar runs.
    pub tool_asset_guard: Option<std::sync::Arc<tokio::sync::RwLock<Option<String>>>>,
    /// Simulated-clock anchor write handle, shared with `ToolRegistryDispatch`.
    /// The executor writes the current decision's timestamp here each cycle
    /// (Task 1.4). `None` for non-sidecar runs.
    pub as_of_guard: Option<std::sync::Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>>,
    /// The run's mode — used to filter `allowed_tools` by forward-only policy
    /// before advertising to the sidecar (Task 1.6).
    pub run_mode: crate::eval::run::RunMode,
}

/// Scope at which a `FilterSignal` is meaningful. First-class so cross-asset
/// and global signals are not second-class "synthetic asset name" hacks.
/// In v1's `PerAsset` fan-out the dispatcher tags signals `Asset(current)`;
/// the other variants exist so future filters emit them with no key migration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalScope {
    #[default]
    Global,
    Asset(AssetSymbol),
    Pair(AssetSymbol, AssetSymbol),
    Custom(String),
}

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
    /// Scope this signal applies to. Defaults to `Global` for back-compat
    /// with pre-multi-asset signal JSON.
    #[serde(default)]
    pub scope: SignalScope,
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

/// Typed sum of every capability-handler return value.
#[derive(Debug, Clone)]
pub enum AgentOutput {
    Trader(TraderDecision),
    Filter(FilterSignal),
    Router(RouteSelection),
}

impl AgentOutput {
    /// Convenience: extract a reference to the inner `FilterSignal` for
    /// edge-predicate evaluation. Returns `None` for any non-Filter
    /// output (the predicate evaluator treats this as "predicate fails"
    /// — a Trader / Router output never satisfies a `FilterSignal`
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
    /// QA30: optional per-step wall-clock budget for the Cline runtime.
    /// `None` means no enforcement. Plumbed through to
    /// `ClineSlotInput.max_wall_ms`. The dispatch input field is opt-in
    /// at every layer above — pipeline default is `None`.
    pub max_wall_ms: Option<u32>,
    pub temperature: Option<f64>,
    pub obs: Option<ObsEmitter>,
    pub memory: Option<Arc<MemoryRecorder>>,
    pub memory_mode: xvision_memory::types::MemoryMode,
    pub agent_id: String,
    pub scenario_start: Option<DateTime<Utc>>,
    pub source_window_start: Option<DateTime<Utc>>,
    pub source_window_end: Option<DateTime<Utc>>,
    pub run_id: String,
    pub scenario_id: String,
    pub cycle_idx: i64,
    /// Optional stable suffix for multiple logical dispatches of the same
    /// slot inside one decision cycle. The Cline sidecar deduplicates by
    /// run_id, so multi-filter re-fires of `trader` need distinct ids while
    /// preserving retry stability for each logical invocation.
    pub invocation_suffix: Option<String>,
    pub catalog: Option<Arc<Catalog>>,
    pub delta_briefing: bool,
    pub prev_briefing: Option<serde_json::Value>,
    pub trace_name: Option<String>,
    pub trace_attrs: Option<serde_json::Value>,
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
    /// Agent runtime selector. Always `AgentRuntime::Cline` since WU-6
    /// retired `LlmDispatch`. Retained on the struct for call-site
    /// compatibility; `should_use_cline` now only checks `cline.is_some()`.
    pub runtime: AgentRuntime,
    /// The live sidecar context, spawned by the eval entry point for this run.
    /// Since WU-6, the trader hard-errors if this is `None`. See [`ClineDispatchCtx`].
    pub cline: Option<ClineDispatchCtx>,
    /// WS-17 parent: the `decision.model` span id the executor opened
    /// around the enclosing `run_pipeline` call. Forwarded into
    /// `ClineSlotInput.model_call_span_id` so the captured
    /// `decision.reasoning` span nests under `decision.model`. `None`
    /// keeps the reasoning span top-level (rehearsal / non-eval paths).
    pub model_call_span_id: Option<String>,
}

/// Result of `dispatch_capability`: the typed `AgentOutput` AND the
/// accumulated input/output token counts from the underlying LLM call(s).
/// Filter stub handlers report `(0, 0)`; Trader and Router report whatever
/// the dispatcher returned.
#[derive(Debug)]
pub struct DispatchOutcome {
    pub output: AgentOutput,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// The raw `LlmResponse` from `execute_slot`, when a real LLM call
    /// happened. Trader and Router carry this; the Filter stub carries `None`.
    ///
    /// The pipeline reads this for two reasons: (1) eval executors still
    /// inspect the raw trader response, (2) the legacy
    /// `PipelineOutputs { regime, trader }` shape needs the
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
        Capability::Router => dispatch_router(input).await,
    }
}

/// Phase C — Filter dispatcher. DSL-backed slots (slot `provider ==
/// Public entry point for the nanochat filter branch, callable directly
/// from integration tests without building a full `DispatchInput`.
///
/// `llm_dir`: the upstream LLM filter's direction (conditioning token).
/// `veto`: true = hard gate; false = advisory.
/// Returns a `FilterSignal` whose `payload` is `Value::Null` when the gate
/// blocks, or `{ direction, confidence }` when it passes.
///
/// All nanochat fields (`weights_sha256`, `input_spec`, `veto`) come from the
/// caller (resolved from `NanoSlotConfig`). `CheckpointRef` carries only the
/// `model_id` for identity/logging — it does NOT carry `weights_sha256` or
/// `input_spec`.
pub async fn dispatch_filter_with_checkpoint(
    role: &str,
    llm_dir: NanoDirection,
    spec: &crate::agent::nano_dispatch::NanoInputSpec,
    ohlcv: &[[f64; 5]],
    indicator_values: &BTreeMap<String, f64>,
    worker_path: &std::path::Path,
    expected_sha256: &str,
    veto: bool,
    timeout_ms: u64,
) -> anyhow::Result<FilterSignal> {
    let request = build_nano_request(spec, llm_dir, ohlcv, indicator_values);
    let inference = run_nano_inference(worker_path, expected_sha256, &request, timeout_ms).await?;

    let payload = match inference {
        NanoInferenceResult::Ok {
            direction,
            confidence,
        } => resolve_nano_filter(llm_dir, direction, confidence, veto)
            .unwrap_or(serde_json::Value::Null),
        NanoInferenceResult::FailSafe { reason } => {
            tracing::warn!(
                event = "nanochat_fail_safe",
                role,
                reason,
                "nanochat inference failed; treating as NEUTRAL under veto"
            );
            // Fail-safe: same as NEUTRAL under veto.
            if veto {
                serde_json::Value::Null
            } else {
                serde_json::json!({"direction": "NEUTRAL", "confidence": 0.0})
            }
        }
    };

    Ok(FilterSignal {
        name: role.to_string(),
        payload,
        granularity: FilterGranularity::Bar,
        ts: Utc::now(),
        scope: SignalScope::Global,
    })
}

/// Walk `upstream_inputs["filter_signals"] → any entry → payload → direction`
/// to extract the upstream LLM filter's direction for nanochat conditioning.
/// Defaults to `Neutral` when absent (conservative fail-safe).
fn extract_llm_direction(upstream: &serde_json::Value) -> NanoDirection {
    upstream
        .get("filter_signals")
        .and_then(|fs| fs.as_object())
        .and_then(|obj| obj.values().next())
        .and_then(|sig| sig.get("payload"))
        .and_then(|p| p.get("direction"))
        .and_then(|d| d.as_str())
        .and_then(|s| serde_json::from_value::<NanoDirection>(serde_json::json!(s)).ok())
        .unwrap_or(NanoDirection::Neutral)
}

/// Extract OHLCV bars from `upstream_inputs["market_data"]["ohlcv"]`.
/// Returns an empty vec when absent.
fn extract_ohlcv(upstream: &serde_json::Value) -> Vec<[f64; 5]> {
    upstream
        .get("market_data")
        .and_then(|md| md.get("ohlcv"))
        .and_then(|arr| arr.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let arr = row.as_array()?;
                    if arr.len() < 5 {
                        return None;
                    }
                    Some([
                        arr[0].as_f64().unwrap_or(0.0),
                        arr[1].as_f64().unwrap_or(0.0),
                        arr[2].as_f64().unwrap_or(0.0),
                        arr[3].as_f64().unwrap_or(0.0),
                        arr[4].as_f64().unwrap_or(0.0),
                    ])
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract named indicator values from `upstream_inputs["market_data"]["indicator_panel"]`.
/// Values absent in the panel default to 0.0.
fn extract_indicators(upstream: &serde_json::Value, names: &[String]) -> BTreeMap<String, f64> {
    let panel = upstream
        .get("market_data")
        .and_then(|md| md.get("indicator_panel"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    names
        .iter()
        .map(|name| {
            let val = panel.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0);
            (name.clone(), val)
        })
        .collect()
}

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

    // Nanochat branch: ResolvedAgentSlot carries a NanoSlotConfig → local model dispatch.
    // `nano` is populated by the async resolvers (resolve_agent_slots_for_strategy /
    // HTTP eval resolver) when AgentRef.checkpoint is set. No LLM path is entered;
    // token counts are 0/0.
    //
    // NOTE: ALL nanochat fields (weights_sha256, input_spec, veto) are read from
    // `input.resolved.nano` (a NanoSlotConfig). CheckpointRef carries ONLY `model_id`
    // (identity); it does NOT carry weights_sha256 or input_spec.
    if let Some(nano) = input.resolved.nano.as_ref() {
        let worker_path = std::env::var("XVN_NANOCHAT_WORKER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("nanochat/infer.py"));

        // The upstream FilterSignal conditioning direction is read from
        // `input.upstream_inputs["filter_signals"][<prev_role>]["payload"]["direction"]`.
        // Default to NEUTRAL when absent (conservative fail-safe).
        let llm_dir = extract_llm_direction(&input.upstream_inputs);

        // veto and input_spec come from nano (NanoSlotConfig), not from CheckpointRef.
        let veto = nano.veto.unwrap_or(true);
        let spec = &nano.input_spec;

        let ohlcv = extract_ohlcv(&input.upstream_inputs);
        let indicator_values = extract_indicators(&input.upstream_inputs, &spec.indicators);

        // weights_sha256 comes from nano (loaded from trained_models at resolve time).
        // nano.checkpoint.model_id is used only for identity/logging.
        let signal = dispatch_filter_with_checkpoint(
            &role,
            llm_dir,
            spec,
            &ohlcv,
            &indicator_values,
            &worker_path,
            &nano.weights_sha256,
            veto,
            input.max_wall_ms.unwrap_or(10_000) as u64,
        )
        .await?;

        return Ok(DispatchOutcome {
            output: AgentOutput::Filter(signal),
            input_tokens: 0,
            output_tokens: 0,
            raw_response: None,
        });
    }

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
            scope: SignalScope::Global,
        };
        return Ok(DispatchOutcome {
            output: AgentOutput::Filter(signal),
            input_tokens: 0,
            output_tokens: 0,
            raw_response: None,
        });
    }

    // filter.eval span: wrap the LLM call so the trace dock can show
    // per-filter verdict without parsing model.call plaintext.
    let span_id = crate::agent::observability::fresh_span_id();
    let obs_clone = input.obs.clone();
    if let Some(obs) = obs_clone.as_ref() {
        obs.emit_filter_eval_started(&span_id, None, &input.scenario_id)
            .await;
    }
    let result = crate::agent::filter_dispatch::run_llm_filter(input).await;
    match result {
        Ok(result) => {
            let verdict = if result.signal.payload.is_null() {
                "reject"
            } else {
                "pass"
            };
            if let Some(obs) = obs_clone.as_ref() {
                obs.emit_filter_eval_finished(&span_id, verdict, None).await;
            }
            Ok(DispatchOutcome {
                output: AgentOutput::Filter(result.signal),
                input_tokens: result.input_tokens,
                output_tokens: result.output_tokens,
                raw_response: None,
            })
        }
        Err(crate::agent::filter_dispatch::FilterDispatchError::Parse(_)) => {
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
                    scope: SignalScope::Global,
                }),
                input_tokens: 0,
                output_tokens: 0,
                raw_response: None,
            })
        }
        Err(crate::agent::filter_dispatch::FilterDispatchError::Dispatch(e)) => Err(e),
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

/// True when this dispatch has a live Cline sidecar context wired.
/// Since WU-6 retired `LlmDispatch`, the Cline path is mandatory for the
/// trader; `cline` being `None` is now an error rather than a silent fallback.
fn should_use_cline(input: &DispatchInput<'_>) -> bool {
    input.cline.is_some()
}

/// Build the Cline idempotency `run_id` (item 2) for a slot invocation.
/// The default shape is `{eval_run_id}::{role}::cycle{cycle_idx}`. Pipeline
/// paths that invoke the same slot more than once in one decision cycle append
/// a stable suffix so the sidecar dedups retries of each logical invocation
/// without treating sibling invocations as duplicates.
fn cline_run_id(input: &DispatchInput<'_>) -> String {
    let base = if input.run_id.is_empty() {
        input.scenario_id.as_str()
    } else {
        input.run_id.as_str()
    };
    let mut run_id = format!("{base}::{}::cycle{}", input.resolved.role, input.cycle_idx);
    if let Some(suffix) = input.invocation_suffix.as_deref().filter(|s| !s.is_empty()) {
        run_id.push_str("::");
        run_id.push_str(suffix);
    }
    run_id
}

/// Run the slot's LLM call through the Cline sidecar. Since WU-6 retired
/// `LlmDispatch`, the Cline path is the ONLY trader runtime. If no
/// `ClineDispatchCtx` is wired, this is a hard error — never a silent fallback.
async fn execute_slot_for_runtime(
    input: &DispatchInput<'_>,
    response_schema: ResponseSchema,
) -> anyhow::Result<LlmResponse> {
    if should_use_cline(input) {
        let ctx = input.cline.as_ref().expect("should_use_cline checked Some");
        let run_id = cline_run_id(input);
        // §2-B: enable recording for this slot ONLY when the run minted a
        // recording AND the recording's slot_role matches THIS slot's role.
        // The recording is keyed per (cycle/run, slot_role); the persister
        // appends frames at the envelope's (slot_role, step_index,
        // frame_index), so stamping the matching role keeps frames readable
        // on replay (footgun c). A slot whose role differs from the
        // recording's role is not recorded (record=false) — never silently
        // mis-keyed.
        let record_slot_role = ctx
            .recording_slot_role
            .as_deref()
            .filter(|r| *r == input.slot.role.as_str())
            .map(str::to_string);
        return execute_slot_cline(ClineSlotInput {
            slot: input.slot,
            provider_entry: &ctx.provider_entry,
            api_key: ctx.api_key.clone(),
            system_prompt: input.system_prompt.clone(),
            upstream_inputs: input.upstream_inputs.clone(),
            response_schema,
            allowed_tools: crate::tools::signal_policy::filter_tools_for_mode(
                &input.slot.allowed_tools,
                ctx.run_mode,
            ),
            max_tokens: input.max_tokens,
            max_wall_ms: input.max_wall_ms,
            run_id,
            cline_client: ctx.client.clone(),
            // Eval/live dispatch always records; replay is driven from the
            // dedicated CLI record/replay entry points (Stage 3 Task 8),
            // not the per-cycle pipeline dispatch.
            trajectory_mode: crate::agent::execute_cline::TrajectoryMode::Record,
            record_slot_role,
            // F5: thread the parent-run emitter so the failure path can
            // correct the child `agent_runs` row status (see
            // `ClineSlotInput::obs` field docs).
            obs: input.obs.clone(),
            // WS-17 (reasoning capture): the executor opens a `decision.model`
            // span around `run_pipeline` (child of the `agent.decision`
            // span) and threads its id down to here, so the captured
            // `<think>` chain-of-thought emits as a `decision.reasoning`
            // span nested under `decision.model`. `None` (rehearsal /
            // non-eval call sites that don't own a decision-model span)
            // keeps the reasoning span top-level — it still reaches the
            // trace.
            model_call_span_id: input.model_call_span_id.clone(),
            // Derive reasoning_effort from model metadata so CoT models
            // (deepseek-r1, qwq, etc.) get an explicit effort hint forwarded
            // to the provider gateway. Non-CoT models produce None (field
            // omitted on the wire).
            reasoning_effort: crate::agents::model::default_reasoning_effort(&input.slot.effective_model()),
        })
        .await;
    }

    // WU-6: LlmDispatch was retired. A slot dispatched without a Cline
    // context is a programmer error — the sidecar must be spawned before
    // the pipeline is entered.
    anyhow::bail!(
        "trader requires the Cline sidecar (WU-6: LlmDispatch was retired); \
         ensure XVN_AGENTD_BIN is set and spawn_cline_ctx was called before \
         entering the pipeline (slot role: {})",
        input.slot.role
    )
}

/// Trader handler. The surrounding pipeline handles the `noop_skip`
/// short-circuit before reaching this seam, so the LLM call is unconditional.
/// Since WU-6, the call always routes through the Cline sidecar.
async fn dispatch_trader(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    let resp = execute_slot_for_runtime(&input, ResponseSchema::trader_output()).await?;

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

/// Router handler. Runs the slot's LLM with a strict JSON response
/// schema requiring `{ "target_agent_ref_index": <usize> }`, parses the
/// output, and validates the index is strictly greater than the Router's
/// own position. Backward / out-of-range targets are rejected as errors
/// so the operator sees the violation rather than a silent fall-through.
async fn dispatch_router(input: DispatchInput<'_>) -> anyhow::Result<DispatchOutcome> {
    let current_index = input.current_index;
    let total_agents = input.total_agents;
    let resp = execute_slot_for_runtime(&input, router_response_schema()).await?;

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
///   (the enum declaration order: Trader, Filter, Router).
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
    fn signal_scope_round_trips_each_variant() {
        use xvision_core::trading::AssetSymbol;
        for scope in [
            SignalScope::Global,
            SignalScope::Asset(AssetSymbol::Btc),
            SignalScope::Pair(AssetSymbol::Btc, AssetSymbol::Eth),
            SignalScope::Custom("vol_basket".into()),
        ] {
            let s = serde_json::to_string(&scope).unwrap();
            let back: SignalScope = serde_json::from_str(&s).unwrap();
            assert_eq!(scope, back);
        }
    }

    #[test]
    fn filter_signal_defaults_scope_to_global_when_absent() {
        let json = serde_json::json!({
            "name": "regime", "payload": {"regime":"trend"},
            "granularity": "bar", "ts": "2026-05-24T00:00:00Z"
        });
        let sig: FilterSignal = serde_json::from_value(json).unwrap();
        assert_eq!(sig.scope, SignalScope::Global);
    }

    #[test]
    fn resolve_activates_prefers_explicit_field() {
        let caps: BTreeSet<Capability> = [Capability::Trader, Capability::Router].into_iter().collect();
        assert_eq!(
            resolve_activates(Some(Capability::Router), &caps),
            Capability::Router,
        );
    }

    #[test]
    fn resolve_activates_falls_back_to_first_capability_in_btreeset_order() {
        // BTreeSet iteration is enum-declaration order: Trader < Filter < Router.
        let caps: BTreeSet<Capability> = [Capability::Router, Capability::Trader].into_iter().collect();
        assert_eq!(resolve_activates(None, &caps), Capability::Trader);

        // No Trader present — the first non-Trader wins.
        let caps: BTreeSet<Capability> = [Capability::Router, Capability::Filter].into_iter().collect();
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
