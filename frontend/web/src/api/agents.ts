// Agents API — wraps `engine::api::agents::*` via the dashboard's
// `/api/agents` surface. See:
//   crates/xvision-dashboard/src/routes/agents.rs
//   docs/superpowers/plans/2026-05-11-agents-page-v1.md

import { apiFetch } from "./client";

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
};

export type UpdateAgentBody = Partial<{
  name: string;
  description: string;
  tags: string[];
  slots: AgentSlot[];
}>;

export type ListAgentsQuery = {
  include_archived?: boolean;
  q?: string;
  limit?: number;
  /// Row offset for paged listings. Server treats `undefined` as 0.
  offset?: number;
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
