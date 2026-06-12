import { apiFetch } from "./client";

export type ToolDiagnostic = {
  name: string;
  registered: boolean;
  description: string | null;
};

export type StrategyAgentDiagnostics = {
  role: string;
  agent_id: string;
  agent_name: string | null;
  agent_resolved: boolean;
  tools: ToolDiagnostic[];
};

export type UnmetTool = {
  role: string;
  agent_id: string;
  tool: string;
};

export type StrategyDiagnostics = {
  strategy_id: string;
  per_agent: StrategyAgentDiagnostics[];
  unregistered_tools: UnmetTool[];
  has_decision_path: boolean;
  launchable: boolean;
  warnings?: string[];
};

export type AgentSlotDiagnostics = {
  slot_name: string;
  model_bound: boolean;
  prompt_present: boolean;
  tools: ToolDiagnostic[];
};

export type AgentDiagnostics = {
  agent_id: string;
  agent_name: string;
  slots: AgentSlotDiagnostics[];
  tool_names: string[];
  agent_ready: boolean;
};

export async function getStrategyDiagnostics(
  strategyId: string,
): Promise<StrategyDiagnostics> {
  return apiFetch<StrategyDiagnostics>(
    `/api/strategy/${encodeURIComponent(strategyId)}/diagnostics`,
  );
}

export async function getAgentDiagnostics(
  agentId: string,
): Promise<AgentDiagnostics> {
  return apiFetch<AgentDiagnostics>(
    `/api/agents/${encodeURIComponent(agentId)}/diagnostics`,
  );
}

export const diagnosticsKeys = {
  all: ["diagnostics"] as const,
  strategy: (id: string) =>
    [...diagnosticsKeys.all, "strategy", id] as const,
  agent: (id: string) => [...diagnosticsKeys.all, "agent", id] as const,
};
