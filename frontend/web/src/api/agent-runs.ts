// frontend/web/src/api/agent-runs.ts
//
// The dashboard's agent-run surface. Two backends:
//
//   - Mock fixtures (Phase 0 / tests / local UI work without the daemon).
//   - Real HTTP + SSE (`/api/agent-runs/:id` and `/api/agent-runs/:id/stream`),
//     served by the `agent-run-observability-export-cli` track.
//
// Selection rule:
//   - `VITE_USE_MOCK_AGENT_RUNS=1`  -> mock
//   - `VITE_USE_MOCK_AGENT_RUNS=0`  -> real HTTP (overrides MODE)
//   - else if MODE is `test`        -> mock (Vitest baseline)
//   - else                          -> real HTTP

import { ApiError, apiFetch } from "./client";
import { decisionIdxFromAttributes } from "@/features/agent-runs/decision-idx";
import {
  MOCK_RUN_COMPLETED,
  MOCK_RUN_ERROR,
  MOCK_RUN_FULL_DEBUG,
  MOCK_RUN_LIVE,
} from "@/features/agent-runs/mock-fixtures";
import { useTraceDock } from "@/stores/trace-dock";
import type {
  AgentRunDetail,
  AgentRunMemoryEventsResponse,
  AgentRunStreamEvent,
  AgentRunSummary,
  AgentRunAccounting,
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
  list: (params?: { status?: string; limit?: number; since?: string }) =>
    [...agentRunKeys.all, "list", params] as const,
  run: (id: string) => [...agentRunKeys.all, "run", id] as const,
};

/**
 * URL of the dashboard's per-run JSON export endpoint.
 * Kept as a plain string helper so callers can pass it to `fetch` directly.
 */
export function agentRunExportUrl(id: string): string {
  return `/api/agent-runs/${encodeURIComponent(id)}/export.json`;
}

/**
 * Whether the shim should serve mock fixtures instead of calling the real
 * backend. Exported for tests so they can flip the mode without
 * monkey-patching `import.meta.env`.
 */
export function shouldUseMockAgentRuns(): boolean {
  // MUST stay literal `import.meta.env.…` expressions (typed via
  // src/vite-env.d.ts) so Vite statically replaces them in production builds;
  // an alias read (`const meta = import.meta`) survives to runtime where
  // browsers have no `import.meta.env`. Vitest keeps env live and the reads
  // happen at call time, so `vi.stubEnv` still flips the flag between cases.
  const explicit = import.meta.env.VITE_USE_MOCK_AGENT_RUNS;
  if (explicit === "1" || explicit === "true") return true;
  if (explicit === "0" || explicit === "false") return false;
  // No explicit override: only the Vitest test runner gets mock by default.
  // Development (`vite dev`) hits the real HTTP backend now that the agent-run
  // observability endpoints are shipped and stable.
  return import.meta.env.MODE === "test";
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
  "interrupted",
  "agent_failure",
]);
const RETENTION_MODES: ReadonlySet<RetentionMode> = new Set([
  "hash_only",
  "redacted",
  "full_debug",
]);

type Problem = string;

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function isAgentRunExportPayload(payload: unknown): payload is Record<string, unknown> {
  return (
    isObject(payload) &&
    (payload.schema_version === "xvn.agent_run.v1" ||
      payload.schema_version === "xvn.agent_run.v2" ||
      // WS-7: the export bumped to v3 (added the full `events` array + inlined
      // blob payloads). The detail page consumes the same payload, so it must
      // accept v3 — otherwise GET /api/agent-runs/:id throws invalid_response.
      payload.schema_version === "xvn.agent_run.v3")
  );
}

function asString(v: unknown, fallback = ""): string {
  return typeof v === "string" ? v : fallback;
}

