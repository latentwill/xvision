// Flywheel API — wraps dashboard routes for memory distillation,
// flywheel status, and memory-demo optimization.

import { apiFetch } from "./client";

export type FlywheelStatus = {
  namespace: string;
  observations: number;
  active_patterns: number;
  staged_patterns: number;
  forgotten_patterns: number;
  autooptimizer_runs: number;
  latest_autooptimizer_run_id?: string | null;
  latest_autooptimizer_created_at?: string | null;
};

export type FlywheelStatusQuery = {
  namespace?: string;
  agent?: string;
};

export type FlywheelVelocityQuery = FlywheelStatusQuery & {
  days?: number;
};

export type FlywheelVelocity = {
  namespace: string;
  days: number;
  since: string;
  observations_captured: number;
  patterns_promoted: number;
  patterns_demoted: number;
  autooptimizer_runs: number;
  optimized_child_agents: number;
  average_lineage_depth: number;
  latest_activity_at?: string | null;
};

export type FlywheelLineageQuery = FlywheelStatusQuery & {
  limit?: number;
};

export type FlywheelLineageItem = {
  optimization_id: string;
  target_agent_id: string;
  child_agent_id?: string | null;
  slot: string;
  method: string;
  demo_source: string;
  reproducible: boolean;
  holdout_split: string;
  cohort_query: string;
  train_observation_count: number;
  dev_observation_count: number;
  holdout_observation_count: number;
  train_hash: string;
  dev_hash: string;
  holdout_hash: string;
  demo_source_pattern_ids: string[];
  prior_pattern_ids: string[];
  prompt_prefix_chars: number;
  status: string;
  created_at: string;
  dev_metric?: string | null;
  holdout_metric?: string | null;
  parent_dev_score?: number | null;
  child_dev_score?: number | null;
  parent_holdout_score?: number | null;
  child_holdout_score?: number | null;
  gate_epsilon?: number | null;
  delta_dev?: number | null;
  delta_holdout?: number | null;
  gate_verdict?: string | null;
  gate_reason?: string | null;
  gated_at?: string | null;
};

export type FlywheelLineage = {
  namespace: string;
  items: FlywheelLineageItem[];
  total: number;
};

export type AutoOptimizerRunRequest = {
  namespace?: string;
  agent?: string;
  scenario_id?: string;
  run_id?: string;
  pattern_text: string;
  active?: boolean;
  limit?: number;
  min_observations?: number;
  embedding: number[];
  embedder_id?: string;
};

export type AutoOptimizerRun = {
  id: string;
  namespace: string;
  observation_ids: string[];
  pattern_id: string;
  pattern_text: string;
  promotion_state: string;
  min_observations: number;
  created_at: string;
  status: string;
  error?: string | null;
  gate_metric?: string | null;
  baseline_score?: number | null;
  candidate_score?: number | null;
  gate_threshold?: number | null;
  gate_passed?: boolean | null;
  gated_at?: string | null;
  finding_text?: string | null;
  finding_model?: string | null;
  finding_blind?: boolean | null;
  parent_day_score?: number | null;
  child_day_score?: number | null;
  parent_holdout_score?: number | null;
  child_holdout_score?: number | null;
  gate_epsilon?: number | null;
  delta_day?: number | null;
  delta_holdout?: number | null;
  gate_verdict?: string | null;
  gate_reason?: string | null;
  qualitative_finding_json?: string | null;
  finding_blinded_metrics?: boolean | null;
  judge_model?: string | null;
  judge_token_cost?: number | null;
};

export type AutoOptimizerGateRequest = {
  metric?: string;
  baseline_score?: number;
  candidate_score?: number;
  min_delta?: number;
  finding_text?: string;
  finding_model?: string;
  promote_if_pass?: boolean;
  parent_day_score?: number;
  child_day_score?: number;
  parent_holdout_score?: number;
  child_holdout_score?: number;
  gate_epsilon?: number;
  gate_reason?: string;
  qualitative_finding_json?: string;
  finding_blinded_metrics?: boolean;
  judge_model?: string;
  judge_token_cost?: number;
};

export type AutoOptimizerRunListQuery = {
  namespace?: string;
  agent?: string;
  limit?: number;
  offset?: number;
};

export type AutoOptimizerRunList = {
  items: AutoOptimizerRun[];
  total: number;
};

export type MemoryDemoObservation = {
  id: string;
  run_id: string;
  scenario_id: string;
  cycle_idx: number;
  source_window_end: string;
};

export type MemoryDemoOptimizeRequest = {
  target_agent_id: string;
  slot?: string;
  namespace?: string;
  memory_agent?: string;
  scenario_id?: string;
  run_id?: string;
  demo_source?: string;
  holdout_split?: string;
  cohort_query?: string;
  manual_observation_ids?: string[];
  prior_pattern_ids?: string[];
  auto_prior_patterns?: boolean;
  prior_pattern_limit?: number;
  limit?: number;
  max_demo_chars?: number;
  apply?: boolean;
  child_name?: string;
};

export type MemoryDemoOptimizeResult = {
  optimization_id?: string | null;
  status: string;
  namespace: string;
  target_agent_id: string;
  child_agent_id?: string | null;
  slot: string;
  demo_count: number;
  demo_source?: string;
  reproducible?: boolean;
  holdout_split?: string;
  cohort_query?: string;
  observation_ids: string[];
  train_observation_ids?: string[];
  dev_observation_ids?: string[];
  holdout_observation_ids?: string[];
  train_hash?: string;
  dev_hash?: string;
  holdout_hash?: string;
  demo_source_pattern_ids?: string[];
  pattern_demo_source_count?: number;
  prior_pattern_ids?: string[];
  pattern_prior_count?: number;
  observations: MemoryDemoObservation[];
  prompt_prefix_chars: number;
  prompt_preview?: string | null;
};

