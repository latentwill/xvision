//! `/api/strategy/:id/diagnostics` + `/api/agents/:id/diagnostics` —
//! read-only capability-completeness diagnostics surface (Phase 4.5).
//!
//! ## What this serves
//!
//! - `GET /api/strategy/:id/diagnostics` returns the engine
//!   [`StrategyDiagnostics`] verbatim. It answers — as *typed* statuses,
//!   never free-text — whether every required capability position in the
//!   pipeline is launchable, and surfaces `launchable` as the single
//!   launch gate the UI checks BEFORE offering a launch action.
//!
//! - `GET /api/agents/:id/diagnostics` returns per-slot capability
//!   diagnostics for a single library agent, independent of any strategy.
//!   The engine only exposes a strategy-scoped `capability_diagnostics`
//!   (a slot's tool grants come from the *strategy manifest*, not the
//!   agent), so this route composes the agent-level view from the engine's
//!   public pure helpers
//!   ([`xvision_engine::diagnostics::is_optimizable`] /
//!   [`is_runtime_supported`] / [`required_tools_for`]). It reports, for
//!   each slot and each declared capability, whether the slot itself is
//!   complete (prompt + model binding present) and whether the runtime
//!   supports the capability — the agent-list capability badges and the
//!   agent-detail diagnostics tab consume this. Tool-grant blockers are
//!   deliberately NOT raised here: tools are granted by a strategy's
//!   manifest, so a `MissingTool` verdict only makes sense in the
//!   strategy-scoped view.
//!
//! ## dspy-free invariant
//!
//! The dashboard MUST NOT depend on `xvision-dspy`. The engine diagnostics
//! module is itself dspy-free (the optimizable-capability set is a
//! hardcoded mirror — see `crates/xvision-engine/src/diagnostics/mod.rs`),
//! so this route reaches no dspy types.
//!
//! ## read-only
//!
//! Both handlers are pure reads — they live on the read-only router.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use xvision_engine::agents::{Agent, AgentSlot, Capability};
use xvision_engine::api::agents as agents_api;
use xvision_engine::diagnostics::{
    capability_diagnostics, is_optimizable, is_runtime_supported, required_tools_for, CapabilityStatus,
    StrategyDiagnostics,
};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/strategy/:id/diagnostics`
///
/// Returns the engine [`StrategyDiagnostics`] for the strategy. A
/// dangling agent reference inside the strategy is NOT a 404 — it surfaces
/// as a per-agent blocker in the payload so the readiness panel renders a
/// complete report. Only an unknown *strategy* id 404s (via the engine's
/// typed `NotFound`).
pub async fn strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StrategyDiagnostics>, DashboardError> {
    let diag = capability_diagnostics(&state.api_context(), &id).await?;
    Ok(Json(diag))
}

/// Per-capability diagnostic line for one slot in the agent-level view.
///
/// Mirrors the engine's `CapabilityDiagnostic` shape but is computed
/// agent-scoped (no strategy manifest, so tool grants are not evaluated —
/// `required_tools` is informational here, never a blocker).
#[derive(Debug, Clone, Serialize)]
pub struct AgentCapabilityLine {
    /// The capability this line is about (serde lower-snake, matching the
    /// engine `Capability` wire form).
    pub capability: Capability,
    /// Typed status for this capability at this slot. In the agent-level
    /// view the only blockers raised are `MissingPrompt`,
    /// `MissingModelBinding`, and `Unsupported`; tool grants are a
    /// strategy concern.
    pub status: CapabilityStatus,
    /// Tools the capability would require in a strategy pipeline (advisory
    /// here). Empty for capabilities that need no tool.
    pub required_tools: Vec<String>,
    /// Whether this capability has a dspy optimizer signature today.
    pub optimizable: bool,
}

/// Diagnostics for one slot of an agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentSlotDiagnostics {
    /// The slot name.
    pub slot_name: String,
    /// Provider+model binding present (non-empty provider AND model).
    pub model_bound: bool,
    /// `system_prompt` is non-empty (after trimming).
    pub prompt_present: bool,
    /// Capabilities this slot declares.
    pub declared: Vec<Capability>,
    /// One line per declared capability.
    pub capabilities: Vec<AgentCapabilityLine>,
}

/// `GET /api/agents/:id/diagnostics` response — per-slot capability
/// completeness for a single library agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDiagnosticsResponse {
    /// The agent id these diagnostics cover.
    pub agent_id: String,
    /// The agent display name.
    pub agent_name: String,
    /// Per-slot diagnostics, in slot order.
    pub slots: Vec<AgentSlotDiagnostics>,
    /// All capabilities the agent declares across its slots, de-duplicated
    /// and sorted — a quick "what can this agent do" summary for the
    /// agent-list badges.
    pub declared_capabilities: Vec<Capability>,
    /// Capabilities the agent declares that have a dspy optimizer
    /// signature today (`Trader` / `Filter`).
    pub optimizable_capabilities: Vec<Capability>,
    /// `true` when every declared capability across all slots is
    /// satisfiable at the agent level — each declaring slot has a prompt +
    /// model binding and the capability's runtime handler exists. A
    /// strategy may still refuse to launch (missing tool grants), so this
    /// is an agent-readiness signal, NOT a launch verdict.
    pub agent_ready: bool,
}

/// Compute the agent-level status of `cap` for `slot`. Precedence mirrors
/// the engine's `compute_status`, minus the strategy-manifest tool-grant
/// check (tools are not granted at the agent level): missing prompt →
/// missing model → unsupported → optimizable → ready.
fn slot_capability_status(slot: &AgentSlot, cap: Capability) -> CapabilityStatus {
    if slot.system_prompt.trim().is_empty() {
        return CapabilityStatus::MissingPrompt;
    }
    if slot.provider.trim().is_empty() || slot.model.trim().is_empty() {
        return CapabilityStatus::MissingModelBinding;
    }
    if !is_runtime_supported(cap) {
        return CapabilityStatus::Unsupported;
    }
    if is_optimizable(cap) {
        CapabilityStatus::Optimizable
    } else {
        CapabilityStatus::Ready
    }
}

/// Build the agent-level diagnostics response from a resolved [`Agent`].
fn diagnose_agent_level(agent: &Agent) -> AgentDiagnosticsResponse {
    use std::collections::BTreeSet;

    let mut declared_all: BTreeSet<Capability> = BTreeSet::new();
    let mut optimizable_all: BTreeSet<Capability> = BTreeSet::new();
    let mut agent_ready = !agent.slots.is_empty();

    let slots: Vec<AgentSlotDiagnostics> = agent
        .slots
        .iter()
        .map(|slot| {
            let model_bound = !slot.provider.trim().is_empty() && !slot.model.trim().is_empty();
            let prompt_present = !slot.system_prompt.trim().is_empty();
            let declared: Vec<Capability> = slot.capabilities.iter().copied().collect();

            let capabilities: Vec<AgentCapabilityLine> = slot
                .capabilities
                .iter()
                .copied()
                .map(|cap| {
                    declared_all.insert(cap);
                    if is_optimizable(cap) {
                        optimizable_all.insert(cap);
                    }
                    let status = slot_capability_status(slot, cap);
                    if status.is_blocker() {
                        agent_ready = false;
                    }
                    AgentCapabilityLine {
                        capability: cap,
                        status,
                        required_tools: required_tools_for(cap).iter().map(|s| s.to_string()).collect(),
                        optimizable: is_optimizable(cap),
                    }
                })
                .collect();

            AgentSlotDiagnostics {
                slot_name: slot.name.clone(),
                model_bound,
                prompt_present,
                declared,
                capabilities,
            }
        })
        .collect();

    AgentDiagnosticsResponse {
        agent_id: agent.agent_id.clone(),
        agent_name: agent.name.clone(),
        slots,
        declared_capabilities: declared_all.into_iter().collect(),
        optimizable_capabilities: optimizable_all.into_iter().collect(),
        agent_ready,
    }
}

/// `GET /api/agents/:id/diagnostics`
///
/// Returns per-slot capability diagnostics for the agent. An unknown agent
/// id 404s via the engine's typed `NotFound`.
pub async fn agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentDiagnosticsResponse>, DashboardError> {
    let agent = agents_api::get(&state.api_context(), &id).await?;
    Ok(Json(diagnose_agent_level(&agent)))
}
