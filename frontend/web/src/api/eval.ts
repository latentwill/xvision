// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import type { RunDetail, RunSummary } from "./types.gen";

export type RunsListResponse = {
  items: RunSummary[];
};

export const evalKeys = {
  all: ["eval"] as const,
  runs: () => [...evalKeys.all, "runs"] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
};

export function listRuns(): Promise<RunSummary[]> {
  return apiFetch<RunsListResponse>("/api/eval/runs").then((r) => r.items);
}

export function getRun(id: string): Promise<RunDetail> {
  return apiFetch<RunDetail>(`/api/eval/runs/${encodeURIComponent(id)}`);
}
