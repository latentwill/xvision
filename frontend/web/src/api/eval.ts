// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  errorSummary,
} from "@/lib/logger";
import type {
  ComparisonReport,
  LiveConfig,
  ReviewModel,
  RunDetail,
  RunMode,
  RunSummary,
} from "./types.gen";

type RunsListResponse = {
  items: RunSummary[];
  /// Total row count matching the filter, BEFORE LIMIT/OFFSET. Added
  /// by the backend pagination follow-up to PR #386's list wave; the
  /// SPA uses this for "page X of N" without a second round-trip.
  total: number;
};

/// Paged response envelope returned by `listRunsPaged`. `listRuns`
/// drops `total` and returns just the items because most call sites
/// (chart preview, retry idempotency) only need the rows.
export type RunsPage = {
  items: RunSummary[];
  total: number;
};

// Hand-rolled — `EvalRunRequest` doesn't have ts-rs derives yet.
// Mirrors `xvision_engine::api::eval::EvalRunRequest`.
export type StartRunReq = {
  agent_id: string;
  scenario_id: string;
  mode: RunMode;
  params_override?: Record<string, unknown> | null;
  live_config?: LiveConfig | null;
  auto_fire_review?: boolean;
  review_model?: ReviewModel | null;
  max_annotations_per_review?: number | null;
};

export type ListRunsParams = {
  agent_id?: string;
  scenario_id?: string;
  status?: string;
  /// Page size. Server defaults to 50, caps at 200.
  limit?: number;
  /// Row offset. Server treats `undefined` as 0.
  offset?: number;
  /// Inclusive lower bound on `started_at`, RFC-3339 (e.g.
  /// `2026-06-06T00:00:00Z`). Backend returns rows WHERE
  /// `started_at >= since`; absent/empty => no filter (bead-008).
  since?: string;
};

export const evalKeys = {
  all: ["eval"] as const,
  /// Cache key includes `limit`/`offset` so page changes refetch
  /// instead of slicing a single full-list result. Required by the
  /// backend-pagination follow-up to #386.
  runs: (params?: ListRunsParams) =>
    [
      ...evalKeys.all,
      "runs",
      params?.agent_id ?? "",
      params?.scenario_id ?? "",
      params?.status ?? "",
      params?.limit ?? null,
      params?.offset ?? null,
      // Empty `since` is treated identically to absent so the default 'All'
      // window collapses onto the unscoped key (no extra fetch on first paint).
      params?.since || "",
    ] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
  compare: (ids: string[]) =>
    [...evalKeys.all, "compare", ids.join(",")] as const,
};

export function buildRunsListUrl(params?: ListRunsParams): string {
  const qs = new URLSearchParams();
  if (params?.agent_id) {
    qs.set("agent_id", params.agent_id);
  }
  if (params?.scenario_id) {
    qs.set("scenario_id", params.scenario_id);
  }
  if (params?.status) {
    qs.set("status", params.status);
  }
  if (params?.limit !== undefined) {
    qs.set("limit", String(params.limit));
  }
  if (params?.offset !== undefined) {
    qs.set("offset", String(params.offset));
  }
  if (params?.since) {
    qs.set("since", params.since);
  }
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return `/api/eval/runs${suffix}`;
}

export function listRuns(params?: ListRunsParams): Promise<RunSummary[]> {
  return apiFetch<RunsListResponse>(buildRunsListUrl(params)).then(
    (r) => r.items,
  );
}

/// Paged variant — preserves the `total` field so the dashboard's
/// `ListPagination` primitive can render "page X of N" without a
/// second round-trip.
export function listRunsPaged(params?: ListRunsParams): Promise<RunsPage> {
  return apiFetch<RunsListResponse>(buildRunsListUrl(params)).then((r) => ({
    items: r.items,
    total: r.total,
  }));
}

export function getRun(id: string): Promise<RunDetail> {
  return apiFetch<RunDetail>(`/api/eval/runs/${encodeURIComponent(id)}`);
}

