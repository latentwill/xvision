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

export type AgentRunStreamEvent =
  | { event: "span"; data: RunSpan }
  | { event: "summary"; data: AgentRunSummary };
