// Diagnostics API — wraps the dashboard's read-only diagnostics surface
// (Phase 4.5):
//   GET /api/strategy/:id/diagnostics
//   GET /api/agents/:id/diagnostics
// See: crates/xvision-dashboard/src/routes/diagnostics.rs
//
// dspy-free: the engine diagnostics module itself is dspy-free (the
// optimizable-capability set is a hardcoded mirror). The frontend renders
// the typed statuses verbatim; it never reaches a dspy type.

import { apiFetch } from "./client";
import type { Capability } from "./agents";

/// Typed status of a single capability position. Internally-tagged on
/// `kind` (serde `#[serde(tag = "kind")]`). `MissingTool` carries the
/// missing tool name. Mirrors `xvision_engine::diagnostics::CapabilityStatus`.
export type CapabilityStatus =
  | { kind: "ready" }
  | { kind: "missing_prompt" }
  | { kind: "missing_model_binding" }
  | { kind: "missing_tool"; tool: string }
  | { kind: "unsupported" }
  | { kind: "optimizable" }
  | { kind: "optional" };

/// The four hard launch blockers. `ready` / `optimizable` / `optional`
/// never block.
export function isBlocker(status: CapabilityStatus): boolean {
  return (
    status.kind === "missing_prompt" ||
    status.kind === "missing_model_binding" ||
    status.kind === "missing_tool" ||
    status.kind === "unsupported"
  );
}

/// Operator-facing remediation copy for a blocking status. Empty for the
/// non-blocking states (the caller shows a positive badge instead).
export function remediationFor(
  status: CapabilityStatus,
  capability: Capability,
): string {
  switch (status.kind) {
    case "missing_prompt":
      return `Add a system prompt to the ${capability} slot — it is currently empty.`;
    case "missing_model_binding":
      return `Bind a provider and model to the ${capability} slot.`;
    case "missing_tool":
      return `Grant the "${status.tool}" tool: add it to this strategy's required tools.`;
    case "unsupported":
      return `The "${capability}" capability has no runtime handler yet — it cannot launch.`;
    case "ready":
    case "optimizable":
    case "optional":
      return "";
  }
}

/// Short human label for a status, for badge text.
export function statusLabel(status: CapabilityStatus): string {
  switch (status.kind) {
    case "ready":
      return "Ready";
    case "missing_prompt":
      return "No prompt";
    case "missing_model_binding":
      return "No model";
    case "missing_tool":
      return "Missing tool";
    case "unsupported":
      return "Unsupported";
    case "optimizable":
      return "Optimizable";
    case "optional":
      return "Optional";
  }
}

// ── strategy-level diagnostics (engine StrategyDiagnostics verbatim) ──────

/// Per-capability diagnostic line inside an `AgentDiagnostics`. Mirrors
/// `xvision_engine::diagnostics::CapabilityDiagnostic`.
export type CapabilityDiagnostic = {
  capability: Capability;
  status: CapabilityStatus;
  required: boolean;
  required_tools: string[];
  optimizable: boolean;
};

/// Diagnostics for a single agent position in a strategy. Mirrors
/// `xvision_engine::diagnostics::AgentDiagnostics`.
export type StrategyAgentDiagnostics = {
  role: string;
  agent_id: string;
  agent_name: string | null;
  agent_resolved: boolean;
  declared: Capability[];
  required: Capability | null;
  capabilities: CapabilityDiagnostic[];
};

/// One unmet required capability — a launch blocker. Mirrors
/// `xvision_engine::diagnostics::UnmetRequirement`.
export type UnmetRequirement = {
  role: string;
  agent_id: string;
  capability: Capability;
  status: CapabilityStatus;
};

/// Aggregated capability-completeness diagnostics for a strategy. Mirrors
/// `xvision_engine::diagnostics::StrategyDiagnostics`.
export type StrategyDiagnostics = {
  strategy_id: string;
  per_agent: StrategyAgentDiagnostics[];
  required_capabilities: Capability[];
  required_unmet: UnmetRequirement[];
  optimizable: Capability[];
  launchable: boolean;
};

// ── agent-level diagnostics (dashboard-composed) ──────────────────────────

/// Per-capability line in the agent-level view. Mirrors the dashboard's
/// `AgentCapabilityLine`.
export type AgentCapabilityLine = {
  capability: Capability;
  status: CapabilityStatus;
  required_tools: string[];
  optimizable: boolean;
};

/// Per-slot diagnostics for a single agent. Mirrors the dashboard's
/// `AgentSlotDiagnostics`.
export type AgentSlotDiagnostics = {
  slot_name: string;
  model_bound: boolean;
  prompt_present: boolean;
  declared: Capability[];
  capabilities: AgentCapabilityLine[];
};

/// Agent-level diagnostics response. Mirrors the dashboard's
/// `AgentDiagnosticsResponse`.
export type AgentDiagnostics = {
  agent_id: string;
  agent_name: string;
  slots: AgentSlotDiagnostics[];
  declared_capabilities: Capability[];
  optimizable_capabilities: Capability[];
  agent_ready: boolean;
};

/// Fetch capability-readiness diagnostics for a strategy. Surfaces WHY a
/// strategy can't launch (typed per-agent blockers) BEFORE launch.
export async function getStrategyDiagnostics(
  strategyId: string,
): Promise<StrategyDiagnostics> {
  return apiFetch<StrategyDiagnostics>(
    `/api/strategy/${encodeURIComponent(strategyId)}/diagnostics`,
  );
}

/// Fetch per-slot capability diagnostics for a single library agent.
export async function getAgentDiagnostics(
  agentId: string,
): Promise<AgentDiagnostics> {
  return apiFetch<AgentDiagnostics>(
    `/api/agents/${encodeURIComponent(agentId)}/diagnostics`,
  );
}

export const diagnosticsKeys = {
  all: ["diagnostics"] as const,
  strategy: (id: string) =>
    [...diagnosticsKeys.all, "strategy", id] as const,
  agent: (id: string) => [...diagnosticsKeys.all, "agent", id] as const,
};
