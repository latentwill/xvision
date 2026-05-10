// Strategies API — wraps `GET /api/strategies` with the engine-derived types.

import { apiFetch } from "./client";
import type { StrategySummary } from "./types.gen";

export type StrategiesListResponse = {
  items: StrategySummary[];
};

export const strategyKeys = {
  all: ["strategies"] as const,
  list: () => [...strategyKeys.all, "list"] as const,
};

export function listStrategies(): Promise<StrategySummary[]> {
  return apiFetch<StrategiesListResponse>("/api/strategies").then(
    (r) => r.items,
  );
}
