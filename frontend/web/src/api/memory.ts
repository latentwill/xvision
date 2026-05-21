// Memory API — wraps `engine::api::memory::*` via the dashboard's
// `/api/memory` surface (V2D v1.1 follow-up).
//
// Routes mirrored here:
//   GET    /api/memory                  → listMemory
//   GET    /api/memory/:id              → getMemoryItem
//   POST   /api/memory/patterns         → createPattern
//   DELETE /api/memory/:id              → deleteMemoryItem
//   DELETE /api/memory?namespace=|agent=→ forgetMemory
//
// ts-rs generated types are not yet emitted for the MemoryItem family
// (requires the engine's `ts-export` feature to be built and the
// resulting *.ts files committed). Hand-written types here match the
// engine's `MemoryItemDto` / `MemoryListResponse` /
// `PatternCreateRequest` / `ForgetResponse` shapes verbatim — keep in
// sync by hand until ts-rs codegen is wired into CI.

import { apiFetch } from "./client";

/// Tier discriminator on the wire. The engine sends lower-case strings.
export type Tier = "observation" | "pattern";

/// Memory item as returned by the engine. Mirrors `MemoryItemDto`.
export type MemoryItem = {
  id: string;
  namespace: string;
  tier: Tier;
  text: string;
  /// RFC3339 timestamp.
  created_at: string;
  run_id: string | null;
  scenario_id: string | null;
  cycle_idx: number | null;
  /// RFC3339 date; null on Observations and on operator-attested
  /// Patterns where the operator wants global applicability.
  training_window_end: string | null;
};

export type MemoryListResponse = {
  items: MemoryItem[];
  total: number;
};

export type ListMemoryQuery = {
  tier?: Tier;
  namespace?: string;
  agent?: string;
  scenario_id?: string;
  run_id?: string;
  limit?: number;
  offset?: number;
};

export type PatternCreateBody = {
  text: string;
  namespace: string;
  /// Optional RFC3339 date. If set, the Pattern is only recalled in
  /// scenarios that start AFTER this timestamp (V2D leakage filter).
  training_window_end?: string | null;
};

export type ForgetResponse = {
  deleted: number;
};

function buildListUrl(q?: ListMemoryQuery): string {
  if (!q) return "/api/memory";
  const params = new URLSearchParams();
  if (q.tier) params.set("tier", q.tier);
  if (q.namespace) params.set("namespace", q.namespace);
  if (q.agent) params.set("agent", q.agent);
  if (q.scenario_id) params.set("scenario_id", q.scenario_id);
  if (q.run_id) params.set("run_id", q.run_id);
  if (q.limit !== undefined) params.set("limit", String(q.limit));
  if (q.offset !== undefined) params.set("offset", String(q.offset));
  const qs = params.toString();
  return qs ? `/api/memory?${qs}` : "/api/memory";
}

export async function listMemory(
  q?: ListMemoryQuery,
): Promise<MemoryListResponse> {
  return apiFetch<MemoryListResponse>(buildListUrl(q));
}

export async function getMemoryItem(id: string): Promise<MemoryItem> {
  return apiFetch<MemoryItem>(`/api/memory/${encodeURIComponent(id)}`);
}

export async function createPattern(
  body: PatternCreateBody,
): Promise<MemoryItem> {
  return apiFetch<MemoryItem>("/api/memory/patterns", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function deleteMemoryItem(id: string): Promise<void> {
  await apiFetch<void>(`/api/memory/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

/// Bulk forget. Pass either `namespace` (exact match, e.g. `"global"`
/// or `"agent:<id>"`) or `agent` (convenience for `agent:<id>`). The
/// engine rejects callers that set both or neither.
export async function forgetMemory(opts: {
  namespace?: string;
  agent?: string;
}): Promise<ForgetResponse> {
  const params = new URLSearchParams();
  if (opts.namespace) params.set("namespace", opts.namespace);
  if (opts.agent) params.set("agent", opts.agent);
  return apiFetch<ForgetResponse>(`/api/memory?${params.toString()}`, {
    method: "DELETE",
  });
}

/// Build the canonical `agent:<id>` namespace string used by V2D's
/// agent-scoped memory. Centralising it here keeps the UI and CLI
/// pairs from drifting against the engine's `memory::agent_namespace`.
export function agentNamespace(agentId: string): string {
  return `agent:${agentId}`;
}

export const memoryKeys = {
  all: ["memory"] as const,
  list: (q?: ListMemoryQuery) => [...memoryKeys.all, "list", q ?? {}] as const,
  detail: (id: string) => [...memoryKeys.all, "detail", id] as const,
};
