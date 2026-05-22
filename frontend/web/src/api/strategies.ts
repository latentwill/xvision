// Strategies API — wraps `engine::api::strategy::*` (PR #4 list/get,
// PR #47 mutations). Strategy / slot / risk / validation types are
// hand-rolled because the strategy module doesn't have ts-rs derives yet;
// if those land later, replace these with `import type { ... } from
// "./types.gen"`.

import { apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  errorSummary,
} from "@/lib/logger";

export type StrategyListItem = {
  agent_id: string;
  display_name: string;
  template: string;
  decision_cadence_minutes: number;
  tags?: string[];
  model?: string;
  providers?: string[];
  models?: string[];
  provider_models?: ProviderModelPair[];
};

export type ProviderModelPair = {
  provider: string;
  model: string;
};

export type PipelineKind = "single" | "sequential" | "graph";
type AgentRef = {
  agent_id: string;
  role: string;
};
type PipelineEdge = {
  from_role: string;
  to_role: string;
};
type PipelineDef = {
  kind: PipelineKind;
  edges?: PipelineEdge[];
};

type StrategiesListResponse = {
  items: StrategyListItem[];
  total: number;
};

/// Paged response envelope returned by `listStrategiesPaged`.
export type StrategiesPage = {
  items: StrategyListItem[];
  total: number;
};

export type ListStrategiesParams = {
  /// Page size. Server defaults to 50, caps at 200.
  limit?: number;
  /// Row offset. Server treats `undefined` as 0.
  offset?: number;
};

type LLMSlot = {
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

type PublicManifest = {
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

type TemplatesListResponse = {
  items: TemplateInfo[];
};

export type CreateStrategyReq = {
  name: string;
  creator?: string | null;
};

export type CreateStrategyOut = {
  id: string;
};

export type CloneStrategyReq = {
  display_name?: string;
};

export const strategyKeys = {
  all: ["strategies"] as const,
  /// Cache key includes `limit`/`offset` so page changes refetch.
  list: (params?: ListStrategiesParams) =>
    [
      ...strategyKeys.all,
      "list",
      params?.limit ?? null,
      params?.offset ?? null,
    ] as const,
  detail: (id: string) => [...strategyKeys.all, "detail", id] as const,
  validate: (id: string) => [...strategyKeys.all, "validate", id] as const,
  templates: () => [...strategyKeys.all, "templates"] as const,
};

function buildStrategiesListUrl(params?: ListStrategiesParams): string {
  const qs = new URLSearchParams();
  if (params?.limit !== undefined) qs.set("limit", String(params.limit));
  if (params?.offset !== undefined) qs.set("offset", String(params.offset));
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return `/api/strategies${suffix}`;
}

export function listStrategies(): Promise<StrategyListItem[]> {
  // Unpaged convenience wrapper — callers that want a "names + ids"
  // lookup table (eval-runs page, run-detail page) don't need
  // pagination. Internally this still hits the paginated endpoint;
  // the SPA caps at `DEFAULT_LIMIT` strategies which is plenty for the
  // lookup tables that consume this. Callers that need true paging
  // (the /strategies list page) use `listStrategiesPaged`.
  return apiFetch<StrategiesListResponse>(buildStrategiesListUrl()).then(
    (r) => r.items,
  );
}

/// Paged variant — preserves the `total` field so the dashboard's
/// pager can render "page X of N".
export function listStrategiesPaged(
  params?: ListStrategiesParams,
): Promise<StrategiesPage> {
  return apiFetch<StrategiesListResponse>(buildStrategiesListUrl(params)).then(
    (r) => ({ items: r.items, total: r.total }),
  );
}

export function getStrategy(id: string): Promise<Strategy> {
  return apiFetch<Strategy>(`/api/strategy/${encodeURIComponent(id)}`);
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
  const trace = createTrace("strategy", { strategy_id: id });
  const started = performance.now();
  trace.info("strategy.validate.start");
  return apiFetch<ValidateDraftOut>(
    `/api/strategy/${encodeURIComponent(id)}/validate`,
    { method: "POST" },
  )
    .then((result) => {
      trace.info("strategy.validate.ok", {
        ok: result.ok,
        diagnostic_count: result.errors.length,
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("strategy.validate.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function addStrategyAgent(
  strategyId: string,
  body: { agent_id: string; role: string },
): Promise<StrategyAgentsOut> {
  const trace = createTrace("strategy", {
    strategy_id: strategyId,
    agent_id: body.agent_id,
    role: body.role,
  });
  const started = performance.now();
  trace.info("strategy.agent.attach.start");
  return apiFetch<StrategyAgentsOut>(
    `/api/strategy/${encodeURIComponent(strategyId)}/agents`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  )
    .then((result) => {
      trace.info("strategy.agent.attach.ok", {
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("strategy.agent.attach.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
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

/// Create a new blank draft strategy. Returns the new agent_id; the UI
/// redirects to /authoring/:id after this resolves.
export function createStrategy(
  body: CreateStrategyReq,
): Promise<CreateStrategyOut> {
  const trace = createTrace("strategy", {
    display_name_len: body.name.length,
  });
  const started = performance.now();
  trace.info("strategy.create.start");
  return apiFetch<CreateStrategyOut>("/api/strategies", {
    method: "POST",
    body: JSON.stringify(body),
  })
    .then((result) => {
      trace.info("strategy.create.ok", {
        strategy_id: result.id,
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("strategy.create.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function cloneStrategy(
  id: string,
  req: CloneStrategyReq,
): Promise<Strategy> {
  const trace = createTrace("strategy", { strategy_id: id });
  const started = performance.now();
  trace.info("strategy.clone.start", {
    display_name_len: req.display_name?.length ?? 0,
  });
  return apiFetch<Strategy>(`/api/strategy/${encodeURIComponent(id)}/clone`, {
    method: "POST",
    body: JSON.stringify(req),
  })
    .then((strategy) => {
      trace.info("strategy.clone.ok", {
        strategy_id: strategy.manifest.id,
        duration_ms: durationSince(started),
      });
      return strategy;
    })
    .catch((err) => {
      trace.error("strategy.clone.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function deleteStrategy(id: string): Promise<void> {
  const trace = createTrace("strategy", { strategy_id: id });
  const started = performance.now();
  trace.info("strategy.delete.start");
  return apiFetch<void>(`/api/strategy/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
    .then((result) => {
      trace.info("strategy.delete.ok", {
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("strategy.delete.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}
