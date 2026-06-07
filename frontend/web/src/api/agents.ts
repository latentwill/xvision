// Agents API — wraps `engine::api::agents::*` via the dashboard's
// `/api/agents` surface. See:
//   crates/xvision-dashboard/src/routes/agents.rs
//   docs/superpowers/plans/2026-05-11-agents-page-v1.md

import { apiFetch } from "./client";
import type { MemoryMode } from "./types.gen/MemoryMode";

export type AgentSlot = {
  name: string;
  provider: string;
  model: string;
  system_prompt: string;
  // Forward-compat hook for the v1.1 workspace skill registry
  // (kind = tool | prompt_fragment | evaluator). Picker is hidden in v1
  // until `/agents/skills` ships. Not related to the Plan 2b `xvn skill`
  // surface removed in ADR 0012.
  skill_ids: string[];
  /// `null` means "auto from the selected model" — the engine resolves
  /// the effective budget at dispatch time from the canonical model
  /// metadata table (q15 §1). A number is honored verbatim, clamped to
  /// the model's per-request ceiling server-side.
  max_tokens: number | null;
  /// Optional per-step wall-clock budget for the Cline runtime,
  /// measured in milliseconds. `null` means no enforcement; the
  /// SlotForm UI accepts seconds and converts to this wire unit.
  max_wall_ms?: number | null;
  /// Optional cap on the number of `bar_history` entries the eval
  /// executor surfaces to the trader LLM at each decision. `null`
  /// preserves today's behavior — the full `warmup_bars`-sized history
  /// slice is sent through. A positive integer trims the slice to its
  /// most-recent N entries before the trader sees it, which keeps the
  /// prompt prefix stable across many decisions so provider prompt-
  /// caching (Anthropic) can land a hit on the static portion.
  ///
  /// Server enforces non-positive → null at persist time (migration 025
  /// + store layer) so a stray `0` can't silently drop every bar.
  /// Shipped runner-side via PR #372 (eval-prompt-cache-and-rolling-window).
  bar_history_limit?: number | null;
  /// V2D: cortex-memory mode for this slot. `"off"` (the default)
  /// keeps the dispatcher's memory seam dormant. `"global"` and
  /// `"agent_scoped"` opt this slot into recall + write through
  /// `xvision-memory`. Optional on the wire — the server's
  /// `#[serde(default)]` collapses missing values to `"off"`.
  memory_mode?: MemoryMode;
  allowed_tools: string[];
};

export type Agent = {
  agent_id: string;
  name: string;
  description: string;
  tags: string[];
  slots: AgentSlot[];
  archived: boolean;
  created_at: string;
  updated_at: string;
  /// `undefined` (the default) → workspace agent. A string → this
  /// agent is scoped to that strategy and hidden from the default
  /// workspace list. Migration 036.
  scope_strategy_id?: string | null;
};

type Severity = "Error" | "Warning" | "Info";

export type ValidationDiagnostic = {
  code: string;
  severity: Severity;
  message: string;
  field: string | null;
};

export type StrategyRef = {
  strategy_id: string;
  name: string;
};

export type RunRef = {
  run_id: string;
  scenario_id: string;
  status: string;
};

export type CreateAgentBody = {
  name: string;
  description?: string;
  tags?: string[];
  slots: AgentSlot[];
  /// Optional strategy id this agent is scoped to. `undefined` (the
  /// default) creates a workspace-visible agent; setting it scopes
  /// the agent to a single strategy and hides it from the default
  /// workspace list. Phase 3 of `agent-firing-filter` (migration 036).
  scope_strategy_id?: string;
};

export type UpdateAgentBody = Partial<{
  name: string;
  description: string;
  tags: string[];
  slots: AgentSlot[];
  /// Three-valued patch for `Agent.scope_strategy_id`:
  /// - undefined → leave the column alone
  /// - { set: "<id>" } → scope the agent to that strategy
  /// - "clear" → promote a scoped agent back to the workspace
  scope_strategy_id: { set: string } | "clear";
}>;

export type ListAgentsQuery = {
  include_archived?: boolean;
  q?: string;
  limit?: number;
  /// Row offset for paged listings. Server treats `undefined` as 0.
  offset?: number;
  /// Scope filter. Default ("workspace") hides scoped agents.
  /// `"all"` returns every row. Any other value is interpreted as a
  /// strategy id and merges that strategy's scoped agents with the
  /// workspace set. Phase 3 of `agent-firing-filter` (migration 036).
  scope?: string;
};

