// Chart API — typed fetchers for the chart payload endpoints.

import { apiFetch } from "./client";
import type { CompareChartPayload, RunChartPayload } from "./types.gen";

export const chartKeys = {
  run: (id: string) => ["chart", "run", id] as const,
  compare: (ids: string[]) =>
    ["chart", "compare", ids.slice().sort().join(",")] as const,
};

export function getRunChart(runId: string): Promise<RunChartPayload> {
  return apiFetch<RunChartPayload>(
    `/api/eval/runs/${encodeURIComponent(runId)}/chart`,
  );
}

export function getCompareChart(runIds: string[]): Promise<CompareChartPayload> {
  return apiFetch<CompareChartPayload>(
    `/api/eval/runs/compare/chart?ids=${encodeURIComponent(runIds.join(","))}`,
  );
}
