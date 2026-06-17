// Strategies API — wraps `engine::api::strategy::*` (PR #4 list/get,
// PR #47 mutations). Strategy / slot / risk / validation types are
// hand-rolled because the strategy module doesn't have ts-rs derives yet;
// if those land later, replace these with `import type { ... } from
// "./types.gen"`.

import { useMutation, useQueryClient } from "@tanstack/react-query";
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
  creator?: string | null;
  decision_cadence_minutes: number;
  tags?: string[];
  color?: string | null;
  model?: string;
  providers?: string[];
  models?: string[];
  provider_models?: ProviderModelPair[];
  capabilities?: string[];
  agent_count?: number;
  filter_count?: number;
  activation_mode?: ActivationMode;
  asset_universe?: string[];
  execution_mode?: string;
  /// blake3 content hash (hex) of the strategy bundle — the id older
  /// CLI-launched eval runs carry in `run.agent_id`, and the key of
  /// autooptimizer `lineage_nodes`. Optional: absent on pre-upgrade servers.
  bundle_hash?: string;
  /// `"optimizer"` when this strategy's bundle hash appears in the
  /// autooptimizer lineage (it is evaluated inside optimizer cycles).
  /// Treat absent as `"user"`.
  origin?: StrategyOrigin;
  /// Server-computed: true when at least one COMPLETED eval run references
  /// this strategy (by ULID or bundle hash), over the FULL eval_runs table —
  /// not just the page the client fetched.
  evaluated?: boolean;
  /// `completed_at` (RFC3339) of the most recent completed eval run.
  last_eval_completed_at?: string | null;
};

/// Mirrors the generated `types.gen/StrategyOrigin.ts`; kept inline so this
/// hand-rolled module stays self-contained (see header comment).
export type StrategyOrigin = "user" | "optimizer";

export type ProviderModelPair = {
  provider: string;
  model: string;
};

export type PipelineKind = "single" | "sequential" | "graph";

// Capability the agent slot plays in this strategy. See
// `frontend/web/src/api/types.gen/Capability.ts` for the canonical
// generated form; mirror it locally so this module doesn't have to
// import the generated barrel for one type.
export type Capability = "trader" | "filter" | "router";

// Predicate evaluated against an upstream Filter agent's signal. Mirrors
// `EdgePredicate` from the engine — kept inline so strategies.ts stays
// self-contained (the strategy module doesn't have ts-rs derives yet).
export type EdgePredicate =
  | { eq: { signal_field: string; value: unknown } }
  | { neq: { signal_field: string; value: unknown } }
  | { gte: { signal_field: string; value: unknown } }
  | { lte: { signal_field: string; value: unknown } }
  | { in: { signal_field: string; values: unknown[] } }
  | { all: EdgePredicate[] }
  | { any: EdgePredicate[] }
  | { not: EdgePredicate };

/** FK reference to a trained nanochat checkpoint. Mirrors
 *  `CheckpointRef` in `crates/xvision-engine/src/strategies/agent_ref.rs`
 *  (added in WU-1.1). Absent from the wire for non-nanochat slots. */
export type CheckpointRef = {
  model_id: string;
};

export type AgentRef = {
  agent_id: string;
  role: string;
  /// Which capability this position activates. `undefined` (default)
  /// lets the runtime pick the slot's first capability — `trader` for
  /// every legacy slot. Set to `"filter"` by the inline composer when
  /// adding a Filter agent to a strategy.
  activates?: Capability | null;
  /** When present, this slot runs a local nanochat checkpoint as a
   *  pre-filter. Absent (undefined) = omitted from wire → existing
   *  strategy hashes remain byte-stable. Added in WU-8.3. */
  checkpoint?: CheckpointRef | null;
  /** Hard-gate mode: true = block trade on NEUTRAL output (default for
   *  nanochat filter slots), false = advisory only.
   *  Absent (undefined) = omitted from wire. Added in WU-8.3. */
  veto?: boolean | null;
};
export type PipelineEdge = {
  from_role: string;
  to_role: string;
  /// Optional firing predicate. `undefined` (default) = unconditional;
  /// the edge fires every cycle. `Some(p)` gates the edge on the
  /// upstream Filter agent's most-recent signal.
  condition?: EdgePredicate | null;
};
export type PipelineDef = {
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
  attested_with: string;
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
  attested_with: string[];
  required_tools: string[];
  risk_preset_or_config: string;
  published_at: string | null;
  color?: string | null;
};

