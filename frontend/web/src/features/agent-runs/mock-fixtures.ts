// frontend/web/src/features/agent-runs/mock-fixtures.ts
import type {
  AgentRunDetail,
  ModelCall,
  RunSpan,
  SpanStatus,
  ToolCall,
} from "@/api/types-agent-runs";

function mkSpan(partial: Partial<RunSpan> & Pick<RunSpan, "span_id" | "name" | "kind">): RunSpan {
  return {
    parent_span_id: null,
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: "2026-05-17T10:00:03.400Z",
    status: "ok",
    attributes: {},
    ...partial,
  };
}

const COMPLETED_SPANS: RunSpan[] = [
  mkSpan({ span_id: "s1", name: "agent.run", kind: "agent.run",
    finished_at: "2026-05-17T10:00:03.400Z" }),
  mkSpan({ span_id: "s2", parent_span_id: "s1", name: "plan", kind: "agent.plan",
    started_at: "2026-05-17T10:00:00.100Z",
    finished_at: "2026-05-17T10:00:00.500Z" }),
  mkSpan({ span_id: "s3", parent_span_id: "s1", name: "claude-opus-4-7", kind: "model.call",
    started_at: "2026-05-17T10:00:00.500Z",
    finished_at: "2026-05-17T10:00:01.600Z",
    attributes: { cost_usd: 0.04, input_tokens: 8412, output_tokens: 1204 } }),
  mkSpan({ span_id: "s4", parent_span_id: "s3", name: "run_backtest", kind: "tool.call",
    started_at: "2026-05-17T10:00:01.600Z",
    finished_at: "2026-05-17T10:00:03.000Z" }),
  mkSpan({ span_id: "s5", parent_span_id: "s1", name: "claude-opus-4-7", kind: "model.call",
    started_at: "2026-05-17T10:00:03.000Z",
    finished_at: "2026-05-17T10:00:03.300Z",
    attributes: { cost_usd: 0.02, input_tokens: 4210, output_tokens: 612 } }),
  mkSpan({ span_id: "s6", parent_span_id: "s1", name: "review", kind: "supervisor.review",
    started_at: "2026-05-17T10:00:03.300Z",
    finished_at: "2026-05-17T10:00:03.400Z" }),
];

const COMPLETED_MODEL_CALLS: ModelCall[] = [
  { model_call_id: "m1", span_id: "s3", provider: "anthropic",
    model: "claude-opus-4-7", input_tokens: 8412, output_tokens: 1204,
    cost_usd: 0.0416, prompt_hash: "sha256:a1b2c3", prompt_text: null, response_text: null },
  { model_call_id: "m2", span_id: "s5", provider: "anthropic",
    model: "claude-opus-4-7", input_tokens: 4210, output_tokens: 612,
    cost_usd: 0.0208, prompt_hash: "sha256:d4e5f6", prompt_text: null, response_text: null },
];

const COMPLETED_TOOL_CALLS: ToolCall[] = [
  { tool_call_id: "t1", span_id: "s4", tool_name: "run_backtest",
    input_json: { strategy: "btc_mean_reversion_v4", days: 30 },
    output_json: { pnl: 0.034, max_drawdown: 0.018 },
    error: null,
    started_at: "2026-05-17T10:00:01.600Z",
    finished_at: "2026-05-17T10:00:03.000Z" },
];

export const MOCK_RUN_COMPLETED: AgentRunDetail = {
  summary: {
    run_id: "run_abc1234",
    objective: "Improve BTC mean reversion strategy",
    strategy_id: "btc_mean_reversion_v4",
    agent_id: "agent_default_trader",
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: "2026-05-17T10:00:03.400Z",
    status: "completed",
    span_count: COMPLETED_SPANS.length,
    model_call_count: COMPLETED_MODEL_CALLS.length,
    tool_call_count: COMPLETED_TOOL_CALLS.length,
    error_count: 0,
    total_cost_usd: 0.0624,
    total_input_tokens: 12622,
    total_output_tokens: 1816,
    duration_ms: 3400,
    financial_eval_id: "eval_456",
    retention_mode: "hash_only",
  },
  spans: COMPLETED_SPANS,
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS,
};

export const MOCK_RUN_LIVE: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_live5678",
    status: "running",
    finished_at: null,
    duration_ms: null,
    span_count: 3,
    model_call_count: 0,
    tool_call_count: 0,
    error_count: 0,
    total_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
  },
  spans: [
    { ...COMPLETED_SPANS[0]!, finished_at: null, status: "in_progress" as SpanStatus },
    COMPLETED_SPANS[1]!,
    { ...COMPLETED_SPANS[2]!, finished_at: null, status: "in_progress" as SpanStatus },
  ],
  model_calls: [],
  tool_calls: [],
};

export const MOCK_RUN_ERROR: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_err9999",
    status: "failed",
    error_count: 1,
  },
  spans: COMPLETED_SPANS.map((s, i) =>
    i === 3 ? { ...s, status: "error", attributes: { ...s.attributes, error: "tool timeout" } } : s,
  ),
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS.map((t) => ({ ...t, error: "tool timeout" })),
};

/**
 * Same shape as the completed run, but recorded under `full_debug` so the
 * detail route renders the retention warning banner.
 */
export const MOCK_RUN_FULL_DEBUG: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_debug42",
    retention_mode: "full_debug",
  },
  spans: COMPLETED_SPANS,
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS,
};

/**
 * A completed run in `replay` mode with a hit ratio, dropped events, and
 * a recovery reason. Used to verify the TrajectoryModeBadge and
 * SpanInspector trajectory section.
 */
export const MOCK_RUN_REPLAY: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_replay99",
    trajectory_mode: "replay",
    replay_hit_ratio: 0.875,
    dropped_events: 3,
    recovery_reason: "replay_divergence",
  },
  spans: COMPLETED_SPANS,
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS,
};

/**
 * A completed run in `record` mode (frames are being written out).
 * No replay metrics since recording is the live path.
 */
export const MOCK_RUN_RECORD: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_record77",
    trajectory_mode: "record",
  },
  spans: COMPLETED_SPANS,
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS,
};