function asNumber(v: unknown, fallback = 0): number {
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

function asNullableNumber(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function asNullableString(v: unknown): string | null {
  return typeof v === "string" ? v : null;
}

function parseAttributes(raw: unknown): Record<string, unknown> {
  if (isObject(raw)) return raw;
  if (typeof raw !== "string" || raw.length === 0) return {};
  try {
    const parsed = JSON.parse(raw);
    return isObject(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function spanStatus(row: Record<string, unknown>): RunSpan["status"] {
  if (row.status === "error") return "error";
  if (row.ended_at == null && row.finished_at == null) return "in_progress";
  return "ok";
}

/**
 * Pull a human-readable error message out of the observability
 * `error_json` payload. The recorder writes
 * `JSON.stringify({ message: "<text>" })` for ToolCallFailed +
 * SpanFinished(Error); some emitters write a plain string. Accept
 * either shape, fall back to the raw string when JSON parse fails.
 * Returns `undefined` for null/missing inputs so the field is dropped
 * from the JSON wire shape.
 */
function parseErrorJson(raw: unknown): string | undefined {
  if (raw == null) return undefined;
  if (typeof raw !== "string" || raw.length === 0) return undefined;
  try {
    const parsed = JSON.parse(raw);
    if (typeof parsed === "string") return parsed;
    if (isObject(parsed)) {
      const msg = parsed.message ?? parsed.error ?? parsed.detail;
      if (typeof msg === "string" && msg.length > 0) return msg;
    }
  } catch {
    // Recorder may have written a bare string; fall through.
  }
  return raw;
}

function flattenExportSpans(spans: unknown, out: RunSpan[] = []): RunSpan[] {
  if (!Array.isArray(spans)) return out;
  for (const raw of spans) {
    if (!isObject(raw)) continue;
    const id = asString(raw.id ?? raw.span_id);
    if (!id) continue;
    const attrs = parseAttributes(raw.attributes_json ?? raw.attributes);
    const errorMessage = parseErrorJson(raw.error_json ?? raw.error_message);
    const kind = asString(raw.kind, "agent.run") as RunSpan["kind"];
    // Project the on-disk `attributes_json.broker_call` blob (written
    // by `xvision_observability::sqlite::SqliteRecorder` on the
    // SpanStarted + BrokerCallFinished arms) onto the typed
    // `RunSpan.broker_call` field so SpanInspector renders without
    // re-reading attributes. `qa-trace-broker-spans`.
    const brokerCall =
      kind === "broker.call" ? extractBrokerCall(attrs) : undefined;
    // Project the per-decision-cycle index off the broker submit's
    // idempotency_key ("<run_id>-<decision_idx>") onto `RunSpan.decision_idx`
    // so the FilterBar dropdown + `use-span-filter` + `deriveDecisions`
    // can match by cycle. PR #385 followup — see decision-idx.ts header
    // for the carrier contract.
    const decisionIdx =
      kind === "broker.call" ? decisionIdxFromAttributes(attrs) : undefined;
    out.push({
      span_id: id,
      parent_span_id: asNullableString(raw.parent_span_id),
      name: asString(raw.name, id),
      kind,
      started_at: asString(raw.started_at),
      finished_at: asNullableString(raw.ended_at ?? raw.finished_at),
      status: spanStatus(raw),
      attributes: attrs,
      ...(errorMessage ? { error_message: errorMessage } : {}),
      ...(brokerCall ? { broker_call: brokerCall } : {}),
      ...(decisionIdx != null ? { decision_idx: decisionIdx } : {}),
    });
    flattenExportSpans(raw.children, out);
  }
  return out;
}

function extractBrokerCall(
  attrs: Record<string, unknown>,
): RunSpan["broker_call"] | undefined {
  const raw = attrs["broker_call"];
  if (!isObject(raw)) return undefined;
  const side = asString(raw.side, "buy") as NonNullable<
    RunSpan["broker_call"]
  >["side"];
  const symbol = asString(raw.symbol);
  if (!symbol) return undefined;
  const outcomeRaw = asString(raw.outcome);
  const outcome = outcomeRaw
    ? (outcomeRaw as NonNullable<RunSpan["broker_call"]>["outcome"])
    : null;
  return {
    side,
    symbol,
    qty: typeof raw.qty === "number" ? raw.qty : 0,
    intended_price: typeof raw.intended_price === "number" ? raw.intended_price : null,
    order_type: asString(raw.order_type, "market"),
    venue: asString(raw.venue, "unknown"),
    idempotency_key: asNullableString(raw.idempotency_key) ?? null,
    outcome,
    fill_price: typeof raw.fill_price === "number" ? raw.fill_price : null,
    fill_qty: typeof raw.fill_qty === "number" ? raw.fill_qty : null,
    fee: typeof raw.fee === "number" ? raw.fee : null,
    broker_order_id: asNullableString(raw.broker_order_id) ?? null,
    error_class: asNullableString(raw.error_class) ?? null,
    error_message: asNullableString(raw.error_message) ?? null,
    severity:
      raw.severity === "warn" || raw.severity === "error"
        ? (raw.severity as "warn" | "error")
        : null,
  };
}

function durationMs(startedAt: string, finishedAt: string | null): number | null {
  if (!finishedAt) return null;
  const start = new Date(startedAt).getTime();
  const end = new Date(finishedAt).getTime();
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  return Math.max(0, end - start);
}

function normalizeAccounting(raw: unknown): AgentRunAccounting | null {
  if (!isObject(raw)) return null;
  return {
    source: asString(raw.source, "none") as AgentRunAccounting["source"],
    eval_run_id: asNullableString(raw.eval_run_id),
    eval_mode: asNullableString(raw.eval_mode),
    eval_status: asNullableString(raw.eval_status),
    eval_actual_input_tokens: asNullableNumber(raw.eval_actual_input_tokens),
    eval_actual_output_tokens: asNullableNumber(raw.eval_actual_output_tokens),
    eval_model_calls: asNumber(raw.eval_model_calls),
    eval_model_call_input_tokens: asNullableNumber(raw.eval_model_call_input_tokens),
    eval_model_call_output_tokens: asNullableNumber(raw.eval_model_call_output_tokens),
    eval_model_call_cost_usd: asNullableNumber(raw.eval_model_call_cost_usd),
  };
}

function normalizeAgentRunExport(payload: Record<string, unknown>): AgentRunDetail {
  const totals = isObject(payload.totals) ? payload.totals : {};
  const accounting = normalizeAccounting(payload.accounting);
  const spans = flattenExportSpans(payload.spans);
  const modelCallsRaw = Array.isArray(payload.model_calls) ? payload.model_calls : [];
  const toolCallsRaw = Array.isArray(payload.tool_calls) ? payload.tool_calls : [];
  const bySpan = new Map(spans.map((s) => [s.span_id, s]));
  // Project per-call provider/model/cost/hashes back onto the matching
  // `model.call` span so SpanInspector can render the model the slot
  // actually invoked (not just the strategy default) and so operators
  // can pivot to the on-disk payload via the prompt/response refs.
  for (const raw of modelCallsRaw) {
    if (!isObject(raw)) continue;
    const spanId = asString(raw.span_id);
    const span = bySpan.get(spanId);
    if (!span) continue;
    const provider = asString(raw.provider);
    const model = asString(raw.model);
    if (provider) span.provider = provider;
    if (model) span.model = model;
    if (typeof raw.input_token_count === "number") span.tokens_in = raw.input_token_count;
    if (typeof raw.output_token_count === "number") span.tokens_out = raw.output_token_count;
    if (typeof raw.cost_usd === "number") span.cost = raw.cost_usd;
    const promptHash = asString(raw.prompt_hash);
    if (promptHash) span.hash = promptHash;
    const responseHash = asNullableString(raw.response_hash);
    if (responseHash) span.response_hash = responseHash;
    const promptText = asNullableString(raw.prompt_text);
    if (promptText) span.prompt = promptText;
    const responseText = asNullableString(raw.response_text);
    if (responseText) span.response = responseText;
    const promptRef = asNullableString(raw.prompt_payload_ref);
    if (promptRef) span.prompt_payload_ref = promptRef;
    const responseRef = asNullableString(raw.response_payload_ref);
    if (responseRef) span.response_payload_ref = responseRef;
  }
  const status = asString(payload.status, "completed") as RunStatus;
  const startedAt = asString(payload.started_at);
  const finishedAt = asNullableString(payload.finished_at);
  const errorCount =
    spans.filter((s) => s.status === "error").length +
    (status === "failed" || status === "agent_failure" ? 1 : 0);

  return {
    summary: {
      run_id: asString(payload.run_id),
      objective: asString(payload.objective),
      strategy_id: asNullableString(payload.strategy_id),
      agent_id: null,
      started_at: startedAt,
      finished_at: finishedAt,
      status,
      span_count: spans.length,
      model_call_count: asNumber(totals.model_calls, modelCallsRaw.length),
      tool_call_count: asNumber(totals.tool_calls, toolCallsRaw.length),
      error_count: errorCount,
      total_cost_usd: asNumber(totals.cost_usd),
      total_input_tokens: asNumber(totals.input_tokens),
      total_output_tokens: asNumber(totals.output_tokens),
      duration_ms: durationMs(startedAt, finishedAt),
      financial_eval_id: asNullableString(payload.eval_run_id),
      retention_mode: asString(payload.retention_mode, "hash_only") as RetentionMode,
      ...(accounting ? { accounting } : {}),
    },
    spans,
    model_calls: modelCallsRaw.filter(isObject).map((row) => ({
      model_call_id: asString(row.span_id),
      span_id: asString(row.span_id),
      provider: asString(row.provider),
      model: asString(row.model),
      input_tokens:
        typeof row.input_token_count === "number" ? row.input_token_count : null,
      output_tokens:
        typeof row.output_token_count === "number" ? row.output_token_count : null,
      cost_usd: typeof row.cost_usd === "number" ? row.cost_usd : null,
      prompt_hash: asString(row.prompt_hash),
      response_hash: asNullableString(row.response_hash),
      prompt_text: asNullableString(row.prompt_text),
      prompt_payload_ref: asNullableString(row.prompt_payload_ref),
      response_payload_ref: asNullableString(row.response_payload_ref),
      response_text: asNullableString(row.response_text),
    })),
    tool_calls: toolCallsRaw.filter(isObject).map((row) => {
      const spanId = asString(row.span_id);
      const span = bySpan.get(spanId);
      return {
        tool_call_id: spanId,
        span_id: spanId,
        tool_name: asString(row.tool_name),
        input_json: row.input_payload_ref ?? row.input_hash ?? null,
        output_json: row.output_payload_ref ?? row.output_hash ?? null,
        error: null,
        started_at: span?.started_at ?? "",
        finished_at: span?.finished_at ?? null,
      };
    }),
  };
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
    problems.push("summary.retention_mode: missing (expected hash_only|redacted|full_debug)");
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
  if (isAgentRunExportPayload(payload)) {
    return validateAgentRunDetail(normalizeAgentRunExport(payload));
  }

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

/**
 * Alias so callers can use the more natural name.
 * @see shouldUseMockAgentRuns
 */
export const useMockAgentRuns = shouldUseMockAgentRuns;

/**
 * List agent runs, optionally filtered by status.
 *
 * In mock mode returns the MOCK_RUN_LIVE summary. In production calls
 * `GET /api/agent-runs` and returns the run list.
 */
export async function listAgentRuns(params?: {
  status?: string;
  limit?: number;
  /// Inclusive lower bound on `started_at`, RFC-3339; absent/empty => no
  /// filter. Mirrors the eval-runs `since` contract (bead-008).
  since?: string;
}): Promise<AgentRunSummary[]> {
  if (shouldUseMockAgentRuns()) {
    return Promise.resolve([MOCK_RUN_LIVE.summary]);
  }
  const qs = new URLSearchParams();
  if (params?.status) qs.set("status", params.status);
  if (params?.limit) qs.set("limit", String(params.limit));
  if (params?.since) qs.set("since", params.since);
  const url = `/api/agent-runs${qs.toString() ? `?${qs}` : ""}`;
  const resp = await apiFetch<{ runs: AgentRunSummary[]; total: number }>(url);
  return resp.runs;
}

/**
 * Fetch the body bytes for a payload ref owned by `runId`. Server
 * returns `application/octet-stream`; we surface the body as text so
 * the SpanInspector can render it inline. Binary refs (rare — model
 * payloads are JSON/UTF-8 in practice) will round-trip through the
 * browser's UTF-8 decoder.
 *
 * Errors map to `ApiError` codes:
 *   - 400 → `validation`         (malformed ref shape)
 *   - 403 → `forbidden`          (run is hash_only retention)
 *   - 404 → `not_found`          (ref not owned by run or missing on disk)
 *   - 5xx → `internal`
 *
 * No mock branch — the route doesn't ship in fixtures yet. Caller is
 * expected to gate by `shouldUseMockAgentRuns()` if relevant.
 */
export async function fetchAgentRunBlob(
  runId: string,
  ref: string,
): Promise<string> {
  const url = `/api/agent-runs/${encodeURIComponent(runId)}/blobs/${encodeURIComponent(ref)}`;
  let res: Response;
  try {
    res = await fetch(url, { headers: { accept: "application/octet-stream" } });
  } catch (err) {
    throw new ApiError(0, "network", err instanceof Error ? err.message : String(err));
  }
  if (!res.ok) {
    let body: { code?: string; message?: string } | undefined;
    try {
      body = await res.json();
    } catch {
      // 5xx may not be JSON; fall back to status text.
    }
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
    );
  }
  return await res.text();
}

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

export async function getAgentRunMemoryEvents(
  id: string,
): Promise<AgentRunMemoryEventsResponse> {
  return apiFetch<AgentRunMemoryEventsResponse>(
    `/api/agent-runs/${encodeURIComponent(id)}/memory-events`,
  );
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
        total_cost_usd: tickCost,
      },
    });
  }, 800);
  return () => window.clearInterval(interval);
}

