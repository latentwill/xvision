// Strategies API — wraps `engine::api::strategy::*` (PR #4 list/get,
// PR #47 mutations). Bundle / slot / risk / validation types are
// hand-rolled because the bundle module doesn't have ts-rs derives yet;
// if those land later, replace these with `import type { ... } from
// "./types.gen"`.

import { apiFetch } from "./client";
import type { StrategySummary } from "./types.gen";

export type StrategiesListResponse = {
  items: StrategySummary[];
};

export type LLMSlot = {
  role: string;
  prompt: string;
  model_requirement: string;
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

export type StrategyBundle = {
  manifest: PublicManifest;
  regime_slot: LLMSlot | null;
  intern_slot: LLMSlot | null;
  trader_slot: LLMSlot | null;
  risk: RiskConfig;
  mechanical_params: unknown;
};

export type UpdateSlotBody = Partial<{
  prompt: string;
  model_requirement: string;
  allowed_tools: string[];
}>;

export type UpdateSlotOut = {
  id: string;
  updated: string[];
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

export const strategyKeys = {
  all: ["strategies"] as const,
  list: () => [...strategyKeys.all, "list"] as const,
  detail: (id: string) => [...strategyKeys.all, "detail", id] as const,
  validate: (id: string) => [...strategyKeys.all, "validate", id] as const,
};

export function listStrategies(): Promise<StrategySummary[]> {
  return apiFetch<StrategiesListResponse>("/api/strategies").then(
    (r) => r.items,
  );
}

export function getStrategy(id: string): Promise<StrategyBundle> {
  return apiFetch<StrategyBundle>(`/api/strategy/${encodeURIComponent(id)}`);
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
