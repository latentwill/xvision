//! Capability-completeness diagnostics for strategy-agents (Phase 4.1).
//!
//! Given a strategy id, [`capability_diagnostics`] loads the strategy and
//! every agent it references, then answers — as **typed** statuses, never
//! free-text warnings — whether each capability position in the pipeline
//! is launchable:
//!
//! * which capabilities each agent/slot *declares*
//!   ([`AgentDiagnostics::declared`]),
//! * which capability each pipeline position *requires*
//!   ([`AgentDiagnostics::required`]),
//! * which tools that required capability *needs*
//!   ([`required_tools_for`]) and whether the slot+manifest grant them,
//! * which prompts / demos / schemas / model bindings / memory inputs /
//!   filters / data sources are MISSING (each a [`CapabilityStatus`]
//!   variant),
//! * which capabilities can be optimized *now* — they have a signature in
//!   xvision-dspy's capability set ([`OPTIMIZABLE_CAPABILITIES`]),
//! * which are intentionally unsupported by the current runtime
//!   ([`CapabilityStatus::Unsupported`]).
//!
//! The whole result aggregates into [`StrategyDiagnostics`], whose
//! [`StrategyDiagnostics::launchable`] flag is the single launch gate.
//! [`assert_launchable`] turns a non-launchable result into a typed
//! [`DiagnosticsError`] listing the unmet *required* requirements;
//! OPTIONAL capabilities never block.
//!
//! HARD INVARIANT: this module does **not** depend on `xvision-dspy`. The
//! optimizable capability set is hardcoded as [`OPTIMIZABLE_CAPABILITIES`]
//! with a comment pointing at the dspy registry it mirrors. The engine
//! stays dspy-free (see `ApiContext::open`'s migration-045 comment).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agents::{Agent, AgentSlot, Capability};
use crate::api::{agents as agents_api, strategy as strategy_api, ApiContext, ApiError};
use crate::strategies::agent_ref::{canonical_role, AgentRef};
use crate::strategies::Strategy;

/// Capabilities that have an implemented optimizer signature in
/// `xvision-dspy` *today*. Mirrors `xvision_dspy::capability::Capability::
/// has_optimizer`, which returns `true` only for `Trader` and `Filter`
/// (the remaining dspy capabilities — `DecisionGrader`, `Intern`,
/// `ChatAuthoring` — are declared stubs that fail with
/// `OptimizerError::MissingCapabilityOptimizer`).
///
/// HARD INVARIANT: this is a hand-maintained mirror, NOT an import. The
/// engine must not depend on `xvision-dspy` (offline-isolation invariant).
/// If the dspy registry's optimizable set drifts, update this const by
/// hand — there is deliberately no compile-time coupling. See
/// `crates/xvision-dspy/src/capability.rs` (`has_optimizer`) and
/// `crates/xvision-dspy/src/signatures.rs` (`signature_for`).
pub const OPTIMIZABLE_CAPABILITIES: &[Capability] = &[Capability::Trader, Capability::Filter];

/// Whether `cap` has a dspy optimizer signature today. Pure lookup over
/// [`OPTIMIZABLE_CAPABILITIES`].
pub fn is_optimizable(cap: Capability) -> bool {
    OPTIMIZABLE_CAPABILITIES.contains(&cap)
}

/// Tools a capability needs at runtime to do its job. The runtime grants
/// a tool to a slot only when the slot's `allowed_tools` (legacy slot
/// path) or the strategy manifest's `required_tools` declares it — a
/// required tool that appears nowhere is a [`CapabilityStatus::MissingTool`].
///
/// These names match the built-in [`crate::tools::ToolRegistry`] entries
/// (`ohlcv`, `indicator_panel`). The set is intentionally small: a Trader
/// needs price data to decide; a Filter needs the indicator panel it
/// gates on; the rest are advisory and require no tool to function.
pub fn required_tools_for(cap: Capability) -> &'static [&'static str] {
    match cap {
        // A trader must be able to read price action to produce a
        // TraderDecision.
        Capability::Trader => &["ohlcv"],
        // A filter scores a candidate signal off the indicator panel.
        Capability::Filter => &["indicator_panel"],
        // Critic inspects upstream output; Intern gathers context from the
        // briefing it's handed; Router picks a branch off a FilterSignal —
        // none require a registered tool.
        Capability::Critic | Capability::Intern | Capability::Router => &[],
    }
}

