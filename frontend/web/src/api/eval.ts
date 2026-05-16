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