export function deleteRun(id: string): Promise<void> {
  return apiFetch<void>(`/api/eval/runs/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export function cancelRun(id: string): Promise<RunSummary> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.cancel.start");
  return apiFetch<RunSummary>(
    `/api/eval/runs/${encodeURIComponent(id)}/cancel`,
    {
      method: "POST",
    },
  )
    .then((run) => {
      trace.info("eval.cancel.ok", {
        status: run.status,
        duration_ms: durationSince(started),
      });
      return run;
    })
    .catch((err) => {
      trace.error("eval.cancel.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

/// Pause a live run's broker submits without stopping the run loop.
/// Hits `POST /api/eval/runs/:id/pause`; the returned `RunSummary` has
/// `paused: true` + a fresh `paused_at`. Mirrors `cancelRun` / `pauseSafety`.
export function pauseRun(id: string): Promise<RunSummary> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.pause.start");
  return apiFetch<RunSummary>(
    `/api/eval/runs/${encodeURIComponent(id)}/pause`,
    {
      method: "POST",
    },
  )
    .then((run) => {
      trace.info("eval.pause.ok", {
        paused: run.paused,
        duration_ms: durationSince(started),
      });
      return run;
    })
    .catch((err) => {
      trace.error("eval.pause.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

/// Resume a paused live run. Hits `POST /api/eval/runs/:id/resume`; the
/// returned `RunSummary` has `paused: false`. Mirrors `pauseRun`.
export function resumeRun(id: string): Promise<RunSummary> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.resume.start");
  return apiFetch<RunSummary>(
    `/api/eval/runs/${encodeURIComponent(id)}/resume`,
    {
      method: "POST",
    },
  )
    .then((run) => {
      trace.info("eval.resume.ok", {
        paused: run.paused,
        duration_ms: durationSince(started),
      });
      return run;
    })
    .catch((err) => {
      trace.error("eval.resume.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

/// Request a one-shot "flatten positions" for a live run: close ALL open
/// broker positions on the next cycle WITHOUT terminating the run. Hits
/// `POST /api/eval/runs/:id/flatten`; the returned `RunSummary` has
/// `flatten_requested: true` (cleared by the executor once it acts). Mirrors
/// `pauseRun` / `cancelRun`. Live Trading fires this from the [Flatten
/// positions] inline action after a pause (spec §2.7).
export function flattenRun(id: string): Promise<RunSummary> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.flatten.start");
  return apiFetch<RunSummary>(
    `/api/eval/runs/${encodeURIComponent(id)}/flatten`,
    {
      method: "POST",
    },
  )
    .then((run) => {
      trace.info("eval.flatten.ok", {
        flatten_requested: run.flatten_requested,
        duration_ms: durationSince(started),
      });
      return run;
    })
    .catch((err) => {
      trace.error("eval.flatten.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

/// Re-queue a failed eval run with the same inputs as the source.
/// Resolves to the `RunDetail` of the freshly-queued run (or the
/// existing in-flight retry if one was already queued/running for the
/// same fingerprint).
export function retryRun(id: string): Promise<RunDetail> {
  const trace = createTrace("eval", { source_run_id: id });
  const started = performance.now();
  trace.info("eval.retry.start");
  return apiFetch<RunDetail>(
    `/api/eval/runs/${encodeURIComponent(id)}/retry`,
    {
      method: "POST",
    },
  )
    .then((detail) => {
      trace.info("eval.retry.queued", {
        new_run_id: detail.summary.id,
        status: detail.summary.status,
        duration_ms: durationSince(started),
      });
      return detail;
    })
    .catch((err) => {
      trace.error("eval.retry.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}


/// `POST /api/eval/runs/:id/reconnect` — resume a disconnected
/// live/forward-test run from its last persisted bar.
export function reconnectRun(id: string): Promise<RunDetail> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.reconnect.start");
  return apiFetch<RunDetail>(
    `/api/eval/runs/${encodeURIComponent(id)}/reconnect`,
    { method: "POST" },
  ).then((r) => {
    trace.info("eval.reconnect.ok", {
      duration_ms: Math.round(durationSince(started)),
    });
    return r;
  }).catch((e) => {
    trace.error("eval.reconnect.err", errorSummary(e));
    throw e;
  });
}

/// `POST /api/eval/runs/:id/reconcile` — query broker for open
/// positions and diff against xvision's expected book state.
export function reconcileRun(
  id: string,
): Promise<import("./types.gen/ReconcileOutcome").ReconcileOutcome> {
  const trace = createTrace("eval", { run_id: id });
  const started = performance.now();
  trace.info("eval.reconcile.start");
  return apiFetch<import("./types.gen/ReconcileOutcome").ReconcileOutcome>(
    `/api/eval/runs/${encodeURIComponent(id)}/reconcile`,
    { method: "POST" },
  ).then((r) => {
    trace.info("eval.reconcile.ok", {
      duration_ms: Math.round(durationSince(started)),
    });
    return r;
  }).catch((e) => {
    trace.error("eval.reconcile.err", errorSummary(e));
    throw e;
  });
}
export function compareRuns(ids: string[]): Promise<ComparisonReport> {
  const qs = ids.map(encodeURIComponent).join(",");
  return apiFetch<ComparisonReport>(`/api/eval/compare?ids=${qs}`);
}

/// Fetch the full `EvalRunExport` JSON for a terminal run. Returned as
/// `unknown` because the shape mirrors the spec §3 envelope verbatim and
/// isn't ts-rs-exported — consumers should treat it as opaque JSON for
/// QA round-trip rather than reading individual fields. Server is
/// authoritative; the engine's `xvision_engine::eval::export` builds
/// this for both this route and `xvn eval export`.
export function fetchEvalRunExport(id: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/eval/runs/${encodeURIComponent(id)}/export`);
}

