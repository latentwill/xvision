// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import type {
  ComparisonReport,
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

export const evalKeys = {
  all: ["eval"] as const,
  runs: () => [...evalKeys.all, "runs"] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
  compare: (ids: string[]) =>
    [...evalKeys.all, "compare", ids.join(",")] as const,
  scenarios: () => [...evalKeys.all, "scenarios"] as const,
};

export function listRuns(): Promise<RunSummary[]> {
  return apiFetch<RunsListResponse>("/api/eval/runs").then((r) => r.items);
}

export function getRun(id: string): Promise<RunDetail> {
  return apiFetch<RunDetail>(`/api/eval/runs/${encodeURIComponent(id)}`);
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

/// Kick off a new eval run. Returns the queued `RunDetail` (status =
/// `Queued`); the actual run drives in a background task and progresses
/// to `Running` then `Completed` / `Failed`. Frontend polls
/// `GET /api/eval/runs/:id` until terminal.
export function startRun(req: StartRunReq): Promise<RunDetail> {
  return apiFetch<RunDetail>("/api/eval/runs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}