/// Whether a capability is supported by the current runtime dispatcher.
///
/// Phase A of the capability-first agent model spec persists all five
/// capability shapes but only `Trader` and `Filter` have a live runtime
/// handler today (the unified `dispatch_capability` seam lands the rest in
/// Phase B). `Critic`, `Intern`, and `Router` are therefore reported as
/// [`CapabilityStatus::Unsupported`] when a strategy *requires* them —
/// they cannot be launched even though they persist cleanly.
///
/// See `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`.
pub fn is_runtime_supported(cap: Capability) -> bool {
    matches!(cap, Capability::Trader | Capability::Filter)
}

/// Typed status of a single capability position in a strategy pipeline.
///
/// Exactly one status is computed per `(agent position, required
/// capability)`. The variants are ordered by precedence in
/// [`compute_status`]: a hard blocker (missing prompt / model / tool /
/// unsupported) wins over the softer `Optimizable` / `Optional` / `Ready`
/// states, so the most actionable reason surfaces first.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Every requirement for this capability is satisfied — it can launch.
    Ready,
    /// The slot fulfilling this capability has an empty/whitespace-only
    /// `system_prompt`. Hard blocker.
    MissingPrompt,
    /// The slot fulfilling this capability has no provider+model binding
    /// (empty provider or empty model). Hard blocker.
    MissingModelBinding,
    /// A tool the capability requires (see [`required_tools_for`]) is not
    /// granted by the slot's `allowed_tools` or the manifest's
    /// `required_tools`. Hard blocker. Carries the missing tool name.
    ///
    /// Modeled as a struct variant (`{ tool }`) rather than a tuple
    /// newtype so it round-trips under serde's internal tagging
    /// (`#[serde(tag = "kind")]`), which cannot serialize a tagged
    /// newtype wrapping a primitive.
    MissingTool { tool: String },
    /// The capability's runtime handler does not exist yet (Phase A
    /// persists the shape; Phase B implements the dispatcher). Hard
    /// blocker for launch but distinct from a misconfiguration.
    Unsupported,
    /// This capability is satisfied AND has a dspy optimizer signature
    /// (`Trader` / `Filter`). Informational — not a blocker. Surfaced so
    /// the operator knows the position is a candidate for `xvn optimize`.
    Optimizable,
    /// The capability is declared by the agent but is NOT a required
    /// position in this strategy's pipeline (e.g. a multi-capability
    /// agent referenced for only one of its capabilities). Never blocks.
    Optional,
}

impl CapabilityStatus {
    /// Whether this status is a hard launch blocker. `Ready`,
    /// `Optimizable`, and `Optional` do not block; everything else does.
    pub fn is_blocker(&self) -> bool {
        matches!(
            self,
            CapabilityStatus::MissingPrompt
                | CapabilityStatus::MissingModelBinding
                | CapabilityStatus::MissingTool { .. }
                | CapabilityStatus::Unsupported
        )
    }

    /// Stable machine-readable code for the status, for `--json` consumers
    /// that want to branch without matching the serde tag string.
    pub fn code(&self) -> &'static str {
        match self {
            CapabilityStatus::Ready => "ready",
            CapabilityStatus::MissingPrompt => "missing_prompt",
            CapabilityStatus::MissingModelBinding => "missing_model_binding",
            CapabilityStatus::MissingTool { .. } => "missing_tool",
            CapabilityStatus::Unsupported => "unsupported",
            CapabilityStatus::Optimizable => "optimizable",
            CapabilityStatus::Optional => "optional",
        }
    }
}

/// Per-capability diagnostic line inside an [`AgentDiagnostics`].
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDiagnostic {
    /// The capability this line is about.
    pub capability: Capability,
    /// Typed status for this capability at this pipeline position.
    pub status: CapabilityStatus,
    /// Whether the strategy pipeline *requires* this capability at this
    /// position (`true`) or it's a declared-but-unused capability
    /// (`false`). Only required + blocking statuses gate launch.
    pub required: bool,
    /// Tools this capability requires (see [`required_tools_for`]). Empty
    /// for capabilities that need no tool.
    pub required_tools: Vec<String>,
    /// Whether this capability has a dspy optimizer signature today.
    pub optimizable: bool,
}

