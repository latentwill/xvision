// Eval API — `GET /api/eval/runs` typed against the engine's RunSummary.

import { apiFetch } from "./client";
import type { RunSummary } from "./types.gen";

export type RunsListResponse = {
  items: RunSummary[];
};

export const evalKeys = {
  all: ["eval"] as const,
  runs: () => [...evalKeys.all, "runs"] as const,
};

export function listRuns(): Promise<RunSummary[]> {
  return apiFetch<RunsListResponse>("/api/eval/runs").then((r) => r.items);
}
