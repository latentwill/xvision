// frontend/web/src/api/agent-runs.ts
//
// Phase 0: backed entirely by mocks. When backend lands, replace
// `MOCK_BY_ID` with apiFetch<AgentRunDetail>(`/api/agent-runs/${id}`).
// The mock fixtures double as the contract negotiation surface.

import { ApiError } from "./client";
import {
  MOCK_RUN_COMPLETED,
  MOCK_RUN_ERROR,
  MOCK_RUN_LIVE,
} from "@/features/agent-runs/mock-fixtures";
import type {
  AgentRunDetail,
  AgentRunStreamEvent,
} from "./types-agent-runs";

const MOCK_BY_ID: Record<string, AgentRunDetail> = {
  [MOCK_RUN_COMPLETED.summary.run_id]: MOCK_RUN_COMPLETED,
  [MOCK_RUN_LIVE.summary.run_id]: MOCK_RUN_LIVE,
  [MOCK_RUN_ERROR.summary.run_id]: MOCK_RUN_ERROR,
};

export const agentRunKeys = {
  all: ["agent-runs"] as const,
  run: (id: string) => [...agentRunKeys.all, "run", id] as const,
};

export async function getAgentRun(id: string): Promise<AgentRunDetail> {
  const detail = MOCK_BY_ID[id];
  if (!detail) {
    throw new ApiError(404, "not_found", `agent run ${id} not found`);
  }
  // Simulate async, fixed delay — easy to remove when real API lands.
  await new Promise((r) => setTimeout(r, 30));
  return detail;
}

/**
 * Open a mock stream for a live run. Emits the in-progress span as a
 * "span" event every 800ms with a synthesized cost increment so the strip
 * + dock can demo their live behavior. Returns a close() function.
 */
export function openAgentRunStream(
  runId: string,
  onEvent: (ev: AgentRunStreamEvent) => void,
): () => void {
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
