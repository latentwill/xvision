// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import type { ComparisonReport, EvalRunRequest, RunDetail, RunSummary } from "./types.gen";

export type RunsListResponse = {
  items: RunSummary[];
};

export const evalKeys = {
  all: ["eval"] as const,
  runs: () => [...evalKeys.all, "runs"] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
  compare: (ids: string[]) => [...evalKeys.all, "compare", ids.join(",")] as const,
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

export function runEval(req: EvalRunRequest): Promise<RunSummary> {
  return apiFetch<RunSummary>("/api/eval/runs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}