/// Diagnostics for a single agent position in a strategy.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDiagnostics {
    /// The strategy-side role this agent plays (`AgentRef.role`).
    pub role: String,
    /// The referenced agent's id.
    pub agent_id: String,
    /// The agent's display name, if the agent record resolved.
    pub agent_name: Option<String>,
    /// `false` when the `AgentRef.agent_id` does not resolve to an agent
    /// record in the workspace library — a hard blocker on its own.
    pub agent_resolved: bool,
    /// Capabilities the agent's matching slot *declares*
    /// (`AgentSlot.capabilities`). Empty when the agent didn't resolve.
    pub declared: Vec<Capability>,
    /// The capability this pipeline position *requires* — `AgentRef.
    /// activates` if set, else the slot's first declared capability
    /// (`Trader` for legacy slots). `None` when the agent didn't resolve.
    pub required: Option<Capability>,
    /// One diagnostic line per declared capability (plus the required one,
    /// if it isn't declared).
    pub capabilities: Vec<CapabilityDiagnostic>,
}

/// One unmet *required* capability, surfaced in
/// [`StrategyDiagnostics::required_unmet`] and the typed
/// [`DiagnosticsError`]. This is the structured form of "why is this
/// strategy not launchable."
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmetRequirement {
    /// Role of the agent position whose required capability is unmet.
    pub role: String,
    /// The agent id at that position.
    pub agent_id: String,
    /// The required capability that is unmet.
    pub capability: Capability,
    /// The typed blocking status.
    pub status: CapabilityStatus,
}

/// Aggregated capability-completeness diagnostics for a whole strategy.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiagnostics {
    /// The strategy id these diagnostics cover.
    pub strategy_id: String,
    /// Per-agent diagnostics, in pipeline order (`Strategy.agents` order).
    pub per_agent: Vec<AgentDiagnostics>,
    /// Capabilities the strategy graph requires across all positions,
    /// de-duplicated and sorted (for a quick "what does this strategy
    /// need" summary).
    pub required_capabilities: Vec<Capability>,
    /// Required capabilities that are NOT met — the launch blockers. Empty
    /// iff the strategy is launchable.
    pub required_unmet: Vec<UnmetRequirement>,
    /// Required capabilities that ARE met and have a dspy optimizer
    /// signature — candidates for `xvn optimize`.
    pub optimizable: Vec<Capability>,
    /// `true` iff `required_unmet` is empty AND the strategy has at least
    /// one agent. A strategy with zero agents is not launchable.
    pub launchable: bool,
}

/// Typed error returned by [`assert_launchable`] when a required
/// capability is unmet. Lists every unmet requirement so the caller can
/// render all blockers at once rather than one-at-a-time.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DiagnosticsError {
    /// One or more required capabilities are unmet.
    #[error("strategy '{strategy_id}' is not launchable: {} unmet required capabilit{} ({summary})", unmet.len(), if unmet.len() == 1 { "y" } else { "ies" })]
    NotLaunchable {
        strategy_id: String,
        unmet: Vec<UnmetRequirement>,
        /// Human-readable one-line summary of the unmet requirements,
        /// precomputed so the `Display` impl stays cheap. Each entry is
        /// `role:capability=status_code`.
        summary: String,
    },
    /// The strategy has no agents at all — nothing to launch.
    #[error("strategy '{0}' has no agents — nothing to launch")]
    NoAgents(String),
}

/// Launch gate: fail when a REQUIRED capability is missing. OPTIONAL
/// capabilities never block. The returned error lists every unmet
/// requirement.
///
/// This is the callable gate the conductor wires into the eval-launch
/// path (see the contract's "what the conductor must wire" note). It does
/// not call the launch path itself — it is a pure check over an
/// already-computed [`StrategyDiagnostics`].
pub fn assert_launchable(diag: &StrategyDiagnostics) -> Result<(), DiagnosticsError> {
    if diag.per_agent.is_empty() {
        return Err(DiagnosticsError::NoAgents(diag.strategy_id.clone()));
    }
    if diag.required_unmet.is_empty() {
        return Ok(());
    }
    let summary = diag
        .required_unmet
        .iter()
        .map(|u| format!("{}:{}={}", u.role, capability_key(u.capability), u.status.code()))
        .collect::<Vec<_>>()
        .join(", ");
    Err(DiagnosticsError::NotLaunchable {
        strategy_id: diag.strategy_id.clone(),
        unmet: diag.required_unmet.clone(),
        summary,
    })
}

/// Stable lowercase key for a capability — matches the serde wire form.
fn capability_key(cap: Capability) -> &'static str {
    match cap {
        Capability::Trader => "trader",
        Capability::Filter => "filter",
        Capability::Critic => "critic",
        Capability::Intern => "intern",
        Capability::Router => "router",
    }
}