/// Trigger a browser download of an eval run's export JSON. Used by
/// the "Download JSON" button on run-detail for terminal runs (q15 §3).
/// Stays a pure frontend helper so the dashboard route can keep
/// `Content-Type: application/json` (rather than forcing
/// `Content-Disposition: attachment` server-side — that breaks
/// `xvn eval export` / curl piping use cases).
export async function downloadEvalRunExport(id: string): Promise<void> {
  const trace = createTrace("eval", { run_id: id, op: "download_export" });
  const started = performance.now();
  trace.info("eval.export.download.start");
  try {
    const payload = await fetchEvalRunExport(id);
    const json = JSON.stringify(payload, null, 2);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    try {
      const a = document.createElement("a");
      a.href = url;
      a.download = `eval-run-${id}.json`;
      a.style.display = "none";
      document.body.appendChild(a);
      a.click();
      a.remove();
    } finally {
      // Release the object URL on the next tick; some browsers cancel
      // the download if we revoke synchronously.
      setTimeout(() => URL.revokeObjectURL(url), 0);
    }
    trace.info("eval.export.download.ok", {
      duration_ms: durationSince(started),
      bytes: blob.size,
    });
  } catch (err) {
    trace.error("eval.export.download.error", {
      duration_ms: durationSince(started),
      error: errorSummary(err),
    });
    throw err;
  }
}

/// Kick off a new eval run (non-blocking). Returns the queued `RunDetail`
/// (status = `Queued`); the actual run drives in a background task and
/// progresses to `Running` then `Completed` / `Failed`. Frontend polls
/// `GET /api/eval/runs/:id` until terminal.
export function startRun(req: StartRunReq): Promise<RunDetail> {
  const trace = createTrace("eval", {
    strategy_id: req.agent_id,
    scenario_id: req.scenario_id,
    mode: req.mode,
  });
  const started = performance.now();
  trace.info("eval.launch.start");
  return apiFetch<RunDetail>("/api/eval/runs", {
    method: "POST",
    body: JSON.stringify(req),
  })
    .then((run) => {
      trace.info("eval.launch.queued", {
        run_id: run.summary.id,
        status: run.summary.status,
        duration_ms: durationSince(started),
      });
      return run;
    })
    .catch((err) => {
      trace.error("eval.launch.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}
