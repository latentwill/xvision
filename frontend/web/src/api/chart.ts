// Chart API — typed fetchers for the chart payload endpoints.

import { apiFetch } from "./client";
import type {
  CompareChartPayload,
  RunChartPayload,
  ScenarioChartPayload,
  ScenarioPreviewPayload,
  StrategyChartPayload,
} from "./types.gen";

export const chartKeys = {
  run: (id: string) => ["chart", "run", id] as const,
  compare: (ids: string[]) =>
    ["chart", "compare", ids.slice().sort().join(",")] as const,
};

export const scenarioChartKeys = {
  scenario: (id: string) => ["chart", "scenario", id] as const,
};

export const strategyChartKeys = {
  strategy: (id: string) => ["chart", "strategy", id] as const,
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

export function getScenarioChart(
  scenarioId: string,
): Promise<ScenarioChartPayload> {
  return apiFetch<ScenarioChartPayload>(
    `/api/scenarios/${encodeURIComponent(scenarioId)}/chart`,
  );
}

export function getStrategyChart(
  strategyId: string,
): Promise<StrategyChartPayload> {
  return apiFetch<StrategyChartPayload>(
    `/api/strategies/${encodeURIComponent(strategyId)}/chart`,
  );
}

export function openRunStream(runId: string): EventSource {
  return new EventSource(`/api/eval/runs/${encodeURIComponent(runId)}/stream`);
}

export async function getScenarioPreview(params: {
  asset: string;
  from: string;
  to: string;
  granularity: "1h" | "1d";
  baseline?: boolean;
}): Promise<ScenarioPreviewPayload> {
  const q = new URLSearchParams({
    asset: params.asset,
    from: params.from,
    to: params.to,
    granularity: params.granularity,
  });
  if (params.baseline !== undefined) q.set("baseline", String(params.baseline));
  return apiFetch<ScenarioPreviewPayload>(`/api/scenarios/preview?${q}`);
}