/// Pick the slot inside `agent` that fulfils `role`. v1 agents are
/// single-slot, so we take the first slot; if a future multi-slot agent
/// has a slot named exactly `role`, prefer it.
fn slot_for_role<'a>(agent: &'a Agent, role: &str) -> Option<&'a AgentSlot> {
    let canon = canonical_role(role);
    agent
        .slots
        .iter()
        .find(|s| canonical_role(&s.name) == canon)
        .or_else(|| agent.slots.first())
}

/// The capability a pipeline position requires: `AgentRef.activates` if
/// set, otherwise the slot's first declared capability in `BTreeSet`
/// order (`Trader` for every legacy/pre-033 slot — spec Decision 2).
fn required_capability(agent_ref: &AgentRef, slot: &AgentSlot) -> Capability {
    agent_ref
        .activates
        .or_else(|| slot.capabilities.iter().next().copied())
        .unwrap_or(Capability::Trader)
}

/// Whether the slot+manifest grant `tool` to the capability. A tool is
/// granted if it appears in the slot's `allowed_tools` (legacy slot path —
/// agents v1 slots don't carry `allowed_tools`, so this is the manifest
/// path in practice) or the strategy manifest's `required_tools`.
fn tool_granted(strategy: &Strategy, tool: &str) -> bool {
    crate::tools::ToolRegistry::default_with_builtins()
        .list()
        .into_iter()
        .any(|t| t.as_str() == tool)
        || strategy.manifest.required_tools.iter().any(|t| t == tool)
}

/// Compute the typed status for `cap` at this position. Precedence:
/// missing prompt → missing model → unsupported (required only) → missing
/// tool → optimizable → ready. For a declared-but-not-required capability
/// the status collapses to [`CapabilityStatus::Optional`].
fn compute_status(
    strategy: &Strategy,
    slot: &AgentSlot,
    cap: Capability,
    required: bool,
) -> CapabilityStatus {
    // A declared-but-unused capability never blocks and never needs a
    // readiness verdict — it's simply optional in this strategy.
    if !required {
        return CapabilityStatus::Optional;
    }

    // Hard blockers first, most actionable wins.
    if slot.system_prompt.trim().is_empty() {
        return CapabilityStatus::MissingPrompt;
    }
    if slot.provider.trim().is_empty() || slot.model.trim().is_empty() {
        return CapabilityStatus::MissingModelBinding;
    }
    if !is_runtime_supported(cap) {
        return CapabilityStatus::Unsupported;
    }
    for tool in required_tools_for(cap) {
        if !tool_granted(strategy, tool) {
            return CapabilityStatus::MissingTool {
                tool: (*tool).to_string(),
            };
        }
    }

    // Satisfied. Flag optimizable positions so the operator sees the
    // `xvn optimize` opportunity; otherwise plain Ready.
    if is_optimizable(cap) {
        CapabilityStatus::Optimizable
    } else {
        CapabilityStatus::Ready
    }
}

/// Build the per-agent diagnostics for one `AgentRef`, resolving its
/// agent record from `agents`.
fn diagnose_agent(strategy: &Strategy, agent_ref: &AgentRef, agent: Option<&Agent>) -> AgentDiagnostics {
    let role = agent_ref.role.clone();
    let agent_id = agent_ref.agent_id.clone();

    let Some(agent) = agent else {
        // Unresolved agent ref: a hard blocker. Treat the activated
        // capability (or Trader) as required + unmet via MissingModelBinding
        // — there's no slot, so no prompt/model/tools exist.
        let required = agent_ref.activates.unwrap_or(Capability::Trader);
        return AgentDiagnostics {
            role,
            agent_id,
            agent_name: None,
            agent_resolved: false,
            declared: Vec::new(),
            required: Some(required),
            capabilities: vec![CapabilityDiagnostic {
                capability: required,
                status: CapabilityStatus::MissingModelBinding,
                required: true,
                required_tools: required_tools_for(required)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                optimizable: is_optimizable(required),
            }],
        };
    };

    let slot = slot_for_role(agent, &role);
    let Some(slot) = slot else {
        // Agent record with no slots — pathological but possible. Same
        // treatment as an unresolved ref.
        let required = agent_ref.activates.unwrap_or(Capability::Trader);
        return AgentDiagnostics {
            role,
            agent_id,
            agent_name: Some(agent.name.clone()),
            agent_resolved: true,
            declared: Vec::new(),
            required: Some(required),
            capabilities: vec![CapabilityDiagnostic {
                capability: required,
                status: CapabilityStatus::MissingModelBinding,
                required: true,
                required_tools: required_tools_for(required)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                optimizable: is_optimizable(required),
            }],
        };
    };

    let required_cap = required_capability(agent_ref, slot);
    let declared: Vec<Capability> = slot.capabilities.iter().copied().collect();

    // Emit one diagnostic per declared capability, plus the required
    // capability if the slot didn't declare it (a real misconfiguration
    // we still want surfaced as required + blocking).
    let mut caps: BTreeSet<Capability> = slot.capabilities.iter().copied().collect();
    caps.insert(required_cap);

    let capabilities: Vec<CapabilityDiagnostic> = caps
        .into_iter()
        .map(|cap| {
            let is_required = cap == required_cap;
            CapabilityDiagnostic {
                capability: cap,
                status: compute_status(strategy, slot, cap, is_required),
                required: is_required,
                required_tools: required_tools_for(cap).iter().map(|s| s.to_string()).collect(),
                optimizable: is_optimizable(cap),
            }
        })
        .collect();

    AgentDiagnostics {
        role,
        agent_id,
        agent_name: Some(agent.name.clone()),
        agent_resolved: true,
        declared,
        required: Some(required_cap),
        capabilities,
    }
}

