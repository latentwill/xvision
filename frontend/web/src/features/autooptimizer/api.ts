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

import { useMutation, useQuery } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";

// ─── Wire shapes ──────────────────────────────────────────────────────────────

/** Status of a lineage node. Operator label: "Active" | "Rejected" | "Suspect" */
export type LineageStatus = "active" | "rejected" | "quarantined";

/** A single experiment in the genealogy tree. */
/** Wire shape of a Rust `GateVerdict`: `Pass` → `"Pass"`, `Fail { reason }` →
 *  `{ Fail: { reason } }`. The DB form (`"passed"` / `"rejected:<reason>"`) also
 *  flows through here. Always render via {@link formatGateVerdict}. */
export type GateVerdictWire = string | { Pass?: null; Fail?: { reason: string } };

export type LineageNode = {
  bundle_hash: string;
  parent_hash?: string | null;
  gate_verdict?: GateVerdictWire | null;
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
  data?: Record<string, unknown> | null;
};

export type StartRunCycleRequest = {
  strategy_id: string;
  mutator_provider?: string | null;
  mutator_model?: string | null;
  judge_provider?: string | null;
  judge_model?: string | null;
  // F28: token budget ceiling (USD) + per-run evaluation window overrides
  // (YYYY-MM-DD). Omit for no cap / the config default window.
  budget_usd?: number | null;
  day_start?: string | null;
  day_end?: string | null;
  baseline_start?: string | null;
  baseline_end?: string | null;
};

export type StartRunCycleResponse = {
  started: boolean;
  message: string;
};

export type OptimizerRunDefaults = {
  mutator_provider: string;
  mutator_model: string;
  judge_provider: string;
  judge_model: string;
  config_path: string;
  config_exists: boolean;
};

export type StrategyBlob = Record<string, unknown>;

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

export type LineageQuery = { cycleId?: string; status?: LineageStatus; limit?: number };

export async function listLineageNodes(q?: LineageQuery): Promise<LineageNode[]> {
  const params = new URLSearchParams();
  if (q?.cycleId) params.set("cycle_id", q.cycleId);
  if (q?.status) params.set("status", q.status);
  if (q?.limit != null) params.set("limit", String(q.limit));
  const qs = params.toString();
  return apiFetch<LineageNode[]>(
    qs ? `/api/autooptimizer/lineage?${qs}` : "/api/autooptimizer/lineage",
  );
}

/** Per-regime performance metrics (mirrors MetricsSummary serialized fields). */
export type RegimeMetrics = {
  total_return_pct: number;
  sharpe: number;
  max_drawdown_pct: number;
  win_rate: number;
  n_trades: number;
};

/** One per-regime evaluation result for a lineage node (from autooptimizer_regime_results). */
export type RegimeResult = {
  regime_label: string;
  /** Serialized as snake_case: "bull" | "bear_or_shock" | "chop" */
  side: "bull" | "bear_or_shock" | "chop";
  delta_sharpe: number;
  verdict: string;
  metrics_day: RegimeMetrics;
  metrics_untouched: RegimeMetrics;
};

/** Lineage node enriched with per-regime results (Phase 2 regime matrix). */
export type CycleNodeDetail = LineageNode & {
  metrics_day?: RegimeMetrics | null;
  metrics_untouched?: RegimeMetrics | null;
  /** Per-regime evaluation results; empty for single-window cycles or pre-Phase-2 nodes. */
  regime_results: RegimeResult[];
};

/** One historic optimizer cycle (grouped from lineage) with F23 tokens + cost. */
export type CycleRunSummary = {
  cycle_id: string;
  node_count: number;
  active_count: number;
  /** Quarantined (Suspect) nodes — partial-pass across regimes. */
  suspect_count?: number;
  rejected_count: number;
  first_created_at: string;
  last_created_at: string;
  cost_usd?: number | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  unpriced_calls?: number | null;
};

/** Full detail for one cycle: summary fields + its lineage nodes + honesty check. */
export type CycleRunDetail = CycleRunSummary & {
  nodes: CycleNodeDetail[];
  honesty_check?: {
    passed: boolean;
    sabotage_variant: string;
    message: string;
  } | null;
};

export async function listCycleRuns(): Promise<CycleRunSummary[]> {
  return apiFetch<CycleRunSummary[]>("/api/autooptimizer/cycles");
}

export async function getCycleRun(cycleId: string): Promise<CycleRunDetail> {
  return apiFetch<CycleRunDetail>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}`,
  );
}

/** F35.3: live per-cycle cost/tokens, read straight from `cycle_cost`. Unlike a
 *  cycle's detail this is populated by the background ticker every ~10s while the
 *  cycle runs — and before the first candidate commits — so the Live tab can show
 *  climbing spend. `recorded` is false until the first persist / for unknown ids. */
export type CycleCost = {
  cycle_id: string;
  cost_usd?: number | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  unpriced_calls?: number | null;
  recorded: boolean;
};

export async function getCycleCost(cycleId: string): Promise<CycleCost> {
  return apiFetch<CycleCost>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}/cost`,
  );
}

