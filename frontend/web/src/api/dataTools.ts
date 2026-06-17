// Data-tools settings API — GET / PUT for `[[data_tools]]` in the
// workspace config (Nansen, Elfa, …).
//
// Hand-written wire shapes (no types.gen regen) matching `DataToolEntry` /
// `DataToolsReport` / `SetDataToolsRequest` in
// `crates/xvision-engine/src/api/settings/data_tools.rs`.

import { apiFetch } from "./client";
import { settingsKeys } from "./settings";

// ── Wire types ───────────────────────────────────────────────────────────────

export interface DataToolEntry {
  kind: "nansen" | "elfa";
  base_url: string;
  /** Env-var NAME only — the secret never round-trips through this API. */
  api_key_env: string;
  enabled: boolean;
  budget_credits_per_run: number | null;
  nansen_lookahead_lag_days: number | null;
}

/** Shape returned by GET /api/settings/data-tools. */
export interface DataToolsReport {
  data_tools: DataToolEntry[];
}

/** Shape accepted by PUT /api/settings/data-tools. */
export interface SetDataToolsRequest {
  data_tools: DataToolEntry[];
}

// ── TanStack Query keys ──────────────────────────────────────────────────────

export const dataToolsKeys = {
  all: [...settingsKeys.all, "data-tools"] as const,
  list: () => [...dataToolsKeys.all] as const,
};

// ── Fetchers ─────────────────────────────────────────────────────────────────

export function getDataTools(): Promise<DataToolsReport> {
  return apiFetch<DataToolsReport>("/api/settings/data-tools");
}

export function setDataTools(req: SetDataToolsRequest): Promise<DataToolsReport> {
  return apiFetch<DataToolsReport>("/api/settings/data-tools", {
    method: "PUT",
    body: JSON.stringify(req),
  });
}
