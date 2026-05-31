// Autoresearch API — wrappers around the dashboard's `/api/autoresearch/*`
// surface added in the AR-3 backend PR.
//
// Routes:
//   GET  /api/autoresearch/lineage           → LineageNode[]
//   GET  /api/autoresearch/lineage/:hash     → LineageNode
//   GET  /api/autoresearch/seals             → CycleSeal[]
//   GET  /api/autoresearch/seals/:cycle_id   → CycleSeal
//   GET  /api/autoresearch/ladder            → MutatorScore[]
//   GET  /api/autoresearch/diversity?...     → DiversityEntry[]
//   GET  /api/autoresearch/events            → SSE stream of CycleProgressEvent
//
// Operator-facing names (per terminology lock):
//   LineageNode    → "Experiment" / genealogy node
//   Mutator        → "Experiment writer"
//   CycleSeal      → "Evening summary"
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
  diff_hash?: string | null;
  gate_verdict?: string | null;
  status: LineageStatus;
  cycle_id?: string | null;
  created_at: string;
  diversity_score?: number | null;
};

/** Evening summary — sealed record of a completed cycle. */
export type CycleSeal = {
  seal_id: string;
  cycle_id: string;
  merkle_root: string;
  operator_signature: string;
  sealed_at: string;
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

/** SSE event from the /api/autoresearch/events stream. */
export type CycleProgressEvent = {
  event_type: string;
  cycle_id?: string | null;
  bundle_hash?: string | null;
  display_label?: string | null;
  ts: string;
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
  return qs ? `/api/autoresearch/diversity?${qs}` : "/api/autoresearch/diversity";
}

// ─── Fetch functions ──────────────────────────────────────────────────────────

export async function listLineageNodes(): Promise<LineageNode[]> {
  return apiFetch<LineageNode[]>("/api/autoresearch/lineage");
}

export async function getLineageNode(hash: string): Promise<LineageNode> {
  return apiFetch<LineageNode>(`/api/autoresearch/lineage/${encodeURIComponent(hash)}`);
}

export async function listSeals(): Promise<CycleSeal[]> {
  return apiFetch<CycleSeal[]>("/api/autoresearch/seals");
}

export async function getSeal(cycleId: string): Promise<CycleSeal> {
  return apiFetch<CycleSeal>(`/api/autoresearch/seals/${encodeURIComponent(cycleId)}`);
}

export async function getLadder(): Promise<MutatorScore[]> {
  return apiFetch<MutatorScore[]>("/api/autoresearch/ladder");
}

export async function getDiversity(q?: DiversityQuery): Promise<DiversityEntry[]> {
  return apiFetch<DiversityEntry[]>(buildDiversityUrl(q));
}

// ─── Query keys ───────────────────────────────────────────────────────────────

export const autoresearchKeys = {
  all: ["autoresearch"] as const,
  lineage: () => [...autoresearchKeys.all, "lineage"] as const,
  lineageNode: (hash: string) => [...autoresearchKeys.all, "lineage", hash] as const,
  seals: () => [...autoresearchKeys.all, "seals"] as const,
  seal: (cycleId: string) => [...autoresearchKeys.all, "seals", cycleId] as const,
  ladder: () => [...autoresearchKeys.all, "ladder"] as const,
  diversity: (q?: DiversityQuery) =>
    [...autoresearchKeys.all, "diversity", q ?? {}] as const,
};

// ─── TanStack Query hooks ─────────────────────────────────────────────────────

export function useLineageNodes() {
  return useQuery({
    queryKey: autoresearchKeys.lineage(),
    queryFn: listLineageNodes,
    staleTime: 30_000,
  });
}

export function useLineageNode(hash: string) {
  return useQuery({
    queryKey: autoresearchKeys.lineageNode(hash),
    queryFn: () => getLineageNode(hash),
    enabled: !!hash,
    staleTime: 60_000,
  });
}

export function useSeals() {
  return useQuery({
    queryKey: autoresearchKeys.seals(),
    queryFn: listSeals,
    staleTime: 60_000,
  });
}

export function useLadder() {
  return useQuery({
    queryKey: autoresearchKeys.ladder(),
    queryFn: getLadder,
    staleTime: 30_000,
  });
}

export function useDiversity(q?: DiversityQuery) {
  return useQuery({
    queryKey: autoresearchKeys.diversity(q),
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
  switch (event.event_type) {
    case "cycle_started":
      return "Cycle started";
    case "cycle_finished":
    case "cycle_sealed":
      return "Evening summary written";
    case "mutation_proposed":
      return "Experiment proposed";
    case "mutation_accepted":
      return "Experiment accepted";
    case "mutation_rejected":
      return "Experiment rejected";
    case "gate_evaluated":
      return "Gate evaluated";
    case "diversity_scored":
      return "Diversity scored";
    default:
      return event.event_type.replace(/_/g, " ");
  }
}
