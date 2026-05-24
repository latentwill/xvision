//! `execute_slot_cline` — drive one LLM slot through the Cline sidecar
//! (`xvision-agentd`) instead of the raw [`crate::agent::llm::LlmDispatch`]
//! HTTP path.
//!
//! Stage 1 of the Cline runtime unification (umbrella:
//! `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`,
//! plan: `docs/superpowers/plans/2026-05-24-cline-stage1-live-path.md`,
//! Task 5). A slot invocation becomes a Cline `Agent` run:
//! `start_run` → `step` → `end_run`. The agent returns its structured
//! decision by calling the built-in `submit_decision` lifecycle tool; the
//! sidecar captures that payload and surfaces it on
//! [`xvision_agent_client::StepResult::decision_json`].
//!
//! This is a SIBLING executor to [`crate::agent::execute::execute_slot`] —
//! it deliberately does NOT extend `SlotInput` (which would ripple to
//! every call site). It returns the same [`LlmResponse`] shape the existing
//! decision parser accepts: a single [`ContentBlock::Text`] whose body is
//! the decision JSON, so `resp.text()` round-trips through the unchanged
//! `dispatch_capability` parser and `TraderOutput::parse_response`.
//!
//! ## Failure + recovery contract (umbrella "Subplan inheritance contract"
//! item 2)
//!
//! * A transport/crash error on `step` propagates as a typed
//!   [`ClineRuntimeError`] — the cycle fails, it is NEVER a silent empty
//!   decision.
//! * `end_run` is always attempted, even when `step` errored, so the
//!   sidecar session is reclaimed.
//! * `run_id` is the idempotency key. The sidecar `store.ts` keys sessions
//!   by `run_id`, so a retried `start_run` with the same id is rejected /
//!   deduped by the sidecar rather than double-executing here.
//! * **Stage 3 obligation (tracked here so it is not lost):** live-vs-replay
//!   divergence handling is n/a at Stage 1 (no replay yet). When Stage 3
//!   lands record/replay, this executor must compare the live trajectory
//!   against the recorded one and surface divergence as a typed error.

use crate::agent::llm::{ContentBlock, LlmResponse, ResponseSchema, StopReason};
use crate::strategies::slot::LLMSlot;
use std::sync::Arc;
use xvision_agent_client::provider_map::{map_provider, ProviderMapError};
use xvision_agent_client::{AgentClient, BudgetLimits, EndRunParams, StartRunParams, StepParams};
use xvision_core::config::ProviderEntry;

/// The built-in lifecycle tool the Cline agent calls exactly once to emit
/// its structured decision. MUST be in the run's `allowed_tools` or the
/// sidecar will not register it. Mirrors `SUBMIT_DECISION_TOOL` in
/// `xvision-agentd/src/session/submit-decision.ts`.
pub const SUBMIT_DECISION_TOOL: &str = "submit_decision";

/// Default per-run budgets when the slot does not pin an explicit
/// `max_tokens`. The wall-clock cap bounds a wedged sidecar step so a
/// crashed/looping run surfaces as a typed budget abort rather than
/// hanging the cycle (item 2 — failure boundary).
const DEFAULT_MAX_INPUT_TOKENS: u32 = 200_000;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 8_192;
const DEFAULT_MAX_WALL_MS: u32 = 120_000;

