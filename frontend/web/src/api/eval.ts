// Eval API — typed fetchers against `engine::api::eval::*`.

import { apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  errorSummary,
} from "@/lib/logger";
import type {
  ComparisonReport,
  RunDetail,
  RunMode,
  RunSummary,
} from "./types.gen";

type RunsListResponse = {
  items: RunSummary[];
};

// Hand-rolled — `EvalRunRequest` doesn't have ts-rs derives yet.
// Mirrors `xvision_engine::api::eval::EvalRunRequest`.
export type StartRunReq = {
  agent_id: string;
  scenario_id: string;
  mode: RunMode;
  params_override?: Record<string, unknown> | null;
};

export type ListRunsParams = {
  agent_id?: string;
  scenario_id?: string;
  status?: string;
};

export const evalKeys = {
  all: ["eval"] as const,
  runs: (params?: ListRunsParams) =>
    [
      ...evalKeys.all,
      "runs",
      params?.agent_id ?? "",
      params?.scenario_id ?? "",
      params?.status ?? "",
    ] as const,
  run: (id: string) => [...evalKeys.all, "run", id] as const,
  compare: (ids: string[]) =>
    [...evalKeys.all, "compare", ids.join(",")] as const,
};

export function listRuns(params?: ListRunsParams): Promise<RunSummary[]> {
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
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return apiFetch<RunsListResponse>(`/api/eval/runs${suffix}`).then(
    (r) => r.items,
  );
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
