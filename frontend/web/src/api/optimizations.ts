// Optimizations API — wraps the dashboard's `/api/optimizations` surface
// (Phase 3.7). See:
//   crates/xvision-dashboard/src/routes/optimizations.rs
//
// dspy-free: the dashboard exposes the optimizer's *results* (candidate
// instructions + an opaque snapshot blob) read back from the engine
// OptimizationStore. The frontend never touches dspy types either; it renders
// the plain candidate instruction strings + scalar metrics.

import { apiFetch } from "./client";
import type { Agent } from "./agents";

/// One persisted optimization run header (the reproduction recipe). Mirrors
/// `xvision_engine::optimization::OptimizationRun`.
export type OptimizationRun = {
  id: string;
  agent_id: string;
  slot_name: string;
  capability: string;
  /// `mipro` | `gepa` | `copro` — surfaced only in the advanced detail.
  optimizer: string;
  metric: string;
  corpus_query: string;
  rng_seed: number;
  model_provider: string | null;
  model_name: string | null;
  signature_hash: string | null;
  optimizer_version: string | null;
  /// `pending` | `running` | `completed` | `failed`.
  status: string;
  created_at: string;
};

/// One per-candidate search result. `instruction` is the candidate prompt the
/// "accept" action would adopt as the optimized slot's system prompt.
export type OptimizationCandidate = {
  id: string;
  run_id: string;
  candidate_index: number;
  instruction: string;
  metric_value: number | null;
  /// `train` | `holdout` — the holdout split column the detail view shows.
  split: string;
  demo_set: string | null;
  selected: boolean;
};

/// A persisted snapshot row. `snapshot_json` is an opaque optimizer blob the
/// UI does not parse; it carries the accept flag the detail surface toggles.
export type OptimizationSnapshot = {
  id: string;
  run_id: string;
  snapshot_json: string;
  signature_hash: string;
  demo_set: string | null;
  accepted: boolean;
  created_at: string;
};

/// A lineage edge: a child agent minted from an accepted run.
export type LineageEdge = {
  child_agent_id: string;
  parent_agent_id: string;
  optimization_run_id: string;
  created_at: string;
};

export type RunDetail = {
  run: OptimizationRun;
  candidates: OptimizationCandidate[];
  snapshots: OptimizationSnapshot[];
  lineage: LineageEdge[];
};

export type AcceptResponse = {
  child_agent: Agent;
  lineage: LineageEdge;
  snapshot_id: string;
  accepted: boolean;
};

export type RevertResponse = {
  snapshot_id: string;
  child_agent_id: string;
  accepted: boolean;
};

/// List optimization runs for an agent, optionally scoped to one slot.
export async function listOptimizations(
  agentId: string,
  slot?: string,
): Promise<OptimizationRun[]> {
  const params = new URLSearchParams({ agent: agentId });
  if (slot) params.set("slot", slot);
  const res = await apiFetch<{ runs: OptimizationRun[] }>(
    `/api/optimizations?${params.toString()}`,
  );
  return res.runs;
}

/// Fetch a run's full detail (header + candidate table + snapshots + lineage).
/// A failed run still returns its partial candidate set, so callers should
/// render whatever evidence comes back rather than treating failure as empty.
export async function getOptimization(runId: string): Promise<RunDetail> {
  return apiFetch<RunDetail>(
    `/api/optimizations/${encodeURIComponent(runId)}`,
  );
}

/// Accept the run's selected candidate as a new child agent. The parent agent
/// is left unchanged; a lineage edge (child → parent) is recorded.
export async function acceptOptimization(
  runId: string,
  snapshotId: string,
  childName?: string,
): Promise<AcceptResponse> {
  return apiFetch<AcceptResponse>(
    `/api/optimizations/${encodeURIComponent(runId)}/accept`,
    {
      method: "POST",
      body: JSON.stringify({
        snapshot_id: snapshotId,
        ...(childName ? { child_name: childName } : {}),
      }),
    },
  );
}

/// Revert a previously-accepted optimization: clears the snapshot accept flag
/// and drops the lineage edge for the child agent. The child agent row itself
/// is left in place (archive it from the agents surface if desired).
export async function revertOptimization(
  runId: string,
  snapshotId: string,
  childAgentId: string,
): Promise<RevertResponse> {
  return apiFetch<RevertResponse>(
    `/api/optimizations/${encodeURIComponent(runId)}/revert`,
    {
      method: "POST",
      body: JSON.stringify({
        snapshot_id: snapshotId,
        child_agent_id: childAgentId,
      }),
    },
  );
}

export const optimizationKeys = {
  all: ["optimizations"] as const,
  list: (agentId: string, slot?: string) =>
    [...optimizationKeys.all, "list", agentId, slot ?? null] as const,
  detail: (runId: string) =>
    [...optimizationKeys.all, "detail", runId] as const,
};