/// Paged response envelope returned by `listAgentsPaged`.
export type AgentsPage = {
  items: Agent[];
  total: number;
};

function buildListUrl(q?: ListAgentsQuery): string {
  if (!q) return "/api/agents";
  const params = new URLSearchParams();
  if (q.include_archived) params.set("include_archived", "true");
  if (q.q) params.set("q", q.q);
  if (q.limit !== undefined) params.set("limit", String(q.limit));
  if (q.offset !== undefined) params.set("offset", String(q.offset));
  if (q.scope !== undefined && q.scope !== "") params.set("scope", q.scope);
  const qs = params.toString();
  return qs ? `/api/agents?${qs}` : "/api/agents";
}

export async function listAgents(q?: ListAgentsQuery): Promise<Agent[]> {
  const res = await apiFetch<{ items: Agent[]; total: number }>(buildListUrl(q));
  return res.items;
}

/// Paged variant — returns the `total` field so the dashboard's
/// `ListPagination` primitive can render "page X of N".
export async function listAgentsPaged(q?: ListAgentsQuery): Promise<AgentsPage> {
  const res = await apiFetch<{ items: Agent[]; total: number }>(buildListUrl(q));
  return { items: res.items, total: res.total };
}

export async function getAgent(agentId: string): Promise<Agent> {
  return apiFetch<Agent>(`/api/agents/${encodeURIComponent(agentId)}`);
}

export async function createAgent(body: CreateAgentBody): Promise<Agent> {
  return apiFetch<Agent>("/api/agents", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function updateAgent(
  agentId: string,
  body: UpdateAgentBody,
): Promise<Agent> {
  return apiFetch<Agent>(`/api/agents/${encodeURIComponent(agentId)}`, {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function archiveAgent(agentId: string): Promise<void> {
  await apiFetch<{ archived: boolean }>(
    `/api/agents/${encodeURIComponent(agentId)}`,
    { method: "DELETE" },
  );
}

export async function validateAgent(
  agentId: string,
): Promise<ValidationDiagnostic[]> {
  const res = await apiFetch<{ diagnostics: ValidationDiagnostic[] }>(
    `/api/agents/${encodeURIComponent(agentId)}/validate`,
    { method: "POST" },
  );
  return res.diagnostics;
}

export async function deployedInStrategies(
  agentId: string,
): Promise<StrategyRef[]> {
  const res = await apiFetch<{ items: StrategyRef[] }>(
    `/api/agents/${encodeURIComponent(agentId)}/strategies`,
  );
  return res.items;
}

export async function recentRuns(
  agentId: string,
  limit?: number,
): Promise<RunRef[]> {
  const path = limit
    ? `/api/agents/${encodeURIComponent(agentId)}/runs?limit=${limit}`
    : `/api/agents/${encodeURIComponent(agentId)}/runs`;
  const res = await apiFetch<{ items: RunRef[] }>(path);
  return res.items;
}

export type AgentTemplate = {
  id: string;
  name: string;
  description: string;
  slots: AgentSlot[];
};

export async function listAgentTemplates(): Promise<AgentTemplate[]> {
  const res = await apiFetch<{ items: AgentTemplate[] }>(
    "/api/agents/templates",
  );
  return res.items;
}

// Computed status — Draft / Validated / In use / Archived.
// v1 simplified vocabulary per the plan; "In use" is computed from
// `deployed_in`, which is always empty in v1, so this resolves to
// Draft/Validated/Archived in practice.
export type AgentStatus = "Draft" | "Validated" | "In use" | "Archived";

export const agentKeys = {
  all: ["agents"] as const,
  /// Cache key includes the full query (including `limit`/`offset`)
  /// so page changes refetch instead of slicing the same response.
  list: (q?: ListAgentsQuery) =>
    [...agentKeys.all, "list", q ?? {}] as const,
  detail: (id: string) => [...agentKeys.all, "detail", id] as const,
  validate: (id: string) => [...agentKeys.all, "validate", id] as const,
  deployedIn: (id: string) =>
    [...agentKeys.all, "deployed-in", id] as const,
  recentRuns: (id: string) =>
    [...agentKeys.all, "recent-runs", id] as const,
  templates: () => [...agentKeys.all, "templates"] as const,
};