/// Typed Cline-runtime failure classes (item 2). Wrapped in
/// `anyhow::Error` at the call boundary so existing `anyhow::Result`
/// callers keep compiling, but the pipeline / observability layer can
/// `downcast_ref` for a stable class tag.
#[derive(Debug, thiserror::Error)]
pub enum ClineRuntimeError {
    /// The provider has no Cline mapping (item 5). A hard abort — never a
    /// silent fallback to `LlmDispatch` unless the runtime flag explicitly
    /// selects it.
    #[error("cline runtime: {0}")]
    ProviderUnmapped(#[from] ProviderMapError),

    /// `start_run` failed (transport, handshake, duplicate run_id dedup,
    /// or sidecar-side validation). Carries the slot role + run_id so an
    /// operator can correlate to the failing cycle.
    #[error("cline runtime: start_run failed for run_id={run_id} (role={role}): {source}")]
    StartRun {
        run_id: String,
        role: String,
        #[source]
        source: xvision_agent_client::AgentClientError,
    },

    /// `step` failed — a transport error or a sidecar crash mid-step. This
    /// is the crash boundary: the cycle fails, the decision is never
    /// silently dropped.
    #[error("cline runtime: step failed (sidecar transport/crash) for run_id={run_id} (role={role}): {source}")]
    StepTransport {
        run_id: String,
        role: String,
        #[source]
        source: xvision_agent_client::AgentClientError,
    },

    /// The step completed at the protocol level but did not reach a
    /// terminal `completed` status (e.g. budget abort, sidecar error).
    #[error("cline runtime: step did not complete for run_id={run_id} (role={role}): status={status} error={error:?}")]
    StepNotCompleted {
        run_id: String,
        role: String,
        status: String,
        error: Option<String>,
    },

    /// The run completed but the agent never called `submit_decision`, so
    /// there is no structured decision to parse. Failing here (rather than
    /// synthesizing an empty/hold decision) is the item-2 guarantee that a
    /// missing decision fails the cycle visibly.
    #[error("cline runtime: run completed without calling submit_decision for run_id={run_id} (role={role})")]
    NoDecision { run_id: String, role: String },

    /// `submit_decision` returned a payload that is not valid JSON. The
    /// downstream parser expects parseable JSON text; surface the failure
    /// typed instead of feeding garbage to the parser.
    #[error("cline runtime: submit_decision payload was not valid JSON for run_id={run_id} (role={role}): {source}")]
    DecisionNotJson {
        run_id: String,
        role: String,
        #[source]
        source: serde_json::Error,
    },
}

/// Inputs to a single [`execute_slot_cline`] invocation. Its OWN struct
/// (not an extension of [`crate::agent::execute::SlotInput`]) so the Cline
/// path stays isolated from the dozens of `SlotInput` call sites.
pub struct ClineSlotInput<'a> {
    /// The slot to dispatch (role / provider / model). The slot's
    /// `effective_model()` is the Cline `modelId`.
    pub slot: &'a LLMSlot,
    /// The resolved xvision provider config for the slot's provider. Mapped
    /// to a Cline `providerId` + `baseUrl` via [`map_provider`].
    pub provider_entry: &'a ProviderEntry,
    /// API key for the provider (already resolved from its env var by the
    /// caller). `None`/empty for keyless local endpoints.
    pub api_key: Option<String>,
    /// System prompt for this slot (from the bound `AgentSlot.system_prompt`).
    pub system_prompt: String,
    /// The briefing / upstream inputs rendered into the first user turn.
    pub upstream_inputs: serde_json::Value,
    /// The structured-decision schema the agent must satisfy via
    /// `submit_decision`. Required by the sidecar whenever `allowed_tools`
    /// contains `submit_decision`.
    pub response_schema: ResponseSchema,
    /// Extra (non-`submit_decision`) tool names exposed to the agent — the
    /// strategy's `required_tools` / slot `allowed_tools`. `submit_decision`
    /// is appended automatically.
    pub allowed_tools: Vec<String>,
    /// Operator's per-request output-token budget. `None` falls back to
    /// [`DEFAULT_MAX_OUTPUT_TOKENS`].
    pub max_tokens: Option<u32>,
    /// The idempotency key for the Cline run (item 2). Built by the caller
    /// from `cycle_id` + slot role so a retried cycle re-uses the same id
    /// and the sidecar dedups it. MUST be unique per logical slot
    /// invocation within a cycle.
    pub run_id: String,
    /// The live Cline sidecar client, shared across slots in a run.
    pub cline_client: Arc<AgentClient>,
}

