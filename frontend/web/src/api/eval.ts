// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import type {
  ComparisonReport,
  EvalRunRequest,
  RunDetail,
  RunMode,
  RunSummary,
} from "./types.gen";

export type RunsListResponse = {
  items: RunSummary[];
};

// Hand-rolled — `ScenarioSummary` doesn't have ts-rs derives yet on the
// engine side. Mirrors `xvision_engine::api::eval::ScenarioSummary`.
export type ScenarioSummary = {
  id: string;
  display_name: string;
  asset_universe: string[];
  regime_tags: string[];
  time_window_days: number;
};

export type ScenariosListResponse = {
  items: ScenarioSummary[];
};

// Hand-rolled — `EvalRunRequest` doesn't have ts-rs derives yet.
// Mirrors `xvision_engine::api::eval::EvalRunRequest`.
export type StartRunReq = {
  agent_id: string;
  scenario_id: string;
  mode: RunMode;
  params_override?: Record<string, unknown> | null;
};

export type ListRunsParams = {
  agent_id?: string;
  scenario_id?: string;
  status?: string;
};

export const evalKeys = {
  all: ["eval"] as const,
  runs: (params?: ListRunsParams) =>
    [
      ...evalKeys.all,
      "runs",
      params?.agent_id ?? "",
      params?.scenario_id ?? "",
      params?.status ?? "",
    ] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
  compare: (ids: string[]) =>
    [...evalKeys.all, "compare", ids.join(",")] as const,
  scenarios: () => [...evalKeys.all, "scenarios"] as const,
};

export function listRuns(params?: ListRunsParams): Promise<RunSummary[]> {
  const qs = new URLSearchParams();
  if (params?.agent_id) {
    qs.set("agent_id", params.agent_id);
  }
  if (params?.scenario_id) {
    qs.set("scenario_id", params.scenario_id);
  }
  if (params?.status) {
    qs.set("status", params.status);
  }
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return apiFetch<RunsListResponse>(`/api/eval/runs${suffix}`).then(
    (r) => r.items,
  );
}

export function getRun(id: string): Promise<RunDetail> {
  return apiFetch<RunDetail>(`/api/eval/runs/${encodeURIComponent(id)}`);
}

export function deleteRun(id: string): Promise<void> {
  return apiFetch<void>(`/api/eval/runs/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export function cancelRun(id: string): Promise<RunSummary> {
  return apiFetch<RunSummary>(
    `/api/eval/runs/${encodeURIComponent(id)}/cancel`,
    {
      method: "POST",
    },
  );
}

export function compareRuns(ids: string[]): Promise<ComparisonReport> {
  const qs = ids.map(encodeURIComponent).join(",");
  return apiFetch<ComparisonReport>(`/api/eval/compare?ids=${qs}`);
}

/// List the canonical scenarios for the start-run dropdown. Backend
/// list is small and static; safe to cache.
export function listScenarios(): Promise<ScenarioSummary[]> {
  return apiFetch<ScenariosListResponse>("/api/eval/scenarios").then(
    (r) => r.items,
  );
}

/// Kick off a new eval run (non-blocking). Returns the queued `RunDetail`
/// (status = `Queued`); the actual run drives in a background task and
/// progresses to `Running` then `Completed` / `Failed`. Frontend polls
/// `GET /api/eval/runs/:id` until terminal.
export function startRun(req: StartRunReq): Promise<RunDetail> {
  return apiFetch<RunDetail>("/api/eval/runs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

/// Kick off a new eval run (synchronous, blocking). Returns the slim
/// `RunSummary` after the run completes. For testing/CLI flows where
/// you need a completed result directly.
export function runEval(req: EvalRunRequest): Promise<RunSummary> {
  return apiFetch<RunSummary>("/api/eval/runs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}
