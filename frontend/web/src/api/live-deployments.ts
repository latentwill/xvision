// frontend/web/src/api/live-deployments.ts
//
// API client for the CT5 live-deployments surface.
// Endpoint: GET /api/live/deployments
// Response: BARE JSON ARRAY LiveDeploymentSummary[] (NOT wrapped in an envelope)
// The Rust handler returns `Json(Vec<LiveDeploymentSummary>)` directly.
//
// Also exposes a single-deployment getter:
//   GET /api/live/deployments/:id → LiveDeploymentSummary
//
// Mirror of agent-runs.ts / eval.ts patterns.

import { apiFetch } from "./client";
import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";

// ─── query keys ──────────────────────────────────────────────────────────────

export const liveDeploymentKeys = {
  all: ["live-deployments"] as const,
  list: (params?: { status?: string }) =>
    [...liveDeploymentKeys.all, "list", params ?? {}] as const,
  one: (id: string) => [...liveDeploymentKeys.all, "one", id] as const,
};

// ─── API fetchers ─────────────────────────────────────────────────────────────

/**
 * List live deployments.
 *
 * Returns a BARE ARRAY (no envelope) — the Rust handler serialises
 * `Json(Vec<LiveDeploymentSummary>)` directly so the top-level JSON value
 * is an array, not an object.
 *
 * @param params.status  Optional status filter passed as `?status=<value>`.
 *                       e.g. "running", "paused", "completed".
 */
export async function listLiveDeployments(
  params?: { status?: string },
): Promise<LiveDeploymentSummary[]> {
  const qs = new URLSearchParams();
  if (params?.status) qs.set("status", params.status);
  return apiFetch<LiveDeploymentSummary[]>(
    `/api/live/deployments${qs.toString() ? `?${qs}` : ""}`,
  );
}

/**
 * Fetch a single live deployment by id.
 *
 * @param id  The `deployment_id` string (ULID).
 */
export async function getLiveDeployment(id: string): Promise<LiveDeploymentSummary> {
  return apiFetch<LiveDeploymentSummary>(
    `/api/live/deployments/${encodeURIComponent(id)}`,
  );
}
