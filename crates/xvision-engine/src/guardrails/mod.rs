//! No-short-circuit execution guardrails (Phase 4.2).
//!
//! Prevents a strategy-agent run from *appearing* to succeed when it
//! actually skipped a real prerequisite. Each guardrail is a **pure
//! detector**: given the relevant already-loaded inputs it returns
//! `Ok(())` when the precondition holds and `Err(ShortCircuit)` — a
//! distinct, typed variant — when it does not.
//!
//! The spec
//! (`docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md`
//! §4.2) names ten short-circuit classes. Each maps to exactly one
//! [`ShortCircuit`] variant with:
//!
//! * a stable machine [`ShortCircuit::code`] (the CLI branches on it and
//!   returns non-zero),
//! * an operator-facing [`ShortCircuit::remediation`] string (the UI shows
//!   it), and
//! * a [`ShortCircuit::to_typed_error`] mapping into the matching
//!   [`UnifiedPayload::Error*`] event so the event stream **records the
//!   failed prerequisite** rather than a silent success.
//!
//! ## What this module is NOT
//!
//! Pure detection only. It does NOT wire itself into the live agent loop,
//! the chat rail, the wizard loop, the optimization route, the diagnostics
//! module internals, or the mint module — the conductor wires the call
//! sites (see the per-variant "Wire into" docs and the module-level table
//! at the bottom of this file). It also does NOT depend on `xvision-dspy`
//! (offline-isolation invariant): stale-prompt detection compares two
//! already-computed `signature_hash` strings; demo-set emptiness is a
//! count the caller passes in.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use xvision_observability::unified_event::{TypedError, UnifiedPayload};

/// A detected short-circuit: a real prerequisite was missing, so the run
/// must surface a typed error instead of pretending to have done the work.
///
/// One variant per class named in spec §4.2. The variants carry just
/// enough context to render an actionable message; the `code()` is the
/// stable machine identifier and never changes once shipped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShortCircuit {
    /// A tool the capability requires is registered nowhere (not in the
    /// slot's grants, not in the strategy manifest's `required_tools`, not
    /// a built-in). The agent cannot do its job without it.
    #[error("required tool '{tool}' is not available to role '{role}'")]
    MissingTool { role: String, tool: String },

    /// A required tool exists in the registry but is *disabled* for this
    /// run (policy/mode turned it off). Distinct from `MissingTool`: the
    /// tool is known, just not usable right now.
    #[error("required tool '{tool}' is disabled for role '{role}'")]
    DisabledTool { role: String, tool: String },

    /// The provider bound to a slot is not available (no credentials, not
    /// reachable, or not in the enabled-provider set).
    #[error("provider '{provider}' for role '{role}' is unavailable")]
    ProviderUnavailable { role: String, provider: String },

    /// The slot fulfilling a required capability has an empty /
    /// whitespace-only system prompt. There is nothing to send the model.
    #[error("role '{role}' has no system prompt")]
    MissingPrompt { role: String },

    /// A model/tool produced output that does not satisfy the expected
    /// schema. `expected` names the schema; `detail` is the validation
    /// failure.
    #[error("invalid output schema for role '{role}' (expected {expected}): {detail}")]
    InvalidOutputSchema {
        role: String,
        expected: String,
        detail: String,
    },

    /// An optimizer was asked to run (or a snapshot was asked to be built)
    /// for a capability that requires demonstrations, but the demo set is
    /// empty. Optimizing with zero demos silently degrades to a no-op.
    #[error("capability '{capability}' requires demonstrations but the demo set is empty")]
    EmptyDemoSet { capability: String },

    /// An accepted optimized prompt's snapshot `signature_hash` no longer
    /// matches the current bound signature shape — applying it would feed
    /// the model a prompt tuned for a different schema.
    #[error(
        "optimized prompt for role '{role}' is stale: snapshot signature {snapshot_signature_hash} != current {current_signature_hash}"
    )]
    StaleOptimizedPrompt {
        role: String,
        snapshot_signature_hash: String,
        current_signature_hash: String,
    },

    /// A downstream position consumes a `FilterSignal` from an upstream
    /// role, but no filter output for that role exists this cycle. The
    /// downstream agent would run on a missing input.
    #[error("role '{consumer_role}' requested a filter signal from '{producer_role}' but none was produced")]
    FilterSignalRequestedButAbsent {
        consumer_role: String,
        producer_role: String,
    },

    /// A strategy `AgentRef` names a slot (role) that is not attached —
    /// the referenced agent has no slot fulfilling it. The pipeline graph
    /// is wired to a position that cannot execute.
    #[error("strategy references unattached slot '{role}' (agent '{agent_id}')")]
    StrategyReferencesUnattachedSlot { role: String, agent_id: String },

    /// A dashboard action reported success but produced only a UI artifact
    /// with no corresponding persisted row — the change would vanish on
    /// reload.
    #[error("dashboard action '{action}' produced artifact '{artifact_kind}' with no persisted row")]
    DashboardArtifactWithoutPersistedRow { action: String, artifact_kind: String },
}

