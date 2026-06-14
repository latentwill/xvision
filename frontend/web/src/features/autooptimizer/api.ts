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

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
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

/** SSE event from the /api/autooptimizer/events stream.
 *
 * Wire shape (progress.rs) is serde-tagged with per-kind fields flattened at
 * the top level (`passed`, `outcome`, `delta_day`, `parent_count`, …), so the
 * type carries an open index signature beyond the shared base fields. */
export type CycleProgressEvent = {
  event_type?: string;
  type?: string;
  kind?: string;
  cycle_id?: string | null;
  bundle_hash?: string | null;
  parent_hash?: string | null;
  child_hash?: string | null;
  /** WS-11b: on `mutation_gated`, the candidate's persisted eval `Run.id`.
   *  The OPTI reducer nests a navigable `opti.eval-run` node carrying it under
   *  the experiment so an operator can drill cycle → experiment → eval-run
   *  trace. Absent for the regime/no-candidate paths and for runners that
   *  don't surface a run id. */
  eval_run_id?: string | null;
  display_label?: string | null;
  ts?: string;
  payload?: Record<string, unknown> | null;
  data?: Record<string, unknown> | null;
  [key: string]: unknown;
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
  /** Candidate experiments to generate per parent each cycle (1..=64).
   * Omit for the configured `experiments_per_cycle` (default 5). */
  experiments_per_cycle?: number | null;
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
  /** Day-window Sharpe delta from the gate verdict. Null for nodes without a gate result. */
  delta_day?: number | null;
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

/** Pagination options for the historic-cycles list. The backend caps each
 *  page server-side (default 50); the UI passes an explicit page size so the
 *  history list never renders unbounded rows (UI3).
 *  `session_id` filters cycles to a single optimizer session (bead .10). */
export type CycleRunsQuery = { limit?: number; offset?: number; session_id?: string };

export async function listCycleRuns(q?: CycleRunsQuery): Promise<CycleRunSummary[]> {
  const params = new URLSearchParams();
  if (q?.limit != null) params.set("limit", String(q.limit));
  if (q?.offset != null) params.set("offset", String(q.offset));
  if (q?.session_id) params.set("session_id", q.session_id);
  const qs = params.toString();
  return apiFetch<CycleRunSummary[]>(
    qs ? `/api/autooptimizer/cycles?${qs}` : "/api/autooptimizer/cycles",
  );
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

// ─── Cycle-level pause/resume (Control Tower S0 / O3) ─────────────────────────
// NOTE: the mounted pause/resume surface is cycle-level
// (`/api/autooptimizer/cycles/:cycle_id/{pause,resume}`). The older
// `pauseSession`/`resumeSession` helpers below target an unmounted
// `/sessions/:id/...` route and are dead — use these from the Active-tasks strip.

/** Pause the in-flight optimizer cycle (suspends before the next candidate). */
export async function pauseCycle(cycleId: string): Promise<StartRunCycleResponse> {
  return apiFetch<StartRunCycleResponse>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}/pause`,
    { method: "POST" },
  );
}

/** Resume a paused optimizer cycle. */
export async function resumeCycle(cycleId: string): Promise<StartRunCycleResponse> {
  return apiFetch<StartRunCycleResponse>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}/resume`,
    { method: "POST" },
  );
}

/** useMutation hook: pause the in-flight cycle, then refresh status. */
export function usePauseCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (cycleId: string) => pauseCycle(cycleId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["optimizer/status"] }),
  });
}

/** useMutation hook: resume the in-flight cycle, then refresh status. */
export function useResumeCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (cycleId: string) => resumeCycle(cycleId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["optimizer/status"] }),
  });
}

/** useMutation hook: cancel the in-flight cycle (mounted cycle-level route),
 *  then refresh status. Use this instead of `useCancelSession` — the
 *  session-level `/sessions/:id/cancel` route is not mounted. */
export function useCancelCycle() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (cycleId: string) => cancelRunCycle(cycleId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["optimizer/status"] }),
  });
}

// ─── Session-level control mutations (P4) ────────────────────────────────────

/** Pause a running optimizer session. */
export async function pauseSession(sessionId: string): Promise<void> {
  await apiFetch<unknown>(
    `/api/autooptimizer/sessions/${encodeURIComponent(sessionId)}/pause`,
    { method: "POST" },
  );
}

