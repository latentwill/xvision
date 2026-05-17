// frontend/web/src/api/agent-runs.ts
//
// The dashboard's agent-run surface. Two backends:
//
//   - Mock fixtures (Phase 0 / tests / local UI work without the daemon).
//   - Real HTTP + SSE (`/api/agent-runs/:id` and `/api/agent-runs/:id/stream`),
//     served by the `agent-run-observability-export-cli` track.
//
// Selection rule:
//   - `VITE_USE_MOCK_AGENT_RUNS=1`         -> mock
//   - else if MODE is `test` or `development` -> mock (so the UI works
//     before the backend lands; flip the flag to `0` to test against real)
//   - else                                  -> real HTTP
//
// When the backend is shipped and stable, drop the dev-default and require
// the flag explicitly.

import { ApiError, apiFetch } from "./client";
import {
  MOCK_RUN_COMPLETED,
  MOCK_RUN_ERROR,
  MOCK_RUN_FULL_DEBUG,
  MOCK_RUN_LIVE,
} from "@/features/agent-runs/mock-fixtures";
import type {
  AgentRunDetail,
  AgentRunStreamEvent,
  AgentRunSummary,
  RetentionMode,
  RunSpan,
  RunStatus,
} from "./types-agent-runs";

const MOCK_BY_ID: Record<string, AgentRunDetail> = {
  [MOCK_RUN_COMPLETED.summary.run_id]: MOCK_RUN_COMPLETED,
  [MOCK_RUN_LIVE.summary.run_id]: MOCK_RUN_LIVE,
  [MOCK_RUN_ERROR.summary.run_id]: MOCK_RUN_ERROR,
  [MOCK_RUN_FULL_DEBUG.summary.run_id]: MOCK_RUN_FULL_DEBUG,
};

export const agentRunKeys = {
  all: ["agent-runs"] as const,
  run: (id: string) => [...agentRunKeys.all, "run", id] as const,
};

/**
 * Whether the shim should serve mock fixtures instead of calling the real
 * backend. Exported for tests so they can flip the mode without
 * monkey-patching `import.meta.env`.
 */
export function shouldUseMockAgentRuns(): boolean {
  // `import.meta.env` is replaced at build time by Vite. Vitest's jsdom env
  // also provides it via the merged Vite config, with MODE === "test". We
  // read the env object dynamically (not via destructured property access)
  // so test setups can flip the flag with `vi.stubEnv` between cases.
  const meta = import.meta as unknown as {
    env?: Record<string, string | boolean | undefined>;
  };
  const env = meta.env ?? {};
  const explicit = env.VITE_USE_MOCK_AGENT_RUNS;
  if (explicit === "1" || explicit === "true" || explicit === true) return true;
  if (explicit === "0" || explicit === "false" || explicit === false) return false;
  if (env.MODE === "test" || env.MODE === "development") return true;
  return env.DEV === true || env.DEV === "true";
}

// ---------------------------------------------------------------------------
// Runtime shape validation
// ---------------------------------------------------------------------------
//
// We deliberately avoid pulling in zod for ~120 LOC of payload-shape checks.
// The validator returns a list of human-readable problems; a non-empty list
// throws an ApiError with code `invalid_response` so the UI surfaces a
// useful error in dev rather than silently rendering `undefined`.

const RUN_STATUSES: ReadonlySet<RunStatus> = new Set([
  "queued",
  "running",
  "completed",
  "failed",
  "cancelled",
]);
const RETENTION_MODES: ReadonlySet<RetentionMode> = new Set([
  "hash_only",
  "summaries",
  "full_debug",
]);

type Problem = string;

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function checkSummary(summary: unknown, problems: Problem[]): void {
  if (!isObject(summary)) {
    problems.push("summary: expected object");
    return;
  }
  const requiredStrings: Array<keyof AgentRunSummary> = ["run_id", "objective", "started_at"];
  for (const k of requiredStrings) {
    if (typeof summary[k as string] !== "string") {
      problems.push(`summary.${String(k)}: expected string`);
    }
  }
  const status = summary.status;
  if (typeof status !== "string" || !RUN_STATUSES.has(status as RunStatus)) {
    problems.push(`summary.status: expected one of ${[...RUN_STATUSES].join(",")}`);
  }
  if (summary.retention_mode === undefined) {
    problems.push("summary.retention_mode: missing (expected hash_only|summaries|full_debug)");
  } else if (
    typeof summary.retention_mode !== "string" ||
    !RETENTION_MODES.has(summary.retention_mode as RetentionMode)
  ) {
    problems.push(
      `summary.retention_mode: expected one of ${[...RETENTION_MODES].join(",")}`,
    );
  }
  const numericFields = [
    "span_count",
    "model_call_count",
    "tool_call_count",
    "error_count",
    "total_cost_usd",
    "total_input_tokens",
    "total_output_tokens",
  ] as const;
  for (const k of numericFields) {
    if (typeof summary[k] !== "number") {
      problems.push(`summary.${k}: expected number`);
    }
  }
}

