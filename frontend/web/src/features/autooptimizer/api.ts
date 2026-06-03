// AutoOptimizer API — wrappers around the dashboard's `/api/autooptimizer/*`
// surface added in the AR-3 backend PR.
//
// Routes:
//   GET  /api/autooptimizer/lineage           → LineageNode[]
//   GET  /api/autooptimizer/lineage/:hash     → LineageNode
//   GET  /api/autooptimizer/ladder            → MutatorScore[]
//   GET  /api/autooptimizer/diversity?...     → DiversityEntry[]
//   GET  /api/autooptimizer/events            → SSE stream of CycleProgressEvent
//
// Operator-facing names (per terminology lock):
//   LineageNode    → "Experiment" / genealogy node
//   Mutator        → "Experiment writer"
//   gate_verdict   displayed as "Accepted" / "Rejected" / "Suspect"

import { useQuery } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";

// ─── Wire shapes ──────────────────────────────────────────────────────────────

/** Status of a lineage node. Operator label: "Active" | "Rejected" | "Suspect" */
export type LineageStatus = "active" | "rejected" | "quarantined";

/** A single experiment in the genealogy tree. */
export type LineageNode = {
  bundle_hash: string;
  parent_hash?: string | null;
  gate_verdict?: string | null;
  status: LineageStatus;
  cycle_id?: string | null;
  created_at: string;
  diversity_score?: number | null;
};

/** Experiment-writer performance record. */
export type MutatorScore = {
  provider: string;
  model: string;
  prompt_version: string;
  proposals: number;
  accepted: number;
  rejected_overfit: number;
  avg_delta_sharpe: number;
};

/** Entry from the diversity endpoint. */
export type DiversityEntry = {
  bundle_hash: string;
  diversity_score: number;
  cycle_id: string;
  created_at: string;
};

/** SSE event from the /api/autooptimizer/events stream. */
export type CycleProgressEvent = {
  event_type?: string;
  type?: string;
  kind?: string;
  cycle_id?: string | null;
  bundle_hash?: string | null;
  parent_hash?: string | null;
  child_hash?: string | null;
  display_label?: string | null;
  ts?: string;
  payload?: Record<string, unknown> | null;
};

// ─── Query params ─────────────────────────────────────────────────────────────

export type DiversityQuery = {
  cycle_id?: string;
  limit?: number;
};

// ─── URL builders ─────────────────────────────────────────────────────────────

function buildDiversityUrl(q?: DiversityQuery): string {
  const params = new URLSearchParams();
  if (q?.cycle_id) params.set("cycle_id", q.cycle_id);
  if (q?.limit != null) params.set("limit", String(q.limit));
  const qs = params.toString();
  return qs ? `/api/autooptimizer/diversity?${qs}` : "/api/autooptimizer/diversity";
}

// ─── Fetch functions ──────────────────────────────────────────────────────────

export async function listLineageNodes(): Promise<LineageNode[]> {
  return apiFetch<LineageNode[]>("/api/autooptimizer/lineage");
}

export async function getLineageNode(hash: string): Promise<LineageNode> {
  return apiFetch<LineageNode>(`/api/autooptimizer/lineage/${encodeURIComponent(hash)}`);
}

export async function getLadder(): Promise<MutatorScore[]> {
  return apiFetch<MutatorScore[]>("/api/autooptimizer/ladder");
}

export async function getDiversity(q?: DiversityQuery): Promise<DiversityEntry[]> {
  return apiFetch<DiversityEntry[]>(buildDiversityUrl(q));
}

// ─── Query keys ───────────────────────────────────────────────────────────────

export const autooptimizerKeys = {
  all: ["autooptimizer"] as const,
  lineage: () => [...autooptimizerKeys.all, "lineage"] as const,
  lineageNode: (hash: string) => [...autooptimizerKeys.all, "lineage", hash] as const,
  ladder: () => [...autooptimizerKeys.all, "ladder"] as const,
  diversity: (q?: DiversityQuery) =>
    [...autooptimizerKeys.all, "diversity", q ?? {}] as const,
};

// ─── TanStack Query hooks ─────────────────────────────────────────────────────

export function useLineageNodes() {
  return useQuery({
    queryKey: autooptimizerKeys.lineage(),
    queryFn: listLineageNodes,
    staleTime: 30_000,
  });
}

export function useLineageNode(hash: string) {
  return useQuery({
    queryKey: autooptimizerKeys.lineageNode(hash),
    queryFn: () => getLineageNode(hash),
    enabled: !!hash,
    staleTime: 60_000,
  });
}

export function useLadder() {
  return useQuery({
    queryKey: autooptimizerKeys.ladder(),
    queryFn: getLadder,
    staleTime: 30_000,
  });
}

export function useDiversity(q?: DiversityQuery) {
  return useQuery({
    queryKey: autooptimizerKeys.diversity(q),
    queryFn: () => getDiversity(q),
    staleTime: 30_000,
  });
}

// ─── Operator label helpers ───────────────────────────────────────────────────

/** Map developer status string to operator-facing label (terminology lock). */
export function formatLineageStatus(status: LineageStatus): string {
  switch (status) {
    case "active":
      return "Active";
    case "rejected":
      return "Rejected";
    case "quarantined":
      return "Suspect";
    default:
      return status;
  }
}

/** Map gate_verdict wire value to operator-facing label. */
export function formatGateVerdict(verdict?: string | null): string {
  if (!verdict) return "Pending";
  switch (verdict.toLowerCase()) {
    case "accepted":
    case "pass":
      return "Accepted";
    case "rejected":
    case "fail":
    case "ghost":
      return "Rejected";
    case "quarantined":
      return "Suspect";
    default:
      return verdict;
  }
}

/** Map CycleProgressEvent.event_type to a plain-language operator label. */
export function formatEventLabel(event: CycleProgressEvent): string {
  if (event.display_label) return event.display_label;
  const eventType = event.event_type ?? event.type ?? event.kind ?? "";
  switch (eventType) {
    case "cycle_started":
      return "Cycle started";
    case "parent_selected":
      return "Parent selected";
    case "cycle_finished":
      return "Optimizer run finished";
    case "mutation_proposed":
      return "Experiment proposed";
    case "mutation_accepted":
      return "Experiment accepted";
    case "mutation_rejected":
      return "Experiment rejected";
    case "gate_evaluated":
    case "mutation_gated":
    case "mutation_gated_passed":
    case "mutation_gated_dropped":
      return "Gate evaluated";
    case "honesty_check_run":
      return "Honesty check result";
    case "judge_finding":
      return "Reviewer finished notes";
    case "diversity_scored":
      return "Diversity scored";
    case "job_started":
      return "Optimizer job started";
    case "job_finished":
      return "Optimizer job finished";
    default:
      return eventType ? eventType.replace(/_/g, " ") : "Optimizer event";
  }
}
