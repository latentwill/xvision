// frontend/web/src/api/types-agent-runs.ts
//
// Types mirror the Rust data model in
// docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md.
// When the backend lands ts-rs derives, replace this file with the
// generated bindings.

export type RunStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted"
  | "agent_failure";

/**
 * Retention modes mirror the recorder's on-disk policy (see
 * docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md §retention).
 *
 * - `hash_only` — default. Prompts and tool payloads are hashed; no raw
 *   text on disk. Inspector shows hashes + redaction notes.
 * - `summaries` — short summarized snippets retained alongside hashes.
 * - `full_debug` — raw prompts, responses, and tool I/O retained on disk.
 *   Surfaces a banner because PII/credential leakage risk increases.
 */
export type RetentionMode = "hash_only" | "summaries" | "full_debug";

export type SpanKind =
  | "agent.run"
  | "agent.plan"
  | "model.call"
  | "tool.call"
  | "approval.request"
  | "approval.response"
  | "sandbox.exec"
  | "supervisor.review"
  | "financial.eval"
  | "artifact.write";

export type SpanStatus = "ok" | "error" | "in_progress";

export type RunSpan = {
  span_id: string;
  parent_span_id: string | null;
  name: string;
  kind: SpanKind;
  started_at: string; // ISO
  finished_at: string | null; // ISO, null = in-flight
  status: SpanStatus;
  attributes: Record<string, unknown>;
  // Prototype-driven extensions: live in `attributes` server-side but
  // surface as first-class so the inspector can render them as pull-quotes.
  prompt?: string;
  response?: string;
  response_partial?: string;
  args?: unknown;
  result?: unknown;
  decision_idx?: number;
  provider?: string;
  model?: string;
  /** `prompt_hash` from the matching `model_calls` row (kept as `hash`
   * for back-compat with existing fixtures + SpanInspector field row). */
  hash?: string;
  response_hash?: string;
  /** Blob-store refs for the prompt + completion bodies. Only populated
   * when retention is `summaries` or `full_debug`. The ref itself is
   * surfaced in SpanInspector so operators can pivot to the on-disk
   * payload via CLI until a blob-fetch route lands. */
  prompt_payload_ref?: string;
  response_payload_ref?: string;
  tokens_in?: number;
  tokens_out?: number;
  cost?: number;
  streaming?: boolean;
};

export type ModelCall = {
  model_call_id: string;
  span_id: string;
  provider: string;
  model: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_usd: number | null;
  prompt_hash: string;
  response_hash?: string | null;
  prompt_payload_ref?: string | null;
  response_payload_ref?: string | null;
  response_text: string | null;
};

export type ToolCall = {
  tool_call_id: string;
  span_id: string;
  tool_name: string;
  input_json: unknown;
  output_json: unknown | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};

export type AgentRunSummary = {
  run_id: string;
  objective: string;
  strategy_id: string | null;
  agent_id: string | null;
  started_at: string;
  finished_at: string | null;
  status: RunStatus;
  // Pre-rolled aggregates (avoid client-side scans for the strip).
  span_count: number;
  model_call_count: number;
  tool_call_count: number;
  error_count: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  duration_ms: number | null;
  financial_eval_id: string | null;
  /**
   * Retention mode the run was recorded under. Drives the header badge
   * and (when `full_debug`) the warning banner about on-disk payloads.
   */
  retention_mode: RetentionMode;
};

export type AgentRunDetail = {
  summary: AgentRunSummary;
  spans: RunSpan[];
  model_calls: ModelCall[];
  tool_calls: ToolCall[];
};

// ---------------------------------------------------------------------------
// SSE stream events (real wire protocol shipped in #235)
// ---------------------------------------------------------------------------
//
// Wire format produced by `crates/xvision-dashboard/src/sse/mod.rs`:
//
//   event: snapshot              data: <AgentRunExport JSON>      // first frame
//   event: <variant_snake_case>  data: <RunEvent JSON>            // tail
//   event: lagged                data: {"dropped": n}             // synthetic
//
// Variants below correspond 1:1 to the Rust `RunEvent` enum in
// `crates/xvision-observability/src/events.rs`. We intentionally keep the
// payload types loose (`unknown` / minimal interfaces) for variants the
// frontend does not yet consume — the recorder owns the canonical shape
// and reshaping them here would just create drift.
//
// The mock branch keeps emitting `summary` / `span` arms; both are
// retained additively so the dock keeps working in test/dev MODE.

export type StreamSpanStartedData = {
  span_id: string;
  run_id: string;
  parent_span_id: string | null;
  kind: SpanKind;
  name: string;
  started_at: string;
  otel_trace_id?: string | null;
  otel_span_id?: string | null;
  attributes_json?: string | null;
};

export type StreamSpanFinishedData = {
  span_id: string;
  ended_at: string;
  status: SpanStatus;
  error_json?: string | null;
};

export type StreamModelCallFinishedData = {
  span_id: string;
  provider: string;
  model: string;
  input_token_count?: number | null;
  output_token_count?: number | null;
  cost_usd?: number | null;
  prompt_hash: string;
  response_hash?: string | null;
};

export type StreamToolCallStartedData = {
  span_id: string;
  tool_name: string;
  input_hash: string;
  requires_approval?: boolean;
  is_run_terminator?: boolean;
};

export type StreamToolCallFinishedData = {
  span_id: string;
  output_hash?: string | null;
  exit_code?: number | null;
};

export type StreamToolCallFailedData = {
  span_id: string;
  error_json?: string | null;
};

export type StreamToolCallCancelledData = {
  span_id: string;
  reason?: string | null;
};

export type StreamAssistantTextDeltaData = {
  span_id: string;
  run_id: string;
  delta_len: number;
};

export type StreamSidecarErrorData = {
  run_id: string;
  message: string;
  severity: string;
};

export type StreamLaggedData = { dropped: number };

/**
 * Stream events surfaced to the dock + components. Mock arms (`summary`,
 * `span`) are kept additive so the test/dev mock branch keeps working;
 * real branch arms map 1:1 to the SSE wire vocabulary.
 */
export type AgentRunStreamEvent =
  // Mock-branch arms (Phase 0 fixtures + Vitest).
  | { event: "span"; data: RunSpan }
  | { event: "summary"; data: AgentRunSummary }
  // Real-branch arms (SSE wire protocol).
  | { event: "snapshot"; data: AgentRunDetail }
  | { event: "run_started"; data: { run_id: string; objective: string; started_at: string } }
  | { event: "run_finished"; data: { run_id: string; finished_at: string; status: RunStatus } }
  | { event: "run_interrupted"; data: { run_id: string; finished_at: string; reason: string } }
  | { event: "span_started"; data: StreamSpanStartedData }
  | { event: "span_finished"; data: StreamSpanFinishedData }
  | { event: "model_call_finished"; data: StreamModelCallFinishedData }
  | { event: "tool_call_started"; data: StreamToolCallStartedData }
  | { event: "tool_call_finished"; data: StreamToolCallFinishedData }
  | { event: "tool_call_failed"; data: StreamToolCallFailedData }
  | { event: "tool_call_cancelled"; data: StreamToolCallCancelledData }
  | { event: "assistant_text_delta"; data: StreamAssistantTextDeltaData }
  | { event: "sidecar_error"; data: StreamSidecarErrorData }
  | { event: "lagged"; data: StreamLaggedData };