impl ShortCircuit {
    /// Stable machine-readable code. CLI `--json` consumers branch on this;
    /// it is part of the wire contract and must not change once shipped.
    pub fn code(&self) -> &'static str {
        match self {
            ShortCircuit::MissingTool { .. } => "missing_tool",
            ShortCircuit::DisabledTool { .. } => "disabled_tool",
            ShortCircuit::ProviderUnavailable { .. } => "provider_unavailable",
            ShortCircuit::MissingPrompt { .. } => "missing_prompt",
            ShortCircuit::InvalidOutputSchema { .. } => "invalid_output_schema",
            ShortCircuit::EmptyDemoSet { .. } => "empty_demo_set",
            ShortCircuit::StaleOptimizedPrompt { .. } => "stale_optimized_prompt",
            ShortCircuit::FilterSignalRequestedButAbsent { .. } => "filter_signal_requested_but_absent",
            ShortCircuit::StrategyReferencesUnattachedSlot { .. } => "strategy_references_unattached_slot",
            ShortCircuit::DashboardArtifactWithoutPersistedRow { .. } => {
                "dashboard_artifact_without_persisted_row"
            }
        }
    }

    /// Operator-facing remediation hint. The UI renders this verbatim under
    /// the failed-prerequisite row.
    pub fn remediation(&self) -> String {
        match self {
            ShortCircuit::MissingTool { tool, .. } => format!(
                "Add '{tool}' to the strategy's required_tools (or grant it on the slot) and re-run.",
            ),
            ShortCircuit::DisabledTool { tool, .. } => format!(
                "Tool '{tool}' is disabled in the current mode/policy. Enable it (or switch to a mode that allows it) and re-run.",
            ),
            ShortCircuit::ProviderUnavailable { provider, .. } => format!(
                "Provider '{provider}' is not available. Add its credentials in Settings > Providers, or bind the slot to an enabled provider.",
            ),
            ShortCircuit::MissingPrompt { role, .. } => format!(
                "Write a system prompt for the '{role}' slot before launching.",
            ),
            ShortCircuit::InvalidOutputSchema { expected, .. } => format!(
                "The output did not match the {expected} schema. Tighten the prompt's output contract or fix the schema binding, then re-run.",
            ),
            ShortCircuit::EmptyDemoSet { .. } => {
                "Select a non-empty training corpus (the optimizer needs demonstrations) before optimizing.".to_string()
            }
            ShortCircuit::StaleOptimizedPrompt { .. } => {
                "The optimized prompt was tuned for a different signature. Re-run the optimizer against the current signature before applying.".to_string()
            }
            ShortCircuit::FilterSignalRequestedButAbsent { producer_role, .. } => format!(
                "No filter output from '{producer_role}' this cycle. Ensure the upstream filter ran and emitted a FilterSignal, or remove the dependency.",
            ),
            ShortCircuit::StrategyReferencesUnattachedSlot { role, .. } => format!(
                "Attach a slot fulfilling '{role}' to the referenced agent, or remove the reference from the strategy.",
            ),
            ShortCircuit::DashboardArtifactWithoutPersistedRow { action, .. } => format!(
                "The '{action}' action did not persist. Retry; if it keeps failing, check the API server logs — a persistence failure is being masked as success.",
            ),
        }
    }

    /// Map this short-circuit into the matching typed-error event payload,
    /// so the event stream records the failed prerequisite (never a silent
    /// success).
    ///
    /// Mapping rationale:
    /// * `MissingTool` / `DisabledTool` → [`UnifiedPayload::ErrorMissingTool`]
    ///   (both are "the tool you need isn't usable").
    /// * `ProviderUnavailable` → [`UnifiedPayload::ErrorProviderUnavailable`].
    /// * `MissingPrompt`, `FilterSignalRequestedButAbsent`,
    ///   `StrategyReferencesUnattachedSlot` → [`UnifiedPayload::ErrorMissingCapability`]
    ///   (a required capability prerequisite — prompt / upstream signal /
    ///   attached slot — is absent).
    /// * `InvalidOutputSchema`, `StaleOptimizedPrompt` →
    ///   [`UnifiedPayload::ErrorInvalidSchema`] (output / prompt shape does
    ///   not match the expected schema/signature).
    /// * `EmptyDemoSet` → [`UnifiedPayload::ErrorPolicyDenied`] (a launch
    ///   precondition for optimization is not met).
    /// * `DashboardArtifactWithoutPersistedRow` →
    ///   [`UnifiedPayload::ErrorPersistenceFailed`].
    pub fn to_typed_error(&self) -> UnifiedPayload {
        let err = TypedError {
            code: self.code().to_string(),
            message: self.to_string(),
            remediation: Some(self.remediation()),
        };
        match self {
            ShortCircuit::MissingTool { .. } | ShortCircuit::DisabledTool { .. } => {
                UnifiedPayload::ErrorMissingTool(err)
            }
            ShortCircuit::ProviderUnavailable { .. } => UnifiedPayload::ErrorProviderUnavailable(err),
            ShortCircuit::MissingPrompt { .. }
            | ShortCircuit::FilterSignalRequestedButAbsent { .. }
            | ShortCircuit::StrategyReferencesUnattachedSlot { .. } => {
                UnifiedPayload::ErrorMissingCapability(err)
            }
            ShortCircuit::InvalidOutputSchema { .. } | ShortCircuit::StaleOptimizedPrompt { .. } => {
                UnifiedPayload::ErrorInvalidSchema(err)
            }
            ShortCircuit::EmptyDemoSet { .. } => UnifiedPayload::ErrorPolicyDenied(err),
            ShortCircuit::DashboardArtifactWithoutPersistedRow { .. } => {
                UnifiedPayload::ErrorPersistenceFailed(err)
            }
        }
    }

    /// Render a machine-readable JSON object for CLI `--json` output:
    /// `{ "short_circuit": <code>, "message": ..., "remediation": ...,
    ///   "event_kind": <typed-error payload kind> }`.
    ///
    /// `event_kind` is the snake_case discriminant of the typed-error
    /// payload this maps to, so a CLI consumer can correlate the exit-code
    /// JSON with the event it will see on the stream.
    pub fn to_cli_json(&self) -> serde_json::Value {
        serde_json::json!({
            "short_circuit": self.code(),
            "message": self.to_string(),
            "remediation": self.remediation(),
            "event_kind": typed_error_event_kind(&self.to_typed_error()),
        })
    }
}

