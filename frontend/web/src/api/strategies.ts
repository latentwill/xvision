// Strategies API — wraps `engine::api::strategy::*` (PR #4 list/get,
// PR #47 mutations). Strategy / slot / risk / validation types are
// hand-rolled because the strategy module doesn't have ts-rs derives yet;
// if those land later, replace these with `import type { ... } from
// "./types.gen"`.

import { apiFetch } from "./client";

export type StrategyListItem = {
  agent_id: string;
  display_name: string;
  template: string;
  decision_cadence_minutes: number;
  tags?: string[];
  model?: string;
  providers?: string[];
  models?: string[];
};

export type PipelineKind = "single" | "sequential" | "graph";
export type AgentRef = {
  agent_id: string;
  role: string;
};
export type PipelineEdge = {
  from_role: string;
  to_role: string;
};
export type PipelineDef = {
  kind: PipelineKind;
  edges?: PipelineEdge[];
};

export type StrategiesListResponse = {
  items: StrategyListItem[];
};

export type LLMSlot = {
  role: string;
  prompt: string;
  model_requirement: string;
  provider?: string | null;
  model?: string | null;
  allowed_tools: string[];
};

export type RiskConfig = {
  risk_pct_per_trade: number;
  max_concurrent_positions: number;
  max_leverage: number;
  stop_loss_atr_multiple: number;
  daily_loss_kill_pct: number;
};

export type PublicManifest = {
  id: string;
  display_name: string;
  plain_summary: string;
  creator: string;
  template: string;
  regime_fit: string[];
  asset_universe: string[];
  decision_cadence_minutes: number;
  required_models: string[];
  required_tools: string[];
  risk_preset_or_config: string;
  published_at: string | null;
};

export type Strategy = {
  manifest: PublicManifest;
  regime_slot: LLMSlot | null;
  intern_slot: LLMSlot | null;
  trader_slot: LLMSlot | null;
  risk: RiskConfig;
  mechanical_params: unknown;
  agents?: AgentRef[];
  pipeline?: PipelineDef;
};

export type UpdateSlotBody = Partial<{
  prompt: string;
  model_requirement: string;
  provider: string;
  model: string;
  allowed_tools: string[];
}>;

export type UpdateSlotOut = {
  id: string;
  updated: string[];
};

export type StrategyAgentsOut = {
  strategy_id: string;
  agents: AgentRef[];
  pipeline: PipelineDef;
};

export type SetPipelineBody = {
  kind: PipelineKind;
  edges?: PipelineEdge[];
};

export type PutRiskBody =
  | { preset: string; explicit?: undefined }
  | { explicit: RiskConfig; preset?: undefined };

export type SetRiskConfigOut = {
  id: string;
  applied: "preset" | "explicit";
};

export type ValidateDraftOut = {
  id: string;
  ok: boolean;
  errors: string[];
};

export type TemplateInfo = {
  name: string;
  display_name: string;
  plain_summary: string;
};

export type TemplatesListResponse = {
  items: TemplateInfo[];
};

export type CreateStrategyReq = {
  template: string;
  name: string;
  creator?: string | null;
};

export type CreateStrategyOut = {
  id: string;
};

export const strategyKeys = {
  all: ["strategies"] as const,
  list: () => [...strategyKeys.all, "list"] as const,
  detail: (id: string) => [...strategyKeys.all, "detail", id] as const,
  validate: (id: string) => [...strategyKeys.all, "validate", id] as const,
  templates: () => [...strategyKeys.all, "templates"] as const,
};

export function listStrategies(): Promise<StrategyListItem[]> {
  return apiFetch<StrategiesListResponse>("/api/strategies").then(
    (r) => r.items,
  );
}

export function getStrategy(id: string): Promise<Strategy> {
  return apiFetch<Strategy>(`/api/strategy/${encodeURIComponent(id)}`);
}

export function updateSlot(
  id: string,
  role: string,
  body: UpdateSlotBody,
): Promise<UpdateSlotOut> {
  return apiFetch<UpdateSlotOut>(
    `/api/strategy/${encodeURIComponent(id)}/slot/${encodeURIComponent(role)}`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
}

export function setRiskConfig(
  id: string,
  body: PutRiskBody,
): Promise<SetRiskConfigOut> {
  return apiFetch<SetRiskConfigOut>(
    `/api/strategy/${encodeURIComponent(id)}/risk`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
}

export function validateDraft(id: string): Promise<ValidateDraftOut> {
  return apiFetch<ValidateDraftOut>(
    `/api/strategy/${encodeURIComponent(id)}/validate`,
    { method: "POST" },
  );
}

export function addStrategyAgent(
  strategyId: string,
  body: { agent_id: string; role: string },
): Promise<StrategyAgentsOut> {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategy/${encodeURIComponent(strategyId)}/agents`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export function removeStrategyAgent(
  strategyId: string,
  role: string,
): Promise<StrategyAgentsOut> {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategy/${encodeURIComponent(strategyId)}/agents/${encodeURIComponent(role)}`,
    { method: "DELETE" },
  );
}

export function renameStrategyAgentRole(
  strategyId: string,
  role: string,
  newRole: string,
): Promise<StrategyAgentsOut> {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategy/${encodeURIComponent(strategyId)}/agents/${encodeURIComponent(role)}`,
    {
      method: "PATCH",
      body: JSON.stringify({ new_role: newRole }),
    },
  );
}

export function setStrategyPipeline(
  strategyId: string,
  body: SetPipelineBody,
): Promise<StrategyAgentsOut> {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategy/${encodeURIComponent(strategyId)}/pipeline`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
}

/// List the built-in strategy templates. Static registry on the
/// backend; safe to fetch once and cache.
export function listTemplates(): Promise<TemplateInfo[]> {
  return apiFetch<TemplatesListResponse>("/api/templates").then(
    (r) => r.items,
  );
}

/// Create a new draft strategy from a template. Returns the new
/// agent_id; the picker UI redirects to /authoring/:id after this
/// resolves.
export function createStrategy(
  body: CreateStrategyReq,
): Promise<CreateStrategyOut> {
  return apiFetch<CreateStrategyOut>("/api/strategies", {
    method: "POST",
    body: JSON.stringify(body),
  });
}
