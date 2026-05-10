// `/api/health` fetcher — typed against the engine's HealthReport.

import { apiFetch } from "./client";
import type { HealthReport } from "./types.gen";

export const healthKeys = {
  all: ["health"] as const,
  report: () => [...healthKeys.all, "report"] as const,
};

export function getHealth(): Promise<HealthReport> {
  return apiFetch<HealthReport>("/api/health");
}
