// Chart API — typed fetchers for the chart payload endpoints.

import { apiFetch } from "./client";
import { logInfo } from "@/lib/logger";
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
  scenario: (id: string, granularity?: string | null, asset?: string | null) =>
    ["chart", "scenario", id, granularity ?? "stored", asset ?? "default"] as const,
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
  granularity?: string | null,
  asset?: string | null,
): Promise<ScenarioChartPayload> {
  const q = new URLSearchParams();
  if (granularity) q.set("granularity", granularity);
  if (asset) q.set("asset", asset);
  const suffix = q.size > 0 ? `?${q}` : "";
  return apiFetch<ScenarioChartPayload>(
    `/api/scenarios/${encodeURIComponent(scenarioId)}/chart${suffix}`,
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
  const path = `/api/eval/runs/${encodeURIComponent(runId)}/stream`;
  logInfo("stream", "stream.open", { run_id: runId, path });
  return new EventSource(path);
}

// Charts dashboard section (chart-rework Track B).

import type {
  AnnotatedChartPayload,
  MarketContextPayload,
  MultiStrategyEquityBundle,
} from "@/components/chart/v2/types";

export const dashboardChartKeys = {
  overview: () => ["chart", "dashboards", "overview"] as const,
};

export function getDashboardOverview(): Promise<MultiStrategyEquityBundle> {
  return apiFetch<MultiStrategyEquityBundle>(
    `/api/v2/charts/dashboards/overview`,
  );
}

// B3 — AI annotation chart fetchers.

export type AnnotatedSource = "run" | "live";

export const annotatedChartKeys = {
  run: (runId: string) => ["chart", "annotated", "run", runId] as const,
  live: (symbol: string) => ["chart", "annotated", "live", symbol] as const,
};

export function getAnnotatedRun(runId: string): Promise<AnnotatedChartPayload> {
  return apiFetch<AnnotatedChartPayload>(
    `/api/v2/charts/annotated/${encodeURIComponent(runId)}`,
  );
}

export function getAnnotatedLive(symbol: string): Promise<AnnotatedChartPayload> {
  return apiFetch<AnnotatedChartPayload>(
    `/api/v2/charts/annotated/live/${encodeURIComponent(symbol)}`,
  );
}

// B4 follow-up — market context for MarketContextCard.

export const marketContextKeys = {
  get: () => ["chart", "market-context"] as const,
};

export function getMarketContext(): Promise<MarketContextPayload> {
  return apiFetch<MarketContextPayload>(`/api/v2/charts/market-context`);
}

export async function getScenarioPreview(params: {
  asset: string;
  from: string;
  to: string;
  granularity: string;
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