export type { Filter } from "./types.gen/Filter";
import type { Filter } from "./types.gen/Filter";
import type { ActivationMode } from "./types.gen/ActivationMode";
export type { StrategyRequirements } from "./types.gen/StrategyRequirements";
export type { Requirement } from "./types.gen/Requirement";
import type { StrategyRequirements } from "./types.gen/StrategyRequirements";
export type { MarketplaceProvenance } from "./types.gen/MarketplaceProvenance";
import type { MarketplaceProvenance } from "./types.gen/MarketplaceProvenance";

/// One input knob declared in a Pine Script (or manually added) that the
/// optimizer is allowed to tune. Mirrors `TunableBound` in the Rust engine
/// (`crates/xvision-engine/src/strategies/mod.rs`). Added by WU-A; surfaced
/// in the settings UI by WU-C.
export type TunableBound = {
  /// Dot-separated path into the strategy (e.g. `conditions.0.rhs.numeric`).
  path: string;
  min: number | null;
  max: number | null;
  step: number | null;
  kind: "int" | "float" | "bool";
};

export type Strategy = {
  manifest: PublicManifest;
  regime_slot: LLMSlot | null;
  trader_slot: LLMSlot | null;
  risk: RiskConfig;
  agents?: AgentRef[];
  pipeline?: PipelineDef;
  /// Per-strategy deterministic gate. `null` (or absent) means
  /// `EveryBar` — the strategy fires on every cycle. Non-null means
  /// `Filtered` — the engine evaluates the DSL each bar.
  activation_mode?: ActivationMode;
  filter?: Filter | null;
  /// Whether the strategy uses LLM agents (default) or deterministic
  /// mechanistic rules. Absent from wire for agentic strategies.
  decision_mode?: DecisionMode;
  /// Rule-based entry/exit config. Required when decision_mode is
  /// "mechanistic"; absent for agentic strategies.
  mechanistic_config?: MechanisticConfig | null;
  /// Declared search-space bounds, one per Pine `input.*` knob (or
  /// manually added). Populated by WU-A from `input_mutation_targets`.
  /// Absent / empty for non-Pine strategies — treat as `[]`.
  tunable_bounds?: TunableBound[];
  /// Indicators referenced in briefings (e.g. RSI, EMA column names).
  /// Added to the TS type for parity with the Rust engine field
  /// (`briefing_indicators`) which landed in #998. Absent for strategies
  /// that pre-date that release — treat as `[]`.
  briefing_indicators?: string[];
};

