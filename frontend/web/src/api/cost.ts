// Cost API — typed fetchers against `engine::api::cost::*` (bead-8wn).
//
// Mirrors the api/eval.ts pattern (apiFetch + buildUrl + keys). All `*_usd`
// fields are `number | null`; `null` is HONEST "no real cost data" / "cap
// UNSET" and is NEVER coerced to 0 here (§8.1/§8.9). Callers render null as an
// em-dash / "no cost data", never a faked figure.
//
// Routes (backend `crates/xvision-dashboard/src/routes/cost.rs`):
//   GET /api/cost/rollup?since=<rfc3339>  → CostRollupResponse  (read-only)
//   GET /api/cost/budget                  → CostBudgetResponse  (read-only)
//   PUT /api/cost/budget { daily_cap_usd }→ CostBudgetResponse  (mutation)

import { apiFetch } from "./client";
import type {
  CostBudgetResponse,
  CostRollupResponse,
} from "./types.gen";

export type { CostBudgetResponse, CostRollupResponse };

export interface RollupParams {
  /// Inclusive lower bound on run/cycle start, RFC-3339 (e.g.
  /// `2026-06-13T00:00:00Z`). Absent/empty => backend defaults to the
  /// trailing 24h. Mirrors `api/eval.ts`'s `since` convention.
  since?: string;
}

export const costKeys = {
  all: ["cost"] as const,
  /// Cache key varies on `since` so a window change refetches; empty `since`
  /// collapses onto the unscoped key (no extra fetch when the boundary is
  /// absent — e.g. a first visit with no last-visit stamp).
  rollup: (since?: string) =>
    [...costKeys.all, "rollup", since || ""] as const,
  budget: () => [...costKeys.all, "budget"] as const,
};

/// Build the rollup URL, threading an optional `since` (URL-encoded). An
/// absent/empty `since` yields the bare path so the backend applies its
/// trailing-24h default.
export function buildRollupUrl(params?: RollupParams): string {
  const qs = new URLSearchParams();
  if (params?.since) {
    qs.set("since", params.since);
  }
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return `/api/cost/rollup${suffix}`;
}

/// Windowed cross-source spend rollup. Every `*_usd` field may be `null`
/// (honest "unknown" / "UNSET"); the caller must render null distinctly from
/// a real $0.
export function getCostRollup(
  params?: RollupParams,
): Promise<CostRollupResponse> {
  return apiFetch<CostRollupResponse>(buildRollupUrl(params));
}

/// Read the persisted operator-set daily budget cap. `daily_cap_usd` is
/// `null` when UNSET (render em-dash, never a faked ceiling).
export function getCostBudget(): Promise<CostBudgetResponse> {
  return apiFetch<CostBudgetResponse>("/api/cost/budget");
}

/// Persist the operator-set daily budget cap (mutation, require_auth). The
/// backend rejects NaN / inf / <= 0 with a 400 validation error; surface that
/// to the caller as an `ApiError`.
export function setCostBudget(
  dailyCapUsd: number,
): Promise<CostBudgetResponse> {
  return apiFetch<CostBudgetResponse>("/api/cost/budget", {
    method: "PUT",
    body: JSON.stringify({ daily_cap_usd: dailyCapUsd }),
  });
}