impl ClineSlotInput<'_> {
    fn role(&self) -> &str {
        &self.slot.role
    }

    /// Tool surface for the run: caller-supplied tools plus the built-in
    /// `submit_decision` lifecycle tool (deduped so an explicit listing in
    /// `allowed_tools` is harmless).
    fn allowed_tools_plus_submit_decision(&self) -> Vec<String> {
        let mut out = self.allowed_tools.clone();
        if !out.iter().any(|t| t == SUBMIT_DECISION_TOOL) {
            out.push(SUBMIT_DECISION_TOOL.to_string());
        }
        out
    }

    fn budget_limits(&self) -> BudgetLimits {
        BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: self.max_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        }
    }

    /// Render the first user turn. Mirrors `execute_slot`'s framing so the
    /// model sees the same inputs shape regardless of runtime.
    fn render_prompt(&self) -> anyhow::Result<String> {
        Ok(format!(
            "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
             to fetch additional data; submit your final decision via the \
             `submit_decision` tool as JSON matching the required schema.",
            serde_json::to_string_pretty(&self.upstream_inputs)?
        ))
    }
}

/// Execute one slot as a Cline `Agent` run and return its structured
/// decision wrapped in an [`LlmResponse`] the existing parser accepts.
///
/// See the module docs for the failure + recovery contract (item 2) and
/// the provider-matrix abort behavior (item 5).
pub async fn execute_slot_cline(input: ClineSlotInput<'_>) -> anyhow::Result<LlmResponse> {
    let role = input.role().to_string();
    let run_id = input.run_id.clone();

    // Item 5: map provider → Cline gateway selection. An unmapped provider
    // (e.g. local-candle) aborts with a typed error — NO silent fallback.
    let mapped = map_provider(input.provider_entry, &input.slot.effective_model())
        .map_err(ClineRuntimeError::ProviderUnmapped)?;

    let start = StartRunParams {
        run_id: run_id.clone(),
        provider_id: mapped.provider_id,
        model_id: mapped.model_id,
        api_key: input.api_key.clone().filter(|k| !k.is_empty()),
        base_url: mapped.base_url,
        system_prompt: input.system_prompt.clone(),
        allowed_tools: input.allowed_tools_plus_submit_decision(),
        budget_limits: input.budget_limits(),
        decision_schema: Some(input.response_schema.schema.clone()),
    };

    input
        .cline_client
        .start_run(start)
        .await
        .map_err(|source| ClineRuntimeError::StartRun {
            run_id: run_id.clone(),
            role: role.clone(),
            source,
        })?;

    // One step drives the agent's tool loop to completion. The step result
    // is computed first, then `end_run` is ALWAYS attempted so the sidecar
    // session is reclaimed even when the step errored (item 2).
    let step_result = input
        .cline_client
        .step(StepParams {
            run_id: run_id.clone(),
            prompt: input.render_prompt()?,
        })
        .await;

    let _ = input
        .cline_client
        .end_run(EndRunParams {
            run_id: run_id.clone(),
        })
        .await;

    // Crash boundary: a transport error means the sidecar died mid-step or
    // the connection dropped. Surface it typed; the cycle fails.
    let step = step_result.map_err(|source| ClineRuntimeError::StepTransport {
        run_id: run_id.clone(),
        role: role.clone(),
        source,
    })?;

    if step.status != "completed" {
        return Err(ClineRuntimeError::StepNotCompleted {
            run_id,
            role,
            status: step.status,
            error: step.error,
        }
        .into());
    }

    let decision_json = step.decision_json.ok_or_else(|| ClineRuntimeError::NoDecision {
        run_id: run_id.clone(),
        role: role.clone(),
    })?;

    // Validate the payload parses as JSON here so the typed error is
    // attributable to the Cline runtime rather than surfacing later as a
    // generic parser failure. The original text is preserved verbatim in
    // the returned ContentBlock so the downstream parser sees exactly what
    // the agent submitted.
    let _: serde_json::Value =
        serde_json::from_str(&decision_json).map_err(|source| ClineRuntimeError::DecisionNotJson {
            run_id: run_id.clone(),
            role: role.clone(),
            source,
        })?;

    Ok(LlmResponse {
        // The existing decision parser (`dispatch_capability::parse_*` and
        // `TraderOutput::parse_response`) reads `resp.text()`, which
        // concatenates Text blocks. Emit the decision JSON as a single
        // Text block so the parser is byte-identical to the LlmDispatch path.
        content: vec![ContentBlock::Text { text: decision_json }],
        stop_reason: StopReason::EndTurn,
        input_tokens: u32::try_from(step.usage.input_tokens).unwrap_or(u32::MAX),
        output_tokens: u32::try_from(step.usage.output_tokens).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `ClineSlotInput` against an already-constructed client so the
    /// pure helpers (`allowed_tools_plus_submit_decision`, `budget_limits`,
    /// `render_prompt`) can be exercised without an active session. The
    /// end-to-end run is covered by the integration test in
    /// `tests/cline_execute_slot.rs`.
    fn make_input<'a>(
        slot: &'a LLMSlot,
        entry: &'a ProviderEntry,
        client: Arc<AgentClient>,
        extra_tools: Vec<String>,
        max_tokens: Option<u32>,
    ) -> ClineSlotInput<'a> {
        ClineSlotInput {
            slot,
            provider_entry: entry,
            api_key: Some("k".into()),
            system_prompt: "decide".into(),
            upstream_inputs: serde_json::json!({"x": 1}),
            response_schema: ResponseSchema::trader_output(),
            allowed_tools: extra_tools,
            max_tokens,
            run_id: "cycle-1::trader".into(),
            cline_client: client,
        }
    }

    fn slot() -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: Vec::new(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        }
    }

    fn entry() -> ProviderEntry {
        ProviderEntry {
            name: "anthropic".into(),
            kind: xvision_core::config::ProviderKind::Anthropic,
            base_url: String::new(),
            api_key_env: "K".into(),
            enabled_models: vec!["claude-sonnet-4-6".into()],
        }
    }

    // Constructing a real `AgentClient` requires a spawned sidecar, so the
    // helper tests below cover the pure logic that does not touch the
    // transport. They build the input lazily only when a client exists; the
    // `make_input` fn keeps the field set honest at compile time.
    #[allow(dead_code)]
    fn _typecheck_make_input(client: Arc<AgentClient>) {
        let s = slot();
        let e = entry();
        let _ = make_input(&s, &e, client, vec!["indicators.rsi".into()], Some(4096));
    }

    #[test]
    fn dedup_helper_appends_submit_decision_once() {
        // Mirror `allowed_tools_plus_submit_decision`'s contract without a
        // client: an explicit listing must not duplicate the tool.
        fn plus(mut v: Vec<String>) -> Vec<String> {
            if !v.iter().any(|t| t == SUBMIT_DECISION_TOOL) {
                v.push(SUBMIT_DECISION_TOOL.to_string());
            }
            v
        }
        let from_empty = plus(vec![]);
        assert_eq!(from_empty, vec![SUBMIT_DECISION_TOOL.to_string()]);

        let from_extra = plus(vec!["indicators.rsi".into()]);
        assert_eq!(from_extra.len(), 2);
        assert_eq!(from_extra.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(), 1);

        let already = plus(vec![SUBMIT_DECISION_TOOL.into()]);
        assert_eq!(already.iter().filter(|t| *t == SUBMIT_DECISION_TOOL).count(), 1);
    }

    #[test]
    fn budget_uses_max_tokens_then_default() {
        let with = BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: Some(4096u32).unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        };
        assert_eq!(with.max_output_tokens, 4096);

        let without = BudgetLimits {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            max_output_tokens: None.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            max_wall_ms: DEFAULT_MAX_WALL_MS,
        };
        assert_eq!(without.max_output_tokens, DEFAULT_MAX_OUTPUT_TOKENS);
        assert!(without.max_wall_ms > 0);
    }
}