export type SetFilterBody = {
  source: string;
  format: "json";
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
  /// Soft signals — saveable but worth surfacing in the strategy editor
  /// alongside errors. As of the firing-filter wave the engine populates
  /// this with the no-Filter warning (Trader agent with no upstream
  /// Filter). Optional on the wire so older server responses continue to
  /// parse — treat `undefined` as `[]`.
  warnings?: string[];
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

export type StrategyMetadataPatch = {
  display_name?: string;
  plain_summary?: string;
  asset_universe?: string[];
  decision_cadence_minutes?: number;
  color?: string;
  /** Strategy author/owner handle. Non-empty sets the creator (e.g. the
   *  operator's profile handle); omitted/empty leaves it untouched. */
  creator?: string;
};

export type DecisionMode = "agentic" | "mechanistic";
export type EntryDirection = "long" | "short";

export type EntryRule = {
  signal_name: string;
  direction: EntryDirection;
};

export type ClosePolicy =
  | { kind: "stop_loss"; pct: number }
  | { kind: "take_profit"; pct: number }
  | { kind: "trailing_stop"; pct: number }
  | { kind: "time_exit"; bars: number }
  | { kind: "target_pnl"; usd: number };

export type ExitReason =
  | "stop_loss"
  | "take_profit"
  | "trailing_stop"
  | "time_expiry"
  | "signal"
  | "manual";

export type MechanisticConfig = {
  entry_rules: EntryRule[];
  close_policies: ClosePolicy[];
};

export type SetMechanisticBody = {
  decision_mode: DecisionMode;
  mechanistic_config?: MechanisticConfig | null;
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
  requirements: (id: string) =>
    [...strategyKeys.all, "requirements", id] as const,
  marketplace: (id: string) =>
    [...strategyKeys.all, "marketplace", id] as const,
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

/// Per-strategy model/skill/tool readiness for the buyer's machine. The
/// Strategy detail page highlights gaps and gates the eval/go-live action
/// when `all_models_satisfied` is false. Models gate; skills warn; tools
/// are informational.
export function getStrategyRequirements(
  id: string,
): Promise<StrategyRequirements> {
  return apiFetch<StrategyRequirements>(
    `/api/strategy/${encodeURIComponent(id)}/requirements`,
  );
}

/// Marketplace provenance for a strategy acquired from the marketplace
/// (issue #12 / QA #8): creator, price paid, license NFT, explorer link.
/// `null` when the strategy was not bought (hand-authored / optimizer). Stored
/// in a backend sidecar, NOT on the Strategy artifact — so it is fetched
/// separately rather than read off `getStrategy`.
export function getStrategyMarketplace(
  id: string,
): Promise<MarketplaceProvenance | null> {
  return apiFetch<MarketplaceProvenance | null>(
    `/api/strategy/${encodeURIComponent(id)}/marketplace`,
  );
}

export function patchStrategyMetadata(
  id: string,
  patch: StrategyMetadataPatch,
): Promise<Strategy> {
  return apiFetch<Strategy>(`/api/strategy/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(patch),
  });
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

/// Install/replace the per-strategy deterministic filter. `source` is
/// the raw DSL JSON text; the engine
/// parses + validates server-side and returns the resolved Filter on
/// success. Validation/parse failures come back as ApiError (4xx).
export function setStrategyFilter(
  id: string,
  body: SetFilterBody,
): Promise<Strategy> {
  return apiFetch<Strategy>(
    `/api/strategy/${encodeURIComponent(id)}/filter`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
}

/// Drop the per-strategy filter and revert activation_mode back to
/// `EveryBar`. Server returns 204 No Content; the helper resolves to
/// void.
export function clearStrategyFilter(id: string): Promise<void> {
  return apiFetch<void>(
    `/api/strategy/${encodeURIComponent(id)}/filter`,
    { method: "DELETE" },
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
  body: { agent_id: string; role: string; activates?: Capability },
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
/// redirects to /strategies/:id after this resolves.
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

export function setMechanisticConfig(
  id: string,
  body: SetMechanisticBody,
): Promise<Strategy> {
  return apiFetch<Strategy>(
    `/api/strategy/${encodeURIComponent(id)}/mechanistic`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
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

// ── s3ph.27: persist AgentRef.checkpoint / veto end-to-end ──────────────────

/** Body for `PUT /api/strategy/:id/agents/:role/checkpoint`. */
export type PatchAgentCheckpointBody = {
  /** New checkpoint reference, or `null` to clear. */
  checkpoint: CheckpointRef | null;
  /** Veto mode, or `null` to clear. */
  veto: boolean | null;
};

/** Persist a nanochat checkpoint selection on a strategy's `AgentRef` slot.
 *  Calls `PUT /api/strategy/:id/agents/:role/checkpoint` → returns the
 *  updated `Strategy`. The backend runs the full live_approved +
 *  indicator-compat gate before saving. */
export function patchAgentCheckpoint(
  strategyId: string,
  role: string,
  body: PatchAgentCheckpointBody,
): Promise<Strategy> {
  return apiFetch<Strategy>(
    `/api/strategy/${encodeURIComponent(strategyId)}/agents/${encodeURIComponent(role)}/checkpoint`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
}

/** React Query mutation hook for `patchAgentCheckpoint`.
 *
 *  On success, invalidates the strategy detail query (and its validate
 *  sibling) so the authoring page re-renders with the persisted state. */
export function useSetAgentCheckpoint(strategyId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      role,
      body,
    }: {
      role: string;
      body: PatchAgentCheckpointBody;
    }) => patchAgentCheckpoint(strategyId, role, body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: strategyKeys.detail(strategyId) });
      void qc.invalidateQueries({ queryKey: strategyKeys.validate(strategyId) });
    },
  });
}
