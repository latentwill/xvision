// frontend/web/src/api/live-deployments.ts
//
// CT5 live-deployment surface — the honesty-constrained projection over
// `eval_runs WHERE mode='live'`, joined with broker/execution truth. See
// docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md.
//
// Wave 3b scope: the POLL only. `listDeployments()` hits
// `GET /api/live/deployments` (~5s refetch) and returns the full
// `LiveDeploymentSummary` rows — capital / P&L / drawdown are all sourced from
// THIS poll, never the SSE (per-tick capital streaming is deferred; §4 of the
// contract). Mirrors the `apiFetch` + `buildUrl` + cache-key pattern in
// `api/eval.ts` / `api/agent-runs.ts`.
//
// When the backend lands the per-deployment SSE bindings (§4) the
// `openDeploymentStream` helper + a hand-written `DeploymentStreamEvent` union
// move here; they are intentionally OUT OF SCOPE for 3b.

import { apiFetch } from "./client";
import type { LiveDeploymentSummary } from "./types.gen";

/// List-envelope returned by `GET /api/live/deployments`. `total` is the
/// pre-limit filtered count (§3). Hand-written because the dashboard route's
/// envelope is Serialize-only and not ts-rs-exported — replace with generated
/// bindings when the backend lands ts-rs derives.
type DeploymentsListResponse = {
  items: LiveDeploymentSummary[];
  total: number;
};

export type ListDeploymentsParams = {
  /// Status filter, comma-joined (e.g. `"running,paused"`). The default
  /// server filter is active-only; ActiveTasksStrip passes `running,paused`.
  status?: string;
  /// `"paper" | "live"` — venue-label filter. Absent => both.
  mode?: string;
  /// Page size. Server defaults to 20, caps at 100.
  limit?: number;
  /// bead s78.2: the operator's last-visit boundary (RFC-3339). When present,
  /// the backend populates `risk_veto_count_since_last_visit` with a REAL count
  /// of recorded risk-veto supervisor notes whose `created_at >= since`. Absent
  /// (first visit) => the field stays `null` (can't count "since an unknown
  /// time"). Invalid RFC-3339 => the endpoint returns 400.
  since?: string;
};

export const deploymentKeys = {
  all: ["live-deployments"] as const,
  /// Cache key folds the params into a stable tuple so a status/mode change
  /// refetches instead of slicing a single full-list result. Absent params
  /// collapse onto the same key as empty params (no extra fetch on first
  /// paint when a caller omits the object).
  list: (params?: ListDeploymentsParams) =>
    [
      ...deploymentKeys.all,
      "list",
      params?.status ?? "",
      params?.mode ?? "",
      params?.limit ?? null,
      params?.since ?? "",
    ] as const,
};

export function buildDeploymentsListUrl(params?: ListDeploymentsParams): string {
  const qs = new URLSearchParams();
  if (params?.status) {
    qs.set("status", params.status);
  }
  if (params?.mode) {
    qs.set("mode", params.mode);
  }
  if (params?.limit !== undefined) {
    qs.set("limit", String(params.limit));
  }
  if (params?.since) {
    qs.set("since", params.since);
  }
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return `/api/live/deployments${suffix}`;
}

/// List live/paper deployments. Returns just the rows (drops `total`) — the
/// ActiveTasksStrip 5s poll only needs list membership; the full
/// `LiveDeploymentSummary` (including the honest null-able capital fields) is
/// carried on each item. The endpoint is connection-as-data and never 500s on
/// a venue outage (§2.3), so a normal resolution may still carry rows with
/// `venue_connected=false` and `null` capital fields.
export function listDeployments(
  params?: ListDeploymentsParams,
): Promise<LiveDeploymentSummary[]> {
  return apiFetch<DeploymentsListResponse>(buildDeploymentsListUrl(params)).then(
    (r) => r.items ?? [],
  );
}
