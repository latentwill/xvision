/**
 * TrajectoryFrame types — Node/TypeScript mirror of the Rust `TrajectoryFrame`
 * enum in `crates/xvision-observability/src/trajectory/frame.rs`.
 *
 * Tag names and field names must stay in sync with the Rust mirror so that
 * JSON persisted from Node can be deserialized by Rust's serde and vice versa.
 * Rust uses `serde(tag = "kind")` with snake_case field names.
 *
 * Bump `TRAJECTORY_FRAME_SCHEMA_VERSION` and update both sides whenever any
 * variant is added, removed, or structurally changed.
 */

export const TRAJECTORY_FRAME_SCHEMA_VERSION = 1 as const

/** Monotonic millisecond timestamp attached to every frame. */
export type TsMs = number

/**
 * Request frame: the full model request (messages + tools + system prompt)
 * sent to the provider at the start of each stream() call. This is the only
 * frame that carries full payloads needed to reconstruct the input side of
 * a trajectory step.
 */
export interface RequestFrame {
  kind: "Request"
  ts_ms: TsMs
  messages: unknown
  tools: unknown
  system_prompt?: string
}

/**
 * TextDelta: one chunk of streamed assistant text.
 */
export interface TextDeltaFrame {
  kind: "TextDelta"
  ts_ms: TsMs
  text: string
}

/**
 * ReasoningDelta: one chunk of streamed reasoning/thinking text.
 */
export interface ReasoningDeltaFrame {
  kind: "ReasoningDelta"
  ts_ms: TsMs
  text: string
}

/**
 * ToolCallDelta: one chunk of a streamed tool-call (may be partial input JSON).
 */
export interface ToolCallDeltaFrame {
  kind: "ToolCallDelta"
  ts_ms: TsMs
  tool_call_id?: string
  tool_name?: string
  input?: unknown
}

/**
 * ToolResult: the full output (or error-as-data) returned by a tool execution.
 * Recorded by tool-shim.ts before the result is returned to Cline.
 * Mandatory for replay divergence detection and for reconstructing the next
 * model request (which embeds tool results as messages).
 */
export interface ToolResultFrame {
  kind: "ToolResult"
  ts_ms: TsMs
  tool_call_id: string
  output: unknown
  error?: string
}

/**
 * Usage: token counts and cost snapshot emitted by the model provider.
 */
export interface UsageFrame {
  kind: "Usage"
  ts_ms: TsMs
  input_tokens: number
  output_tokens: number
  cache_read_tokens: number
  cache_write_tokens: number
  total_cost: number
}

/**
 * RetryOrCancel: a retry or cancel decision made by the agent runtime.
 * Captures the reason so replay can reproduce the same branching.
 */
export interface RetryOrCancelFrame {
  kind: "RetryOrCancel"
  ts_ms: TsMs
  reason: string
}

/**
 * Finish: the terminal frame for a stream() call.
 * `reason` mirrors `AgentModelFinishReason` (stop | tool-calls | max-tokens | aborted | error).
 */
export interface FinishFrame {
  kind: "Finish"
  ts_ms: TsMs
  reason: string
  error?: string
}

/**
 * Discriminated union of all trajectory frame variants.
 */
export type TrajectoryFrame =
  | RequestFrame
  | TextDeltaFrame
  | ReasoningDeltaFrame
  | ToolCallDeltaFrame
  | ToolResultFrame
  | UsageFrame
  | RetryOrCancelFrame
  | FinishFrame