/// The snake_case event-kind name for a typed-error payload. Mirrors the
/// `UnifiedEvent::event_name` discriminant for the `Error*` variants so the
/// CLI JSON and the event stream agree.
fn typed_error_event_kind(p: &UnifiedPayload) -> &'static str {
    match p {
        UnifiedPayload::ErrorMissingCapability(_) => "error_missing_capability",
        UnifiedPayload::ErrorMissingTool(_) => "error_missing_tool",
        UnifiedPayload::ErrorInvalidSchema(_) => "error_invalid_schema",
        UnifiedPayload::ErrorProviderUnavailable(_) => "error_provider_unavailable",
        UnifiedPayload::ErrorPolicyDenied(_) => "error_policy_denied",
        UnifiedPayload::ErrorPersistenceFailed(_) => "error_persistence_failed",
        // to_typed_error only ever produces the six Error* payloads above.
        _ => "error_unknown",
    }
}

// ---------------------------------------------------------------------------
// Detectors. Each `check_*` is a pure function over already-loaded inputs.
// The conductor calls these at the wire sites documented per-function; this
// module never reaches into the live loop itself.
// ---------------------------------------------------------------------------

/// Detect a missing required tool: `tool` is needed by `role` but does not
/// appear in `available_tools` (the union of built-ins + manifest
/// `required_tools` + slot grants the caller assembles).
///
/// Wire into: the eval-launch preflight / `dispatch_capability` tool-bind
/// step (alongside `diagnostics::required_tools_for`).
pub fn check_missing_tool(role: &str, tool: &str, available_tools: &[String]) -> Result<(), ShortCircuit> {
    if available_tools.iter().any(|t| t == tool) {
        Ok(())
    } else {
        Err(ShortCircuit::MissingTool {
            role: role.to_string(),
            tool: tool.to_string(),
        })
    }
}