/**
 * Real-branch SSE consumer. Maps the wire vocabulary produced by
 * `crates/xvision-dashboard/src/sse/mod.rs` (one `event:` name per
 * `RunEvent` variant, plus a leading `snapshot` and a synthetic
 * `lagged`) into typed `AgentRunStreamEvent`s.
 *
 * Side effect: every successfully-parsed event is also dispatched into
 * the trace-dock store so the strip + inspector can render streaming
 * indicators without each consumer wiring its own bridge.
 *
 * Reconnect is exponential-backoff per `SSE_BACKOFF_MS`. The snapshot
 * is the first frame on every (re)connect, so a dropped connection
 * recovers the full run state on its own.
 */
export const REAL_SSE_EVENTS = [
  "snapshot",
  "run_started",
  "run_finished",
  "run_interrupted",
  "span_started",
  "span_finished",
  "model_call_finished",
  "tool_call_started",
  "tool_call_finished",
  "tool_call_failed",
  "tool_call_cancelled",
  "broker_call_started",
  "broker_call_finished",
  "assistant_text_delta",
  "sidecar_error",
  "checkpoint_written",
  "supervisor_note",
  "artifact_written",
  "backpressure_dropped",
  "memory_recall",
  "memory_write",
  "engine_event",
  "lagged",
] as const;
type RealSseEventName = (typeof REAL_SSE_EVENTS)[number];