/// Pure core: compute diagnostics from an already-loaded strategy + the
/// agent records it references. Kept separate from
/// [`capability_diagnostics`] so it can be unit-tested without an
/// `ApiContext` / SQLite.
///
/// `agents` is looked up by `agent_id`; a missing entry yields an
/// unresolved-agent blocker.
pub fn diagnose(strategy: &Strategy, agents: &[Agent]) -> StrategyDiagnostics {
    let strategy_id = strategy.manifest.id.clone();

    let per_agent: Vec<AgentDiagnostics> = strategy
        .agents
        .iter()
        .map(|aref| {
            let agent = agents.iter().find(|a| a.agent_id == aref.agent_id);
            diagnose_agent(strategy, aref, agent)
        })
        .collect();

    // Required capabilities, de-duped + sorted.
    let mut required_capabilities: BTreeSet<Capability> = BTreeSet::new();
    for a in &per_agent {
        if let Some(c) = a.required {
            required_capabilities.insert(c);
        }
    }

    // Unmet required requirements: any required + blocking status.
    let mut required_unmet: Vec<UnmetRequirement> = Vec::new();
    let mut optimizable: BTreeSet<Capability> = BTreeSet::new();
    for a in &per_agent {
        for cd in &a.capabilities {
            if !cd.required {
                continue;
            }
            if cd.status.is_blocker() {
                required_unmet.push(UnmetRequirement {
                    role: a.role.clone(),
                    agent_id: a.agent_id.clone(),
                    capability: cd.capability,
                    status: cd.status.clone(),
                });
            } else if cd.optimizable {
                optimizable.insert(cd.capability);
            }
        }
    }

    let launchable = !per_agent.is_empty() && required_unmet.is_empty();

    StrategyDiagnostics {
        strategy_id,
        per_agent,
        required_capabilities: required_capabilities.into_iter().collect(),
        required_unmet,
        optimizable: optimizable.into_iter().collect(),
        launchable,
    }
}

/// Load a strategy and every agent it references, then compute
/// [`StrategyDiagnostics`].
///
/// Returns `ApiError::NotFound` when the strategy id does not resolve.
/// Unresolved *agent* references inside a resolved strategy are NOT an
/// error — they surface as a per-agent blocker so the diagnostics stay a
/// complete report rather than failing fast on the first dangling ref.
pub async fn capability_diagnostics(
    ctx: &ApiContext,
    strategy_id: &str,
) -> Result<StrategyDiagnostics, ApiError> {
    let strategy: Strategy = strategy_api::get(ctx, strategy_id).await?;

    // Resolve each distinct referenced agent. A dangling ref is tolerated
    // (omitted from the resolved list) so `diagnose` can flag it as a
    // per-agent blocker.
    let mut agents: Vec<Agent> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for aref in &strategy.agents {
        if !seen.insert(aref.agent_id.clone()) {
            continue;
        }
        match agents_api::get(ctx, &aref.agent_id).await {
            Ok(a) => agents.push(a),
            Err(ApiError::NotFound(_)) => { /* dangling ref → per-agent blocker */ }
            Err(e) => return Err(e),
        }
    }

    Ok(diagnose(&strategy, &agents))
}

#[cfg(test)]
mod tests;