/// Detect a disabled required tool: `tool` is registered (`is_registered`)
/// but not enabled (`is_enabled == false`) for this run. Order matters at
/// the call site — check `check_missing_tool` first, then this for tools
/// that exist but are switched off.
///
/// Wire into: the server-side tool-policy check (Phase 2.3) when a required
/// tool resolves to a `Denied`/disabled policy outcome.
pub fn check_tool_enabled(
    role: &str,
    tool: &str,
    is_registered: bool,
    is_enabled: bool,
) -> Result<(), ShortCircuit> {
    if is_registered && !is_enabled {
        Err(ShortCircuit::DisabledTool {
            role: role.to_string(),
            tool: tool.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Detect an unavailable provider: the `provider` bound to `role` is not in
/// the `available_providers` set (no credentials / not enabled).
///
/// Wire into: the eval-launch preflight, after resolving each slot's
/// provider against the enabled-provider list (`settings::providers`).
pub fn check_provider_available(
    role: &str,
    provider: &str,
    available_providers: &[String],
) -> Result<(), ShortCircuit> {
    if available_providers.iter().any(|p| p == provider) {
        Ok(())
    } else {
        Err(ShortCircuit::ProviderUnavailable {
            role: role.to_string(),
            provider: provider.to_string(),
        })
    }
}

/// Detect a missing prompt: the slot's `system_prompt` is empty or
/// whitespace-only.
///
/// Wire into: the eval-launch preflight as a hard short-circuit at
/// dispatch time.
pub fn check_prompt_present(role: &str, system_prompt: &str) -> Result<(), ShortCircuit> {
    if system_prompt.trim().is_empty() {
        Err(ShortCircuit::MissingPrompt {
            role: role.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Detect invalid output schema. `valid` is the caller's
/// already-computed validation verdict (the engine has schema validators
/// in the agent-recovery path); when `false`, this records the typed
/// short-circuit. `expected` names the schema, `detail` is the validation
/// failure message.
///
/// Wire into: `agent::execute` schema-recovery after exhausting repair
/// attempts (instead of silently degrading to a noop decision).
pub fn check_output_schema(
    role: &str,
    expected: &str,
    valid: bool,
    detail: &str,
) -> Result<(), ShortCircuit> {
    if valid {
        Ok(())
    } else {
        Err(ShortCircuit::InvalidOutputSchema {
            role: role.to_string(),
            expected: expected.to_string(),
            detail: detail.to_string(),
        })
    }
}

/// Detect an empty demo set when demos are required. `demos_required` is
/// true for capabilities/optimizers that need demonstrations; `demo_count`
/// is the size of the assembled demo set.
///
/// Wire into: the optimization launch path, before kicking off the
/// optimizer (the route the conductor wires; NOT touched here).
pub fn check_demo_set_nonempty(
    capability: &str,
    demos_required: bool,
    demo_count: usize,
) -> Result<(), ShortCircuit> {
    if demos_required && demo_count == 0 {
        Err(ShortCircuit::EmptyDemoSet {
            capability: capability.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Detect a stale optimized prompt: the accepted snapshot's
/// `signature_hash` does not match the current bound signature.
///
/// Reads two already-computed hashes (the snapshot's from
/// `OptimizationSnapshot`/`OptimizationStore`, the current one from the
/// live signature binding). No `xvision-dspy` dependency — both are
/// opaque strings.
///
/// Wire into: the apply-optimized-prompt step (swap-to-child / mint),
/// before writing the prompt onto the slot.
pub fn check_optimized_prompt_fresh(
    role: &str,
    snapshot_signature_hash: &str,
    current_signature_hash: &str,
) -> Result<(), ShortCircuit> {
    if snapshot_signature_hash == current_signature_hash {
        Ok(())
    } else {
        Err(ShortCircuit::StaleOptimizedPrompt {
            role: role.to_string(),
            snapshot_signature_hash: snapshot_signature_hash.to_string(),
            current_signature_hash: current_signature_hash.to_string(),
        })
    }
}

/// Detect a requested-but-absent filter signal: `consumer_role` depends on
/// a `FilterSignal` from `producer_role`, but `producer_emitted` is false
/// for this cycle.
///
/// Wire into: `agent::pipeline` edge resolution, where a downstream
/// position reads `filter_signals[producer_role]` — instead of skipping
/// the edge silently when the upstream produced nothing.
pub fn check_filter_signal_present(
    consumer_role: &str,
    producer_role: &str,
    producer_emitted: bool,
) -> Result<(), ShortCircuit> {
    if producer_emitted {
        Ok(())
    } else {
        Err(ShortCircuit::FilterSignalRequestedButAbsent {
            consumer_role: consumer_role.to_string(),
            producer_role: producer_role.to_string(),
        })
    }
}

/// Detect a strategy reference to an unattached slot: the `AgentRef` for
/// `role`/`agent_id` resolves to an agent that has no slot fulfilling the
/// role. `slot_attached` is the caller's already-computed resolution
/// result (e.g. `diagnostics::slot_for_role(...).is_some()`).
///
/// Wire into: the eval-launch preflight, after agent resolution (mirrors
/// the `agent_resolved=false` / no-slot diagnostics blockers, but as a
/// hard short-circuit).
pub fn check_slot_attached(role: &str, agent_id: &str, slot_attached: bool) -> Result<(), ShortCircuit> {
    if slot_attached {
        Ok(())
    } else {
        Err(ShortCircuit::StrategyReferencesUnattachedSlot {
            role: role.to_string(),
            agent_id: agent_id.to_string(),
        })
    }
}

/// Detect a dashboard action that produced only a UI artifact and no
/// persisted row. `row_persisted` is the caller's verification that the
/// write landed (e.g. the insert returned a row id / affected-rows > 0).
///
/// Wire into: dashboard mutation handlers (strategy/agent/scenario create
/// + edit + delete) after the persist step — assert a row exists before
/// reporting success to the rail.
pub fn check_artifact_persisted(
    action: &str,
    artifact_kind: &str,
    row_persisted: bool,
) -> Result<(), ShortCircuit> {
    if row_persisted {
        Ok(())
    } else {
        Err(ShortCircuit::DashboardArtifactWithoutPersistedRow {
            action: action.to_string(),
            artifact_kind: artifact_kind.to_string(),
        })
    }
}

/// All ten short-circuit codes, in spec order. Exposed so the CLI/UI can
/// enumerate the guardrail classes and a test can assert completeness +
/// distinctness.
pub const SHORT_CIRCUIT_CODES: &[&str] = &[
    "missing_tool",
    "disabled_tool",
    "provider_unavailable",
    "missing_prompt",
    "invalid_output_schema",
    "empty_demo_set",
    "stale_optimized_prompt",
    "filter_signal_requested_but_absent",
    "strategy_references_unattached_slot",
    "dashboard_artifact_without_persisted_row",
];

/// Serializable summary of a short-circuit for embedding in CLI output or
/// an API error body. A thin owned mirror of [`ShortCircuit::to_cli_json`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortCircuitReport {
    pub short_circuit: String,
    pub message: String,
    pub remediation: String,
    pub event_kind: String,
}

impl From<&ShortCircuit> for ShortCircuitReport {
    fn from(sc: &ShortCircuit) -> Self {
        ShortCircuitReport {
            short_circuit: sc.code().to_string(),
            message: sc.to_string(),
            remediation: sc.remediation(),
            event_kind: typed_error_event_kind(&sc.to_typed_error()).to_string(),
        }
    }
}

#[cfg(test)]
mod tests;