function checkSpan(span: unknown, idx: number, problems: Problem[]): void {
  if (!isObject(span)) {
    problems.push(`spans[${idx}]: expected object`);
    return;
  }
  if (typeof span.span_id !== "string") problems.push(`spans[${idx}].span_id: expected string`);
  if (typeof span.name !== "string") problems.push(`spans[${idx}].name: expected string`);
  if (typeof span.kind !== "string") problems.push(`spans[${idx}].kind: expected string`);
  if (typeof span.started_at !== "string")
    problems.push(`spans[${idx}].started_at: expected string`);
}

/**
 * Validate the shape of an `AgentRunDetail` payload. Returns the payload
 * narrowed when valid; throws `ApiError("invalid_response")` otherwise.
 *
 * Exported for tests.
 */
export function validateAgentRunDetail(payload: unknown): AgentRunDetail {
  const problems: Problem[] = [];
  if (!isObject(payload)) {
    throw new ApiError(
      200,
      "invalid_response",
      "invalid agent-run payload: expected an object",
    );
  }
  checkSummary(payload.summary, problems);
  if (!Array.isArray(payload.spans)) {
    problems.push("spans: expected array");
  } else {
    payload.spans.forEach((s, i) => checkSpan(s, i, problems));
  }
  if (!Array.isArray(payload.model_calls)) problems.push("model_calls: expected array");
  if (!Array.isArray(payload.tool_calls)) problems.push("tool_calls: expected array");

  if (problems.length > 0) {
    throw new ApiError(
      200,
      "invalid_response",
      `agent-run payload failed validation: ${problems.slice(0, 5).join("; ")}`,
    );
  }
  return payload as AgentRunDetail;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function getAgentRun(id: string): Promise<AgentRunDetail> {
  if (shouldUseMockAgentRuns()) {
    const detail = MOCK_BY_ID[id];
    if (!detail) {
      throw new ApiError(404, "not_found", `agent run ${id} not found`);
    }
    // Simulate async, fixed delay — easy to remove when real API lands.
    await new Promise((r) => setTimeout(r, 30));
    return detail;
  }
  const payload = await apiFetch<unknown>(`/api/agent-runs/${encodeURIComponent(id)}`);
  return validateAgentRunDetail(payload);
}

// ---------------------------------------------------------------------------
// SSE
// ---------------------------------------------------------------------------

const SSE_BACKOFF_MS = [500, 1000, 2000, 4000, 8000];

type StreamHandle = () => void;

function openMockStream(
  runId: string,
  onEvent: (ev: AgentRunStreamEvent) => void,
): StreamHandle {
  const detail = MOCK_BY_ID[runId];
  if (!detail || detail.summary.status !== "running") {
    return () => {};
  }
  let tickCost = detail.summary.total_cost_usd;
  const interval = window.setInterval(() => {
    tickCost += 0.01;
    onEvent({
      event: "summary",
      data: {
        ...detail.summary,
        total_cost_usd: Number(tickCost.toFixed(4)),
      },
    });
  }, 800);
  return () => window.clearInterval(interval);
}

function openRealStream(
  runId: string,
  onEvent: (ev: AgentRunStreamEvent) => void,
): StreamHandle {
  let closed = false;
  let attempt = 0;
  let source: EventSource | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  const url = `/api/agent-runs/${encodeURIComponent(runId)}/stream`;

  const handle = (eventName: "span" | "summary") => (ev: MessageEvent) => {
    try {
      const data = JSON.parse(ev.data) as RunSpan | AgentRunSummary;
      onEvent({ event: eventName, data } as AgentRunStreamEvent);
    } catch {
      // Drop malformed frames — the validator will surface shape errors on
      // the next snapshot refetch.
    }
  };

  const connect = () => {
    if (closed) return;
    source = new EventSource(url);
    source.addEventListener("open", () => {
      attempt = 0;
    });
    source.addEventListener("span", handle("span") as EventListener);
    source.addEventListener("summary", handle("summary") as EventListener);
    source.addEventListener("error", () => {
      if (closed) return;
      source?.close();
      source = null;
      const delay = SSE_BACKOFF_MS[Math.min(attempt, SSE_BACKOFF_MS.length - 1)]!;
      attempt += 1;
      reconnectTimer = setTimeout(connect, delay);
    });
  };

  connect();

  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    source?.close();
    source = null;
  };
}

/**
 * Open a stream for a run. In mock mode emits a synthesized summary every
 * 800ms; in real mode connects to the SSE endpoint with exponential backoff
 * reconnect. Returns a close() function.
 */
export function openAgentRunStream(
  runId: string,
  onEvent: (ev: AgentRunStreamEvent) => void,
): StreamHandle {
  if (shouldUseMockAgentRuns()) {
    return openMockStream(runId, onEvent);
  }
  return openRealStream(runId, onEvent);
}