/** Resume a paused optimizer session. */
export async function resumeSession(sessionId: string): Promise<void> {
  await apiFetch<unknown>(
    `/api/autooptimizer/sessions/${encodeURIComponent(sessionId)}/resume`,
    { method: "POST" },
  );
}

/** Cancel a running or paused optimizer session. */
export async function cancelSession(sessionId: string): Promise<void> {
  await apiFetch<unknown>(
    `/api/autooptimizer/sessions/${encodeURIComponent(sessionId)}/cancel`,
    { method: "POST" },
  );
}

/** useMutation hook: pause session. */
export function usePauseSession() {
  return useMutation({
    mutationFn: (sessionId: string) => pauseSession(sessionId),
  });
}

/** useMutation hook: resume session. */
export function useResumeSession() {
  return useMutation({
    mutationFn: (sessionId: string) => resumeSession(sessionId),
  });
}

/** useMutation hook: cancel session. */
export function useCancelSession() {
  return useMutation({
    mutationFn: (sessionId: string) => cancelSession(sessionId),
  });
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
  cycleEvents: (cycleId: string | null | undefined) =>
    [...autooptimizerKeys.all, "cycle-events", cycleId ?? ""] as const,
  river: () => [...autooptimizerKeys.all, "river"] as const,
};

// ─── TanStack Query hooks ─────────────────────────────────────────────────────

export function useLineageNodes(q?: LineageQuery) {
  return useQuery({
    queryKey: autooptimizerKeys.lineage(q),
    queryFn: () => listLineageNodes(q),
    staleTime: 30_000,
  });
}

/** F23: historic cycles with per-cycle tokens + realized cost.
 *
 * UI3: pass `{ limit }` to cap the page. The query key folds the params in so
 * a different page size doesn't collide with the unscoped list cache. Calling
 * with no args keeps the original (unscoped) key, so existing consumers and
 * `invalidateQueries({ queryKey: autooptimizerKeys.cycles() })` still match. */