function parseSnapshot(raw: string): AgentRunDetail | null {
  try {
    return validateAgentRunDetail(JSON.parse(raw));
  } catch {
    return null;
  }
}

function dispatchToStore(ev: AgentRunStreamEvent): void {
  try {
    useTraceDock.getState().applyStreamEvent(ev);
  } catch (err) {
    // Store side-effect must never break the consumer callback.
    if (typeof console !== "undefined") {
      console.warn("[agent-runs] trace-dock dispatch failed", err);
    }
  }
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

  const handle = (eventName: RealSseEventName) => (ev: MessageEvent) => {
    if (eventName === "snapshot") {
      const detail = parseSnapshot(ev.data as string);
      if (!detail) return;
      const out: AgentRunStreamEvent = { event: "snapshot", data: detail };
      dispatchToStore(out);
      onEvent(out);
      return;
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(ev.data as string);
    } catch {
      // Drop malformed frames — the validator will surface shape errors
      // on the next snapshot refetch.
      return;
    }
    // The Rust side encodes the variant tag as `kind` inside the JSON
    // payload (see `#[serde(tag = "kind", rename_all = "snake_case")]`).
    // We trust the `event:` name and use the inner payload as-is. The
    // remaining typing is intentionally loose — see types-agent-runs.ts.
    const data = parsed as Record<string, unknown>;
    const out = { event: eventName, data: data as never } as AgentRunStreamEvent;
    dispatchToStore(out);
    onEvent(out);
  };

  const connect = () => {
    if (closed) return;
    source = new EventSource(url);
    source.addEventListener("open", () => {
      attempt = 0;
    });
    for (const name of REAL_SSE_EVENTS) {
      source.addEventListener(name, handle(name) as EventListener);
    }
    // Back-compat: keep the mock arms alive in case the backend ever
    // emits them (e.g. integration shim). Cheap and additive.
    source.addEventListener("span", ((ev: MessageEvent) => {
      try {
        const data = JSON.parse(ev.data) as RunSpan;
        onEvent({ event: "span", data });
      } catch {
        /* swallow */
      }
    }) as EventListener);
    source.addEventListener("summary", ((ev: MessageEvent) => {
      try {
        const data = JSON.parse(ev.data) as AgentRunSummary;
        onEvent({ event: "summary", data });
      } catch {
        /* swallow */
      }
    }) as EventListener);
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