export type OptimizationGateRequest = {
  dev_metric?: string;
  holdout_metric?: string;
  parent_dev_score: number;
  child_dev_score: number;
  parent_holdout_score: number;
  child_holdout_score: number;
  gate_epsilon?: number;
  gate_reason?: string;
};

export type OptimizationGateResult = {
  optimization_id: string;
  dev_metric: string;
  holdout_metric: string;
  parent_dev_score: number;
  child_dev_score: number;
  parent_holdout_score: number;
  child_holdout_score: number;
  gate_epsilon: number;
  delta_dev: number;
  delta_holdout: number;
  gate_verdict: string;
  gate_reason: string;
  gated_at: string;
};

function buildStatusUrl(q?: FlywheelStatusQuery): string {
  const params = new URLSearchParams();
  if (q?.namespace) params.set("namespace", q.namespace);
  if (q?.agent) params.set("agent", q.agent);
  const qs = params.toString();
  return qs ? `/api/flywheel/status?${qs}` : "/api/flywheel/status";
}

function buildVelocityUrl(q?: FlywheelVelocityQuery): string {
  const params = new URLSearchParams();
  if (q?.namespace) params.set("namespace", q.namespace);
  if (q?.agent) params.set("agent", q.agent);
  if (q?.days != null) params.set("days", String(q.days));
  const qs = params.toString();
  return qs ? `/api/flywheel/velocity?${qs}` : "/api/flywheel/velocity";
}

function buildLineageUrl(q?: FlywheelLineageQuery): string {
  const params = new URLSearchParams();
  if (q?.namespace) params.set("namespace", q.namespace);
  if (q?.agent) params.set("agent", q.agent);
  if (q?.limit != null) params.set("limit", String(q.limit));
  const qs = params.toString();
  return qs ? `/api/flywheel/lineage?${qs}` : "/api/flywheel/lineage";
}

function buildAutoOptimizerListUrl(q?: AutoOptimizerRunListQuery): string {
  const params = new URLSearchParams();
  if (q?.namespace) params.set("namespace", q.namespace);
  if (q?.agent) params.set("agent", q.agent);
  if (q?.limit != null) params.set("limit", String(q.limit));
  if (q?.offset != null) params.set("offset", String(q.offset));
  const qs = params.toString();
  return qs ? `/api/autooptimizer?${qs}` : "/api/autooptimizer";
}

export async function getFlywheelStatus(
  q?: FlywheelStatusQuery,
): Promise<FlywheelStatus> {
  return apiFetch<FlywheelStatus>(buildStatusUrl(q));
}

export async function getFlywheelVelocity(
  q?: FlywheelVelocityQuery,
): Promise<FlywheelVelocity> {
  return apiFetch<FlywheelVelocity>(buildVelocityUrl(q));
}

export async function getFlywheelLineage(
  q?: FlywheelLineageQuery,
): Promise<FlywheelLineage> {
  return apiFetch<FlywheelLineage>(buildLineageUrl(q));
}

export async function runAutoOptimizer(
  body: AutoOptimizerRunRequest,
): Promise<AutoOptimizerRun> {
  return apiFetch<AutoOptimizerRun>("/api/autooptimizer/run", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function getAutoOptimizerRun(id: string): Promise<AutoOptimizerRun> {
  return apiFetch<AutoOptimizerRun>(
    `/api/autooptimizer/${encodeURIComponent(id)}`,
  );
}

export async function listAutoOptimizerRuns(
  q?: AutoOptimizerRunListQuery,
): Promise<AutoOptimizerRunList> {
  return apiFetch<AutoOptimizerRunList>(buildAutoOptimizerListUrl(q));
}

export async function promoteAutoOptimizerRun(
  id: string,
): Promise<AutoOptimizerRun> {
  return apiFetch<AutoOptimizerRun>(
    `/api/autooptimizer/${encodeURIComponent(id)}/promote`,
    { method: "POST" },
  );
}

export async function gateAutoOptimizerRun(
  id: string,
  body: AutoOptimizerGateRequest,
): Promise<AutoOptimizerRun> {
  return apiFetch<AutoOptimizerRun>(
    `/api/autooptimizer/${encodeURIComponent(id)}/gate`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export async function demoteAutoOptimizerRun(
  id: string,
): Promise<AutoOptimizerRun> {
  return apiFetch<AutoOptimizerRun>(
    `/api/autooptimizer/${encodeURIComponent(id)}/demote`,
    { method: "POST" },
  );
}

export async function optimizeMemoryDemos(
  body: MemoryDemoOptimizeRequest,
): Promise<MemoryDemoOptimizeResult> {
  return apiFetch<MemoryDemoOptimizeResult>("/api/optimize/memory-demos", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function gateOptimization(
  optimizationId: string,
  body: OptimizationGateRequest,
): Promise<OptimizationGateResult> {
  return apiFetch<OptimizationGateResult>(
    `/api/optimize/memory-demos/${encodeURIComponent(optimizationId)}/gate`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export const flywheelKeys = {
  all: ["flywheel"] as const,
  status: (q?: FlywheelStatusQuery) =>
    [...flywheelKeys.all, "status", q ?? {}] as const,
  velocity: (q?: FlywheelVelocityQuery) =>
    [...flywheelKeys.all, "velocity", q ?? {}] as const,
  lineage: (q?: FlywheelLineageQuery) =>
    [...flywheelKeys.all, "lineage", q ?? {}] as const,
  autooptimizer: (id: string) =>
    [...flywheelKeys.all, "autooptimizer", id] as const,
  autooptimizerList: (q?: AutoOptimizerRunListQuery) =>
    [...flywheelKeys.all, "autooptimizer-list", q ?? {}] as const,
};