export function useCycleRuns(q?: CycleRunsQuery) {
  return useQuery({
    queryKey: q ? [...autooptimizerKeys.cycles(), q] : autooptimizerKeys.cycles(),
    queryFn: () => listCycleRuns(q),
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

// ─── Schedule types (P5-W3) ───────────────────────────────────────────────────

/** A scheduled optimizer run record from GET /api/autooptimizer/schedule. */
export interface Schedule {
  id: number;
  enabled: boolean;
  /** Local time in HH:MM format, e.g. "21:00" */
  time_local: string;
  strategy_id: string;
  last_run_at: string | null;
  next_run_at: string | null;
}

/** Request body for POST /api/autooptimizer/schedule (upsert). */
export type UpsertScheduleRequest = {
  enabled: boolean;
  time_local: string;
  strategy_id: string;
};

export async function getSchedule(): Promise<Schedule | null> {
  return apiFetch<Schedule | null>("/api/autooptimizer/schedule").catch(() => null);
}

export async function upsertSchedule(body: UpsertScheduleRequest): Promise<Schedule> {
  return apiFetch<Schedule>("/api/autooptimizer/schedule", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export function useSchedule() {
  return useQuery({
    queryKey: ["optimizer/schedule"],
    queryFn: getSchedule,
    staleTime: 30_000,
    retry: false,
  });
}

export function useUpsertSchedule() {
  return useMutation({
    mutationFn: (req: UpsertScheduleRequest) => upsertSchedule(req),
  });
}

export async function deleteSchedule(scheduleId: number): Promise<void> {
  await apiFetch<void>(
    `/api/autooptimizer/schedule/${encodeURIComponent(scheduleId)}`,
    { method: "DELETE" },
  );
}

export function useDeleteSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (scheduleId: number) => deleteSchedule(scheduleId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["optimizer/schedule"] }),
  });
}

// ─── Flywheel types (P3-W4) ───────────────────────────────────────────────────

/** Metrics from the most recent DSPy prompt-compile gate evaluation. */
export interface LastPromptCompile {
  dev_metric: string | null;
  parent_dev_score: number | null;
  child_dev_score: number | null;
  delta_dev: number | null;
  parent_holdout_score: number | null;
  child_holdout_score: number | null;
  delta_holdout: number | null;
  gate_verdict: string | null;
  gated_at: string | null;
}

/** Response from GET /api/autooptimizer/flywheel */
export interface FlywheelResponse {
  enabled: boolean;
  cohort_count?: number;
  threshold?: number;
  compiled_pattern_count?: number;
  latest_optimization_run_id?: string;
  last_prompt_compile?: LastPromptCompile | null;
}

export function useFlywheel() {
  return useQuery({
    queryKey: ["optimizer/flywheel"],
    queryFn: () =>
      fetch("/api/autooptimizer/flywheel").then((r) => r.json()) as Promise<FlywheelResponse>,
    staleTime: 30_000,
    retry: false,
  });
}

// ─── Stats rows (P3-W3 charts) ────────────────────────────────────────────────

/** One row from GET /api/autooptimizer/stats — per-cycle aggregates for charts. */
export interface StatsRow {
  cycle_id: string;
  session_id: string;
  ts: string;
  kept: number;
  suspect: number;
  dropped: number;
  best_delta_holdout: number | null;
  /** Best candidate edge over the random baseline this cycle (child − random). */
  best_edge_over_random?: number | null;
  /** Best parent edge over the random baseline this cycle (parent − random). */
  best_parent_edge?: number | null;
  cost_usd: number;
  cum_cost_usd: number;
}

export type StatsQuery = {
  strategy_id?: string;
  session_id?: string;
  since?: string;
};

export function useOptimizerStats(params?: StatsQuery) {
  return useQuery({
    queryKey: ["optimizer/stats", params ?? {}] as const,
    queryFn: () => {
      const q = new URLSearchParams(
        Object.fromEntries(
          Object.entries(params ?? {}).filter(([, v]) => v != null),
        ) as Record<string, string>,
      );
      const qs = q.toString();
      return fetch(`/api/autooptimizer/stats${qs ? `?${qs}` : ""}`).then(
        (r) => r.json(),
      ) as Promise<StatsRow[]>;
    },
    staleTime: 30_000,
    retry: false,
  });
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
  /** Newest in-flight cycle id for the active session — target for the
   *  Active-tasks pause/resume controls (S0 / O3). Absent when idle. */
  active_cycle_id?: string | null;
}

/** Row in the recent-sessions list (GET /api/autooptimizer/sessions) */
export interface SessionListItem {
  session_id: string;
  strategy_id: string;
  state: string;
  mode: string;
  cycles_completed: number;
  kept_count: number;
  /** Candidates demoted to "suspect" across the session (S0 / O1a). */
  suspect_count?: number;
  /** Σ realized cost across the session's cycles (S0 / O1c); undefined → "$?". */
  cost_usd?: number;
  /** Newest cycle's honesty-check outcome (S0 / O1b); undefined → "—". */
  honesty_passed?: boolean;
  /** Newest cycle's accepted-lineage edge over the random baseline
   * (parent − random); undefined → "—". > 0 = still beating random. */
  latest_parent_edge?: number | null;
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

/** Full session record from GET /api/autooptimizer/sessions/:id.
 *  Returns the raw OptimizerSession row (same shape as SessionSummary minus
 *  the `cost_usd` enrichment which is only on the list endpoint). */
export interface SessionDetail {
  session_id: string;
  strategy_id: string;
  state: string;
  mode: string;
  cycles_planned: number | null;
  cycles_completed: number;
  kept_count: number;
  suspect_count: number;
  dropped_count: number;
  errored_count: number;
  created_at: string;
  updated_at: string;
}

/** Fetch a single session by id (bead .10). */
export function useSessionDetail(sessionId: string | undefined) {
  return useQuery<SessionDetail>({
    queryKey: ["optimizer/sessions", sessionId ?? ""],
    queryFn: () =>
      apiFetch<SessionDetail>(
        `/api/autooptimizer/sessions/${encodeURIComponent(sessionId!)}`,
      ),
    enabled: !!sessionId,
    staleTime: 30_000,
    retry: false,
  });
}

// ─── Experiment detail types ──────────────────────────────────────────────────

/** Gate evaluation record for a single lineage node. */
export interface GateRecord {
  bundle_hash: string;
  parent_day_score: number | null;
  child_day_score: number | null;
  parent_holdout_score: number | null;
  child_holdout_score: number | null;
  gate_epsilon: number | null;
  delta_day: number | null;
  delta_holdout: number | null;
  drawdown_ratio: number | null;
  verdict: string;
  reason: string | null;
  /** Edge vs a fixed-seed random baseline (informational, never gating).
   * Optional: absent on gate records written before migration 061. */
  edge_over_random?: number | null;
  parent_edge?: number | null;
  edge_delta?: number | null;
}

/** A single finding emitted by the judge for an experiment. */
export interface ExperimentFinding {
  id: number;
  bundle_hash: string;
  severity: "info" | "warn" | "risk";
  code: string;
  summary: string;
  detail: string | null;
  model: string | null;
}

/** Full detail response for a single experiment. */
export interface ExperimentDetailResponse {
  lineage_node: LineageNode;
  rationale: string | null;
  gate_record: GateRecord | null;
  findings: ExperimentFinding[];
  regime_results: RegimeResult[];
}

export async function getExperimentDetail(hash: string): Promise<ExperimentDetailResponse> {
  return apiFetch<ExperimentDetailResponse>(
    `/api/autooptimizer/experiments/${encodeURIComponent(hash)}/detail`,
  );
}

export function useExperimentDetail(hash: string) {
  return useQuery({
    queryKey: ["experiments", hash, "detail"],
    queryFn: () => getExperimentDetail(hash),
    enabled: !!hash,
    staleTime: 60_000,
    // Fail gracefully — the endpoint may not exist yet in older backend versions.
    retry: false,
  });
}

// ─── Strategy Inspector types (unified optimizer plan) ───────────────────────

export interface StrategyDiff {
  prose: Array<{ agent_role: string; before: string; after: string }>;
  params: Array<{ key: string; before: unknown; after: unknown }>;
  tools: { added: string[]; removed: string[] };
  filter: Array<{ path: string; before: unknown; after: unknown }>;
}

export interface OriginDiffResponse {
  origin_hash: string;
  diff: StrategyDiff;
}

async function getOriginDiff(hash: string): Promise<OriginDiffResponse> {
  return apiFetch<OriginDiffResponse>(
    `/api/optimizer/strategy/${encodeURIComponent(hash)}/diff/origin`,
  );
}

export function useOriginDiff(hash: string | null | undefined) {
  return useQuery({
    queryKey: [...autooptimizerKeys.all, "origin-diff", hash ?? ""] as const,
    queryFn: () => getOriginDiff(hash!),
    enabled: !!hash,
    staleTime: 60_000,
  });
}

export async function promoteStrategy(hash: string): Promise<{ strategy_id: string }> {
  return apiFetch<{ strategy_id: string }>(
    `/api/optimizer/strategy/${encodeURIComponent(hash)}/promote`,
    { method: "POST" },
  );
}

// ─── useCycleEvents + useRiver (optimizer redesign — Task 3) ─────────────────

/** A persisted optimizer cycle event row from the `/cycles/:id/events` endpoint. */
export type PersistedCycleEvent = {
  seq: number;
  session_id: string;
  cycle_id: string | null;
  kind: string;
  payload_json: string;
  ts: string;
};

/** A lineage node enriched with gate scores for the lineage-river chart. */
export type RiverNode = {
  bundle_hash: string;
  parent_hash: string | null;
  cycle_id: string | null;
  status: LineageStatus | string;
  created_at: string;
  child_day_score: number | null;
  delta_day: number | null;
};

/**
 * Fetch the persisted event log for a completed cycle (oldest-first).
 * Enabled only when `cycleId` is non-null. Returns an empty array gracefully
 * on backends that don't yet have the events table (fresh install).
 */
export function useCycleEvents(cycleId: string | null) {
  return useQuery<PersistedCycleEvent[]>({
    queryKey: autooptimizerKeys.cycleEvents(cycleId),
    queryFn: () =>
      apiFetch<PersistedCycleEvent[]>(`/api/autooptimizer/cycles/${cycleId}/events`),
    enabled: !!cycleId,
    staleTime: 60_000,
    retry: false, // endpoint may not exist on older backends
  });
}

/**
 * Fetch all lineage nodes joined with their gate scores for the river chart.
 * Refetches every 15 s when `opts.refetchIntervalWhileRunning` is true.
 */
export function useRiver(opts?: { refetchIntervalWhileRunning?: boolean }) {
  return useQuery<RiverNode[]>({
    queryKey: autooptimizerKeys.river(),
    queryFn: () => apiFetch<RiverNode[]>("/api/autooptimizer/river"),
    staleTime: 30_000,
    refetchInterval: opts?.refetchIntervalWhileRunning ? 15_000 : false,
    retry: false, // endpoint may not exist on older backends — consumers render their empty states
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