/** F29: retire a cycle-produced candidate (move its lineage node to Rejected) —
 *  dashboard parity for `xvn optimizer retire`. */
export type RetireResponse = {
  bundle_hash: string;
  status: string;
  message: string;
};

export async function retireLineageNode(hash: string): Promise<RetireResponse> {
  return apiFetch<RetireResponse>(
    `/api/autooptimizer/lineage/${encodeURIComponent(hash)}/retire`,
    { method: "POST" },
  );
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

export async function startRunCycle(
  body: StartRunCycleRequest,
): Promise<StartRunCycleResponse> {
  return apiFetch<StartRunCycleResponse>("/api/autooptimizer/run-cycle", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function getRunDefaults(): Promise<OptimizerRunDefaults> {
  return apiFetch<OptimizerRunDefaults>("/api/autooptimizer/run-defaults");
}

export async function getBlob<T = StrategyBlob>(hash: string): Promise<T> {
  return apiFetch<T>(`/api/autooptimizer/blob/${encodeURIComponent(hash)}`);
}

/** F28: request cancellation of an in-flight optimizer cycle. */
export async function cancelRunCycle(cycleId: string): Promise<StartRunCycleResponse> {
  return apiFetch<StartRunCycleResponse>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}/cancel`,
    { method: "POST" },
  );
}

// ─── Query keys ───────────────────────────────────────────────────────────────

export const autooptimizerKeys = {
  all: ["autooptimizer"] as const,
  lineage: (q?: LineageQuery) =>
    [...autooptimizerKeys.all, "lineage", q ?? {}] as const,
  lineageNode: (hash: string) => [...autooptimizerKeys.all, "lineage", hash] as const,
  ladder: () => [...autooptimizerKeys.all, "ladder"] as const,
  cycles: () => [...autooptimizerKeys.all, "cycles"] as const,
  cycle: (id: string) => [...autooptimizerKeys.cycles(), id] as const,
  runDefaults: () => [...autooptimizerKeys.all, "run-defaults"] as const,
  cycleCost: (cycleId: string | null | undefined) =>
    [...autooptimizerKeys.all, "cycle-cost", cycleId ?? ""] as const,
  diversity: (q?: DiversityQuery) =>
    [...autooptimizerKeys.all, "diversity", q ?? {}] as const,
  blob: (hash: string | null | undefined) =>
    [...autooptimizerKeys.all, "blob", hash ?? ""] as const,
};

// ─── TanStack Query hooks ─────────────────────────────────────────────────────

export function useLineageNodes(q?: LineageQuery) {
  return useQuery({
    queryKey: autooptimizerKeys.lineage(q),
    queryFn: () => listLineageNodes(q),
    staleTime: 30_000,
  });
}

/** F23: historic cycles with per-cycle tokens + realized cost. */
export function useCycleRuns() {
  return useQuery({
    queryKey: autooptimizerKeys.cycles(),
    queryFn: listCycleRuns,
    staleTime: 30_000,
  });
}

export function useCycleRun(cycleId: string | undefined) {
  return useQuery({
    queryKey: autooptimizerKeys.cycle(cycleId ?? ""),
    queryFn: () => getCycleRun(cycleId!),
    enabled: !!cycleId,
    staleTime: 30_000,
  });
}

/** F35.3: poll the running cycle's live cost/tokens. Pass the active cycle id
 *  (derived from the SSE `cycle_started` event); polling stops once `enabled` is
 *  false (cycle finished/cancelled). 5s cadence matches the backend's ~10s ticker
 *  closely enough for a live ticker without hammering the DB. */
export function useCycleCost(
  cycleId: string | null | undefined,
  enabled: boolean,
) {
  return useQuery({
    queryKey: autooptimizerKeys.cycleCost(cycleId),
    queryFn: () => getCycleCost(cycleId!),
    enabled: !!cycleId && enabled,
    refetchInterval: enabled ? 5_000 : false,
    staleTime: 0,
  });
}

/** F29: retire a lineage node (move it to Rejected). */
export function useRetireLineageNode() {
  return useMutation({
    mutationFn: (hash: string) => retireLineageNode(hash),
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

export function useBlob<T = StrategyBlob>(hash: string | null | undefined) {
  return useQuery({
    queryKey: autooptimizerKeys.blob(hash),
    queryFn: () => getBlob<T>(hash!),
    enabled: !!hash,
    staleTime: 60_000,
  });
}

/** Derive per-regime results for an experiment from the parent cycle's node list.
 *  Avoids a new backend endpoint by piggy-backing on the existing useCycleRun query.
 *  Returns an empty array when cycleId is absent or the matching node has no results. */
// Fix 7: return isLoading so callers can suppress the brief empty-state flash
// while the cycle query is still in-flight.
export function useExperimentRegimeResults(
  hash: string,
  cycleId: string | undefined,
): { results: RegimeResult[]; isLoading: boolean } {
  const { data: cycle, isLoading } = useCycleRun(cycleId);
  if (!cycle || !hash)
    return { results: [], isLoading: isLoading || !hash || !cycleId };
  const node = cycle.nodes?.find((n) => n.bundle_hash === hash);
  return { results: node?.regime_results ?? [], isLoading: false };
}

// ─── Session-level status types (P1 hero / status polling) ───────────────────

/** Summary of one optimizer session for the status hero and recent-runs list. */
export interface SessionSummary {
  session_id: string;
  strategy_id: string;
  /** State machine state: "running" | "paused" | "cancelling" | "finished" | "failed" | "idle" */
  state: string;
  /** Run mode label: "explore" | "exploit" | etc. */
  mode: string;
  cycles_completed: number;
  kept_count: number;
  suspect_count: number;
  dropped_count: number;
}

/** Response from GET /api/autooptimizer/status */
export interface StatusResponse {
  active_session: SessionSummary | null;
  last_event_seq: number;
}

/** Row in the recent-sessions list (GET /api/autooptimizer/sessions) */
export interface SessionListItem {
  session_id: string;
  strategy_id: string;
  state: string;
  mode: string;
  cycles_completed: number;
  kept_count: number;
  cost_usd?: number;
  finished_at?: string;
}

/** Poll the running-session status. Refetch every 5 s while active, 30 s when idle. */
export function useOptimizerStatus(): StatusResponse | undefined {
  const { data } = useQuery({
    queryKey: ["optimizer/status"],
    queryFn: () =>
      fetch("/api/autooptimizer/status").then((r) => r.json()) as Promise<StatusResponse>,
    refetchInterval: (query) => (query.state.data?.active_session ? 5_000 : 30_000),
    // Fall back gracefully when the backend endpoint doesn't exist yet
    retry: false,
  });
  return data;
}

/** Fetch the 10 most recent sessions for the recent-runs list. */
export function useSessionList() {
  return useQuery({
    queryKey: ["optimizer/sessions"],
    queryFn: () =>
      fetch("/api/autooptimizer/sessions?limit=10").then(
        (r) => r.json(),
      ) as Promise<SessionListItem[]>,
    staleTime: 30_000,
    retry: false,
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

/** Map gate_verdict wire value to operator-facing label.
 *
 * The Rust `GateVerdict` enum serializes `Pass` as the string `"Pass"` but
 * `Fail { reason }` as an OBJECT `{ "Fail": { "reason": "…" } }`. The DB form is
 * `"passed"` / `"rejected:<reason>"`. This formatter accepts any of those shapes
 * — and is defensive about unexpected ones — so a rejected lineage node can't
 * crash the Genealogy / Provenance tabs (calling `.toLowerCase()` on an object
 * threw, blanking the whole tab).
 */
export function formatGateVerdict(verdict?: unknown): string {
  if (verdict == null) return "Pending";

  // Object form (Rust externally-tagged enum): { Pass: … } | { Fail: { reason } }.
  if (typeof verdict === "object") {
    const o = verdict as Record<string, unknown>;
    if ("Fail" in o || "fail" in o) return "Rejected";
    if ("Pass" in o || "pass" in o) return "Accepted";
    return "Rejected";
  }

  const key = String(verdict).toLowerCase();
  if (key === "accepted" || key === "pass" || key === "passed") return "Accepted";
  if (key === "quarantined") return "Suspect";
  // `rejected:<reason>` / `fail:<reason>` carry a suffix — match the prefix.
  if (key.startsWith("rejected") || key.startsWith("fail") || key === "ghost") {
    return "Rejected";
  }
  return String(verdict);
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
    case "no_candidate":
      return "No experiment produced";
    case "mutation_accepted":
      return "Experiment accepted";
    case "mutation_rejected":
      return "Experiment rejected";
    case "gate_evaluated":
    case "mutation_gated":
    case "mutation_gated_passed":
    case "mutation_gated_dropped":
      return "Gate evaluated";
    case "honesty_check_run": {
      // F9: prefer the labeled human-readable outcome (e.g. "Honesty check
      // passed: sabotaged variant `kill-trades` …") so the operator sees the
      // result rather than inferring it from raw broker-rule warnings.
      const msg = (event as { message?: unknown }).message;
      return typeof msg === "string" && msg.trim().length > 0 ? msg : "Honesty check result";
    }
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
