// MemorySurface — namespace-scoped memory UI shared between the
// per-agent `<MemoryTab>` and the workspace-level `<MemoryPage>`.
//
// V2D v1.1 introduced two operator surfaces over the memory store:
//
//   - `/agents/<id>` Memory tab — scoped to `agent:<id>` (with the
//     option to flip the picker to `global`).
//   - `/memory` workspace page — scoped to `global` only.
//
// Phase 3 hand-rolled the surface inside `MemoryTab.tsx`. Phase 4
// lifts the list / modal / forget logic up here so the workspace page
// can reuse the same Patterns + Observations + AlertDialog shapes
// without duplicating ~500 LOC of TanStack-Query plumbing.
//
// The surface is configured by a discriminated `mode`:
//   - mode="agent" — Patterns sub-tab exposes the agent↔global
//     namespace picker; Observations sub-tab filters by scenario_id /
//     run_id and is scoped to `agent:<id>`; "Forget all memory"
//     deletes by `agent`.
//   - mode="workspace" — Patterns + Observations are both pinned to
//     `namespace=global`; "Forget all global memory" deletes by
//     `namespace=global`.
//
// Optional `highlightPatternId` (sourced from the `?pattern=<id>`
// query param on either route) tags the matching pattern row with
// `data-highlighted="true"` and a gold-tinted background so deep-link
// navigation from the eval-review MemoryPanel scrolls into a visibly
// distinct row.

import { useEffect, useMemo, useRef, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import {
  demoteAutoOptimizerRun,
  flywheelKeys,
  gateOptimization,
  gateAutoOptimizerRun,
  getFlywheelLineage,
  getFlywheelStatus,
  getFlywheelVelocity,
  listAutoOptimizerRuns,
  optimizeMemoryDemos,
  promoteAutoOptimizerRun,
  runAutoOptimizer,
  type MemoryDemoOptimizeResult,
} from "@/api/flywheel";
import {
  activatePattern,
  agentNamespace,
  createOperatorAttestation,
  createPattern,
  demotePattern,
  forgetMemory,
  listMemory,
  memoryKeys,
  type MemoryItem,
  type PatternCreateBody,
} from "@/api/memory";
import { Card, CardHeader } from "@/components/primitives/Card";
import { formatVerdict, formatPromotionState } from "./labels";

type SubTab = "patterns" | "observations";
type GateDraft = {
  parentDayScore: string;
  childDayScore: string;
  parentHoldoutScore: string;
  childHoldoutScore: string;
  gateEpsilon: string;
  gateReason: string;
};

type OptimizationGateDraft = {
  parentDevScore: string;
  childDevScore: string;
  parentHoldoutScore: string;
  childHoldoutScore: string;
  gateEpsilon: string;
  gateReason: string;
};

const emptyGateDraft: GateDraft = {
  parentDayScore: "",
  childDayScore: "",
  parentHoldoutScore: "",
  childHoldoutScore: "",
  gateEpsilon: "0",
  gateReason: "",
};

const emptyOptimizationGateDraft: OptimizationGateDraft = {
  parentDevScore: "",
  childDevScore: "",
  parentHoldoutScore: "",
  childHoldoutScore: "",
  gateEpsilon: "0",
  gateReason: "",
};

export type MemorySurfaceProps =
  | {
      mode: "agent";
      agentId: string;
      highlightPatternId?: string | null;
    }
  | {
      mode: "workspace";
      highlightPatternId?: string | null;
    };

export function MemorySurface(props: MemorySurfaceProps) {
  const [sub, setSub] = useState<SubTab>("patterns");
  const [forgetOpen, setForgetOpen] = useState(false);

  // Item count for the forget-dialog summary. The query shape mirrors
  // Phase 3's MemoryTab so the cache key matches across surfaces.
  const allItemsQuery = useQuery({
    queryKey:
      props.mode === "agent"
        ? memoryKeys.list({ agent: props.agentId })
        : memoryKeys.list({ namespace: "global" }),
    queryFn: () =>
      props.mode === "agent"
        ? listMemory({ agent: props.agentId })
        : listMemory({ namespace: "global" }),
  });

  return (
    <div className="flex flex-col gap-5">
      <FlywheelPanel {...props} />

      <Card>
        <CardHeader title="Memory" />
        <div className="px-5 pb-5">
          <SubTabBar value={sub} onChange={setSub} />
          <div
            role="tabpanel"
            aria-label={sub === "patterns" ? "Patterns" : "Observations"}
          >
            {sub === "patterns" ? (
              <PatternsPanel
                {...props}
                highlightPatternId={props.highlightPatternId ?? null}
              />
            ) : (
              <ObservationsPanel {...props} />
            )}
          </div>
        </div>
      </Card>

      <div className="flex items-center justify-end">
        <button
          type="button"
          onClick={() => setForgetOpen(true)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[12px] font-medium border border-danger/40 text-danger hover:bg-danger/10 transition-colors"
        >
          {props.mode === "agent"
            ? "Forget all memory for this agent"
            : "Forget all global memory"}
        </button>
      </div>

      {forgetOpen ? (
        <ForgetPanel
          {...props}
          itemCount={allItemsQuery.data?.total ?? 0}
          onClose={() => setForgetOpen(false)}
        />
      ) : null}
    </div>
  );
}

// ── flywheel panel ─────────────────────────────────────────────────────────

type FlywheelPanelProps = MemorySurfaceProps & {
  fullHistory?: boolean;
};

export function FlywheelPanel(props: FlywheelPanelProps) {
  const qc = useQueryClient();
  const namespace =
    props.mode === "agent" ? agentNamespace(props.agentId) : "global";
  const lineageLimit = props.fullHistory ? 20 : 1;
  const runLimit = props.fullHistory ? 25 : 5;
  const statusQuery = useQuery({
    queryKey: flywheelKeys.status(
      props.mode === "agent" ? { agent: props.agentId } : { namespace },
    ),
    queryFn: () =>
      getFlywheelStatus(
        props.mode === "agent" ? { agent: props.agentId } : { namespace },
      ),
  });
  const velocityQueryArgs =
    props.mode === "agent"
      ? { agent: props.agentId, days: 7 }
      : { namespace, days: 7 };
  const velocityQuery = useQuery({
    queryKey: flywheelKeys.velocity(velocityQueryArgs),
    queryFn: () => getFlywheelVelocity(velocityQueryArgs),
  });
  const lineageQueryArgs =
    props.mode === "agent"
      ? { agent: props.agentId, limit: lineageLimit }
      : { namespace, limit: lineageLimit };
  const lineageQuery = useQuery({
    queryKey: flywheelKeys.lineage(lineageQueryArgs),
    queryFn: () => getFlywheelLineage(lineageQueryArgs),
  });
  const runsQueryArgs =
    props.mode === "agent"
      ? { agent: props.agentId, limit: runLimit }
      : { namespace, limit: runLimit };
  const runsQuery = useQuery({
    queryKey: flywheelKeys.autooptimizerList(runsQueryArgs),
    queryFn: () => listAutoOptimizerRuns(runsQueryArgs),
  });

  const [patternText, setPatternText] = useState("");
  const [embeddingJson, setEmbeddingJson] = useState("");
  const [childName, setChildName] = useState("");
  const [demoSource, setDemoSource] = useState("frozen-snapshot");
  const [holdoutSplit, setHoldoutSplit] = useState("70/15/15");
  const [autoPriors, setAutoPriors] = useState(false);
  const [optimizeResult, setOptimizeResult] =
    useState<MemoryDemoOptimizeResult | null>(null);
  const [gateDrafts, setGateDrafts] = useState<Record<string, GateDraft>>({});
  const [optimizationGateDrafts, setOptimizationGateDrafts] = useState<
    Record<string, OptimizationGateDraft>
  >({});
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: flywheelKeys.all });
    qc.invalidateQueries({ queryKey: memoryKeys.all });
  };

  const autooptimizerMutation = useMutation({
    mutationFn: () => {
      if (!patternText.trim()) {
        throw new Error("Pattern text is required.");
      }
      return runAutoOptimizer({
        ...(props.mode === "agent"
          ? { agent: props.agentId }
          : { namespace: "global" }),
        pattern_text: patternText.trim(),
        embedding: parseEmbeddingJson(embeddingJson),
        min_observations: 2,
      });
    },
    onSuccess: () => {
      setPatternText("");
      setError(null);
      refresh();
    },
    onError: (err) => {
      const raw = errorMessage(err);
      // Translate the backend validation error into operator-friendly copy.
      if (/not enough observations/i.test(raw)) {
        setError(
          "Not enough observations to stage a pattern. Run at least 2 cycles with this agent first.",
        );
      } else {
        setError(raw);
      }
    },
  });

  const optimizeMutation = useMutation({
    mutationFn: () => {
      if (props.mode !== "agent") {
        throw new Error("Memory-demo optimization requires an agent.");
      }
      return optimizeMemoryDemos({
        target_agent_id: props.agentId,
        demo_source: demoSource,
        holdout_split: holdoutSplit,
        auto_prior_patterns: autoPriors,
        prior_pattern_limit: 5,
        apply: true,
        child_name: childName.trim() || undefined,
      });
    },
    onSuccess: (result) => {
      setOptimizeResult(result);
      setChildName("");
      setError(null);
      refresh();
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const lifecycleMutation = useMutation({
    mutationFn: ({ id, action }: { id: string; action: "promote" | "demote" }) =>
      action === "promote"
        ? promoteAutoOptimizerRun(id)
        : demoteAutoOptimizerRun(id),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const gateMutation = useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: GateDraft }) =>
      gateAutoOptimizerRun(id, {
        parent_day_score: parseScore("parent day score", draft.parentDayScore),
        child_day_score: parseScore("child day score", draft.childDayScore),
        parent_holdout_score: parseScore(
          "parent holdout score",
          draft.parentHoldoutScore,
        ),
        child_holdout_score: parseScore(
          "child holdout score",
          draft.childHoldoutScore,
        ),
        gate_epsilon: parseScore("gate epsilon", draft.gateEpsilon || "0"),
        gate_reason: draft.gateReason.trim() || undefined,
      }),
    onSuccess: (_run, vars) => {
      setError(null);
      setGateDrafts((prev) => ({
        ...prev,
        [vars.id]: emptyGateDraft,
      }));
      refresh();
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const optimizationGateMutation = useMutation({
    mutationFn: ({
      id,
      draft,
    }: {
      id: string;
      draft: OptimizationGateDraft;
    }) =>
      gateOptimization(id, {
        parent_dev_score: parseScore("parent dev score", draft.parentDevScore),
        child_dev_score: parseScore("child dev score", draft.childDevScore),
        parent_holdout_score: parseScore(
          "parent holdout score",
          draft.parentHoldoutScore,
        ),
        child_holdout_score: parseScore(
          "child holdout score",
          draft.childHoldoutScore,
        ),
        gate_epsilon: parseScore("gate epsilon", draft.gateEpsilon || "0"),
        gate_reason: draft.gateReason.trim() || undefined,
      }),
    onSuccess: (_gate, vars) => {
      setError(null);
      setOptimizationGateDrafts((prev) => ({
        ...prev,
        [vars.id]: emptyOptimizationGateDraft,
      }));
      refresh();
    },
    onError: (err) => setError(errorMessage(err)),
  });

  const updateGateDraft = (id: string, patch: Partial<GateDraft>) => {
    setGateDrafts((prev) => ({
      ...prev,
      [id]: {
        ...(prev[id] ?? emptyGateDraft),
        ...patch,
      },
    }));
  };
  const updateOptimizationGateDraft = (
    id: string,
    patch: Partial<OptimizationGateDraft>,
  ) => {
    setOptimizationGateDrafts((prev) => ({
      ...prev,
      [id]: {
        ...(prev[id] ?? emptyOptimizationGateDraft),
        ...patch,
      },
    }));
  };

  const status = statusQuery.data;
  const runs = runsQuery.data?.items ?? [];
  const lineageItems = lineageQuery.data?.items ?? [];

  return (
    <Card>
      <CardHeader title="Flywheel" />
      <div className="px-5 pb-5 space-y-4">
        {statusQuery.isError ? (
          <div className="text-danger text-[13px]">
            Couldn't load flywheel status: {errorMessage(statusQuery.error)}
          </div>
        ) : (
          <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
            <Metric label="Observations" value={status?.observations} />
            <Metric label="Active" value={status?.active_patterns} />
            <Metric label="Staged" value={status?.staged_patterns} />
            <Metric label="Forgotten" value={status?.forgotten_patterns} />
            <Metric label="Runs" value={status?.autooptimizer_runs} />
          </div>
        )}

        {velocityQuery.data ? (
          <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
            <Metric
              label="Obs / 7d"
              value={velocityQuery.data.observations_captured}
            />
            <Metric
              label="Activated / 7d"
              value={velocityQuery.data.patterns_promoted}
            />
            <Metric
              label="Retired / 7d"
              value={velocityQuery.data.patterns_demoted}
            />
            <Metric
              label="New versions / 7d"
              value={velocityQuery.data.optimized_child_agents}
            />
            <Metric
              label="Generations deep"
              value={Number(velocityQuery.data.average_lineage_depth.toFixed(2))}
            />
          </div>
        ) : null}

        {lineageItems.length > 0 ? (
          <div className="border border-border rounded-sm px-3 py-2 text-[12px] text-text-2">
            <div className="text-[10.5px] uppercase tracking-wide text-text-3">
              {props.fullHistory ? "Training run history" : "Latest Lineage"}
            </div>
            <div className="mt-1 divide-y divide-border">
              {lineageItems.map((item) => {
                const gateDraft =
                  optimizationGateDrafts[item.optimization_id] ??
                  emptyOptimizationGateDraft;
                return (
                  <div
                    key={item.optimization_id}
                    className="py-1.5 first:pt-0 last:pb-0"
                  >
                    <div className="font-mono truncate">
                      {item.optimization_id} · target {item.target_agent_id} ·
                      child {item.child_agent_id ?? "none"}
                    </div>
                    <div className="mt-1 text-text-3">
                      examples {item.train_observation_count}/
                      {item.dev_observation_count}/
                      {item.holdout_observation_count} · patterns{" "}
                      {item.demo_source_pattern_ids.length} · background patterns{" "}
                      {item.prior_pattern_ids.length} · {item.status}
                    </div>
                    <div className="mt-1 font-mono text-[11px] text-text-3 truncate">
                      untouched test {shortHash(item.holdout_hash)} · training{" "}
                      {shortHash(item.train_hash)} · validation{" "}
                      {shortHash(item.dev_hash)}
                    </div>
                    {item.gate_verdict ? (
                      <div className="mt-1 text-[11px] text-text-3">
                        Decision: {formatVerdict(item.gate_verdict)} · validation{" "}
                        {formatDelta(item.delta_dev)} · untouched test{" "}
                        {formatDelta(item.delta_holdout)}
                        {item.gate_reason ? ` · ${item.gate_reason}` : ""}
                      </div>
                    ) : (
                      <div className="mt-2 grid gap-2 md:grid-cols-6 md:items-end">
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Baseline validation score
                          </span>
                          <input
                            aria-label={`Baseline validation score ${item.optimization_id}`}
                            type="number"
                            step="any"
                            value={gateDraft.parentDevScore}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  parentDevScore: e.target.value,
                                },
                              )
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Candidate validation score
                          </span>
                          <input
                            aria-label={`Candidate validation score ${item.optimization_id}`}
                            type="number"
                            step="any"
                            value={gateDraft.childDevScore}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  childDevScore: e.target.value,
                                },
                              )
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Baseline untouched-period score
                          </span>
                          <input
                            aria-label={`Baseline untouched-period score ${item.optimization_id}`}
                            type="number"
                            step="any"
                            value={gateDraft.parentHoldoutScore}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  parentHoldoutScore: e.target.value,
                                },
                              )
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Candidate untouched-period score
                          </span>
                          <input
                            aria-label={`Candidate untouched-period score ${item.optimization_id}`}
                            type="number"
                            step="any"
                            value={gateDraft.childHoldoutScore}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  childHoldoutScore: e.target.value,
                                },
                              )
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Min improvement
                          </span>
                          <input
                            aria-label={`Min improvement ${item.optimization_id}`}
                            type="number"
                            step="any"
                            value={gateDraft.gateEpsilon}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  gateEpsilon: e.target.value,
                                },
                              )
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2">
                          <input
                            aria-label={`Optimization gate reason ${item.optimization_id}`}
                            type="text"
                            value={gateDraft.gateReason}
                            onChange={(e) =>
                              updateOptimizationGateDraft(
                                item.optimization_id,
                                {
                                  gateReason: e.target.value,
                                },
                              )
                            }
                            className="min-w-0 px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                          <button
                            type="button"
                            onClick={() =>
                              optimizationGateMutation.mutate({
                                id: item.optimization_id,
                                draft: gateDraft,
                              })
                            }
                            disabled={optimizationGateMutation.isPending}
                            className="px-2 py-1 rounded text-[11.5px] border border-border text-text-2 hover:text-text hover:border-border-strong disabled:opacity-40"
                          >
                            {optimizationGateMutation.isPending
                              ? "Recording..."
                              : `Record gate decision for ${item.optimization_id}`}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_180px_auto] md:items-end">
          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
              Candidate Pattern
            </span>
            <input
              type="text"
              value={patternText}
              onChange={(e) => setPatternText(e.target.value)}
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13px] text-text focus:outline-none focus:border-gold/40"
            />
          </label>
          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
              Embedding JSON
            </span>
            <input
              type="text"
              value={embeddingJson}
              onChange={(e) => setEmbeddingJson(e.target.value)}
              placeholder="e.g. [0.12, -0.34, …]"
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13px] text-text font-mono placeholder:text-text-4 focus:outline-none focus:border-gold/40"
            />
          </label>
          {(() => {
            const MIN_OBSERVATIONS = 2;
            const obsCount = status?.observations ?? 0;
            const tooFewObs =
              !statusQuery.isPending && obsCount < MIN_OBSERVATIONS;
            const isDisabled = autooptimizerMutation.isPending || tooFewObs;
            return (
              <button
                type="button"
                onClick={() => autooptimizerMutation.mutate()}
                disabled={isDisabled}
                title={
                  tooFewObs
                    ? `Needs at least ${MIN_OBSERVATIONS} observations to stage a pattern (currently ${obsCount})`
                    : undefined
                }
                className="inline-flex justify-center px-3 py-2 rounded text-[12.5px] font-medium border border-border text-text hover:border-border-strong disabled:opacity-50"
              >
                {autooptimizerMutation.isPending ? "Staging..." : "Stage Pattern"}
              </button>
            );
          })()}
        </div>

        {props.mode === "agent" ? (
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_150px_120px_auto] md:items-end">
            <label className="block">
              <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
                Child Agent Name
              </span>
              <input
                type="text"
                value={childName}
                onChange={(e) => setChildName(e.target.value)}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13px] text-text focus:outline-none focus:border-gold/40"
              />
            </label>
            <label className="block">
              <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
                Demo Source
              </span>
              <select
                value={demoSource}
                onChange={(e) => setDemoSource(e.target.value)}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13px] text-text focus:outline-none focus:border-gold/40"
              >
                <option value="frozen-snapshot">Use saved examples</option>
                <option value="fresh-recorder">Capture new examples</option>
              </select>
            </label>
            <label className="block">
              <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
                Split
              </span>
              <input
                type="text"
                value={holdoutSplit}
                onChange={(e) => setHoldoutSplit(e.target.value)}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13px] text-text font-mono focus:outline-none focus:border-gold/40"
              />
            </label>
            <label className="flex items-center gap-2 text-[12px] text-text-2">
              <input
                type="checkbox"
                checked={autoPriors}
                onChange={(e) => setAutoPriors(e.target.checked)}
                className="h-4 w-4 rounded-sm border-border bg-surface-panel"
              />
              Include patterns I've already learned
            </label>
            <button
              type="button"
              onClick={() => optimizeMutation.mutate()}
              disabled={optimizeMutation.isPending}
              className="inline-flex justify-center px-3 py-2 rounded text-[12.5px] font-medium border border-border text-text hover:border-border-strong disabled:opacity-50"
            >
              {optimizeMutation.isPending ? "Training..." : "Train new version"}
            </button>
          </div>
        ) : null}

        {optimizeResult ? (
          <div className="grid gap-2 md:grid-cols-4">
            <Metric
              label="Example patterns"
              value={optimizeResult.pattern_demo_source_count ?? 0}
            />
            <Metric
              label="Background patterns"
              value={optimizeResult.pattern_prior_count ?? 0}
            />
            <Metric
              label="Training examples"
              value={
                optimizeResult.train_observation_ids?.length ??
                optimizeResult.demo_count
              }
            />
            <Metric
              label="Untouched test examples"
              value={optimizeResult.holdout_observation_ids?.length ?? 0}
            />
          </div>
        ) : null}

        <div className="border border-border rounded-sm overflow-hidden">
          <div className="px-3 py-2 border-b border-border text-[11px] uppercase tracking-wide text-text-3">
            {props.fullHistory ? "Optimizer History" : "Recent Optimizer Runs"}
          </div>
          {runsQuery.isPending ? (
            <div className="px-3 py-3 text-[12.5px] text-text-3">
              Loading runs...
            </div>
          ) : runsQuery.isError ? (
            <div className="px-3 py-3 text-[12.5px] text-danger">
              Couldn't load runs: {errorMessage(runsQuery.error)}
            </div>
          ) : runs.length === 0 ? (
            <div className="px-3 py-3 text-[12.5px] text-text-3">
              No autooptimizer runs yet.
            </div>
          ) : (
            <div className="divide-y divide-border">
              {runs.map((run) => {
                const gateDraft = gateDrafts[run.id] ?? emptyGateDraft;
                const isStaged = run.promotion_state === "staged";
                const isDemoted = run.promotion_state === "demoted";
                const gateVerdict =
                  run.gate_verdict ??
                  (run.gate_passed == null
                    ? null
                    : run.gate_passed
                      ? "passed"
                      : "failed");
                const gateSummary =
                  gateVerdict == null
                    ? null
                    : [
                        `Decision: ${formatVerdict(gateVerdict)}`,
                        run.gate_metric,
                        run.delta_day != null
                          ? `day ${run.delta_day.toFixed(3)}`
                          : null,
                        run.delta_holdout != null
                          ? `untouched test ${run.delta_holdout.toFixed(3)}`
                          : null,
                        run.gate_reason ?? run.finding_text,
                      ]
                        .filter(Boolean)
                        .join(" · ");
                return (
                  <div
                    key={run.id}
                    className="grid gap-2 px-3 py-2 md:grid-cols-[minmax(0,1fr)_90px_auto] md:items-center"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-[12.5px] text-text">
                        {run.pattern_text}
                      </div>
                      <div className="mt-0.5 text-[11px] text-text-3 font-mono truncate">
                        {run.id} · {run.observation_ids.length} obs
                      </div>
                      {gateSummary ? (
                        <div className="mt-0.5 text-[11px] text-text-3 truncate">
                          {gateSummary}
                        </div>
                      ) : null}
                    </div>
                    <span className="text-[11px] uppercase tracking-wide text-text-3">
                      {formatPromotionState(run.promotion_state)}
                    </span>
                    <div className="flex gap-2 md:justify-end">
                      <button
                        type="button"
                        onClick={() =>
                          lifecycleMutation.mutate({
                            id: run.id,
                            action: "promote",
                          })
                        }
                        disabled={
                          !isStaged ||
                          gateVerdict !== "passed" ||
                          lifecycleMutation.isPending
                        }
                        className="px-2 py-1 rounded text-[11.5px] border border-border text-text-2 hover:text-text hover:border-border-strong disabled:opacity-40"
                      >
                        Activate
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          lifecycleMutation.mutate({
                            id: run.id,
                            action: "demote",
                          })
                        }
                        disabled={isDemoted || lifecycleMutation.isPending}
                        className="px-2 py-1 rounded text-[11.5px] border border-danger/40 text-danger hover:bg-danger/10 disabled:opacity-40"
                      >
                        Retire
                      </button>
                    </div>
                    {isStaged ? (
                      <div className="grid gap-2 md:col-span-3 md:grid-cols-6 md:items-end">
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Baseline today's score
                          </span>
                          <input
                            aria-label={`Baseline today's score ${run.id}`}
                            type="number"
                            step="any"
                            value={gateDraft.parentDayScore}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                parentDayScore: e.target.value,
                              })
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Candidate today's score
                          </span>
                          <input
                            aria-label={`Candidate today's score ${run.id}`}
                            type="number"
                            step="any"
                            value={gateDraft.childDayScore}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                childDayScore: e.target.value,
                              })
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Baseline untouched-period score
                          </span>
                          <input
                            aria-label={`Baseline untouched-period score ${run.id}`}
                            type="number"
                            step="any"
                            value={gateDraft.parentHoldoutScore}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                parentHoldoutScore: e.target.value,
                              })
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Candidate untouched-period score
                          </span>
                          <input
                            aria-label={`Candidate untouched-period score ${run.id}`}
                            type="number"
                            step="any"
                            value={gateDraft.childHoldoutScore}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                childHoldoutScore: e.target.value,
                              })
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <label className="block">
                          <span className="block text-[10.5px] uppercase tracking-wide text-text-3 mb-1">
                            Min improvement
                          </span>
                          <input
                            aria-label={`Min improvement ${run.id}`}
                            type="number"
                            step="any"
                            value={gateDraft.gateEpsilon}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                gateEpsilon: e.target.value,
                              })
                            }
                            className="w-full px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                        </label>
                        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 md:col-span-1">
                          <input
                            aria-label={`Gate reason ${run.id}`}
                            type="text"
                            value={gateDraft.gateReason}
                            onChange={(e) =>
                              updateGateDraft(run.id, {
                                gateReason: e.target.value,
                              })
                            }
                            className="min-w-0 px-2 py-1.5 bg-surface-panel border border-border rounded-sm text-[12px] text-text focus:outline-none focus:border-gold/40"
                          />
                          <button
                            type="button"
                            onClick={() =>
                              gateMutation.mutate({
                                id: run.id,
                                draft: gateDraft,
                              })
                            }
                            disabled={gateMutation.isPending}
                            className="px-2 py-1 rounded text-[11.5px] border border-border text-text-2 hover:text-text hover:border-border-strong disabled:opacity-40"
                          >
                            {gateMutation.isPending
                              ? "Recording..."
                              : "Record gate decision"}
                          </button>
                        </div>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {error ? <div className="text-danger text-[12.5px]">{error}</div> : null}
      </div>
    </Card>
  );
}

function Metric({
  label,
  value,
}: {
  label: string;
  value: number | undefined;
}) {
  return (
    <div className="border border-border bg-surface-panel rounded-sm px-3 py-2">
      <div className="text-[10.5px] uppercase tracking-wide text-text-3">
        {label}
      </div>
      <div className="mt-1 text-[18px] font-medium text-text tabular-nums">
        {value ?? "—"}
      </div>
    </div>
  );
}

// ── sub-tab bar ─────────────────────────────────────────────────────────────

function SubTabBar({
  value,
  onChange,
}: {
  value: SubTab;
  onChange: (s: SubTab) => void;
}) {
  const tabs: [SubTab, string][] = [
    ["patterns", "Patterns"],
    ["observations", "Observations"],
  ];
  return (
    <div
      role="tablist"
      aria-label="Memory sub-tabs"
      className="flex gap-4 border-b border-border mb-4"
    >
      {tabs.map(([t, label]) => (
        <button
          key={t}
          type="button"
          role="tab"
          aria-selected={value === t}
          aria-label={label}
          onClick={() => onChange(t)}
          className={`pb-2 -mb-px border-b-2 text-[13px] font-medium transition-colors ${
            value === t
              ? "border-gold text-text"
              : "border-transparent text-text-3 hover:text-text-2"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

// ── patterns panel ──────────────────────────────────────────────────────────

type PatternsPanelProps = MemorySurfaceProps & {
  highlightPatternId: string | null;
};

function PatternsPanel(props: PatternsPanelProps) {
  const [addOpen, setAddOpen] = useState(false);
  const [lifecycle, setLifecycle] = useState<
    "all" | "active" | "staged" | "forgotten"
  >("all");
  // Agent mode lets the operator toggle between the agent-scoped
  // namespace and the shared `global` shelf. Workspace mode pins to
  // `global` — no toggle, since the per-agent page already owns the
  // agent-scoped view.
  const [scope, setScope] = useState<"agent" | "global">(
    props.mode === "agent" ? "agent" : "global",
  );

  const namespace =
    props.mode === "agent"
      ? scope === "agent"
        ? agentNamespace(props.agentId)
        : "global"
      : "global";

  const listArgs = {
    tier: "pattern" as const,
    namespace,
    ...(lifecycle === "active" || lifecycle === "staged"
      ? { promotion_state: lifecycle }
      : {}),
    ...(lifecycle === "forgotten" ? { forgotten_only: true } : {}),
  };

  const query = useQuery({
    queryKey: memoryKeys.list(listArgs),
    queryFn: () => listMemory(listArgs),
  });

  const items = query.data?.items ?? [];

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          {props.mode === "agent" ? (
            <label className="text-[12px] text-text-3">
              <span className="mr-2">Namespace</span>
              <select
                value={scope}
                onChange={(e) =>
                  setScope(e.target.value as "agent" | "global")
                }
                className="bg-surface-panel border border-border rounded-sm text-[12.5px] text-text px-2 py-1 focus:outline-none focus:border-gold/40"
              >
                <option value="agent">agent:{props.agentId}</option>
                <option value="global">global</option>
              </select>
            </label>
          ) : (
            <span className="text-[12px] text-text-3">
              Namespace <code className="font-mono text-text-2">global</code>
            </span>
          )}
          <label className="text-[12px] text-text-3">
            <span className="mr-2">Lifecycle</span>
            <select
              value={lifecycle}
              onChange={(e) =>
                setLifecycle(
                  e.target.value as "all" | "active" | "staged" | "forgotten",
                )
              }
              className="bg-surface-panel border border-border rounded-sm text-[12.5px] text-text px-2 py-1 focus:outline-none focus:border-gold/40"
            >
              <option value="all">all live</option>
              <option value="active">active</option>
              <option value="staged">staged</option>
              <option value="forgotten">forgotten</option>
            </select>
          </label>
        </div>
        <button
          type="button"
          onClick={() => setAddOpen(true)}
          className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:text-text hover:border-border-strong transition-colors"
        >
          + Add Pattern
        </button>
      </div>

      {query.isPending ? (
        <div className="text-text-3 text-[13px] py-6">Loading patterns…</div>
      ) : query.isError ? (
        <div className="text-danger text-[13px] py-6">
          Couldn't load patterns: {errorMessage(query.error)}
        </div>
      ) : items.length === 0 ? (
        <div className="text-text-3 text-[13px] py-6">
          No {lifecycle === "all" ? "" : `${lifecycle} `}patterns yet for{" "}
          <code className="font-mono">{namespace}</code>.
          Use "+ Add Pattern" to seed one.
        </div>
      ) : (
        <PatternList
          items={items}
          highlightPatternId={props.highlightPatternId}
        />
      )}

      {addOpen ? (
        <AddPatternPanel
          {...props}
          defaultNamespace={namespace}
          onClose={() => setAddOpen(false)}
        />
      ) : null}
    </div>
  );
}

function PatternList({
  items,
  highlightPatternId,
}: {
  items: MemoryItem[];
  highlightPatternId: string | null;
}) {
  const qc = useQueryClient();
  const highlightRef = useRef<HTMLLIElement | null>(null);
  const lifecycleMutation = useMutation({
    mutationFn: ({ id, action }: { id: string; action: "activate" | "demote" }) =>
      action === "activate" ? activatePattern(id) : demotePattern(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      qc.invalidateQueries({ queryKey: flywheelKeys.all });
    },
  });

  // Scroll the deep-linked row into view once it mounts. We don't
  // animate; a soft jump keeps the page reload-shareable without
  // confusing the operator about where focus moved.
  useEffect(() => {
    // `scrollIntoView` is unavailable in jsdom (vitest); guard so the
    // highlight effect doesn't crash unit tests. Production browsers
    // always have it.
    if (
      highlightPatternId &&
      highlightRef.current &&
      typeof highlightRef.current.scrollIntoView === "function"
    ) {
      highlightRef.current.scrollIntoView({
        block: "center",
        behavior: "auto",
      });
    }
  }, [highlightPatternId, items.length]);

  return (
    <ul className="flex flex-col gap-2">
      {items.map((it) => {
        const highlighted = highlightPatternId === it.id;
        const isForgotten = Boolean(it.forgotten_at);
        const state = it.promotion_state ?? "active";
        return (
          <li
            key={it.id}
            ref={highlighted ? highlightRef : null}
            data-highlighted={highlighted ? "true" : undefined}
            className={
              "border rounded-sm px-3 py-2 transition-colors " +
              (highlighted
                ? "border-gold/60 bg-gold/[0.08]"
                : "border-border bg-surface-panel")
            }
          >
            <div className="flex items-start justify-between gap-3">
              <div className="text-[13px] text-text whitespace-pre-wrap">
                {it.text}
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <div className="text-[11px] text-text-3 font-mono">
                  {it.training_window_end
                    ? `ends ${it.training_window_end.slice(0, 10)}`
                    : "open"}
                </div>
                <button
                  type="button"
                  onClick={() =>
                    lifecycleMutation.mutate({
                      id: it.id,
                      action: "activate",
                    })
                  }
                  disabled={
                    state === "active" ||
                    isForgotten ||
                    lifecycleMutation.isPending
                  }
                  className="px-2 py-1 rounded text-[11px] border border-border text-text-2 hover:text-text hover:border-border-strong disabled:opacity-40"
                >
                  Activate
                </button>
                <button
                  type="button"
                  onClick={() =>
                    lifecycleMutation.mutate({ id: it.id, action: "demote" })
                  }
                  disabled={isForgotten || lifecycleMutation.isPending}
                  className="px-2 py-1 rounded text-[11px] border border-danger/40 text-danger hover:bg-danger/10 disabled:opacity-40"
                >
                  Demote
                </button>
              </div>
            </div>
            <div className="mt-1 flex items-center gap-2 text-[10.5px] text-text-3 font-mono">
              <span>{it.namespace}</span>
              <span>·</span>
              <span>{formatPromotionState(isForgotten ? "forgotten" : state)}</span>
              <span>·</span>
              <span>{it.created_at.slice(0, 10)}</span>
            </div>
          </li>
        );
      })}
    </ul>
  );
}

// ── add-pattern modal ───────────────────────────────────────────────────────

type AddPatternPanelProps = MemorySurfaceProps & {
  defaultNamespace: string;
  onClose: () => void;
};

type AddPatternMutationBody = PatternCreateBody & {
  operator_initials?: string;
};

function AddPatternPanel(props: AddPatternPanelProps) {
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const [trainingEnd, setTrainingEnd] = useState("");
  const [namespace, setNamespace] = useState(props.defaultNamespace);
  const [attestNullWindow, setAttestNullWindow] = useState(false);
  const [operatorInitials, setOperatorInitials] = useState("");
  const [submitError, setSubmitError] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: async (body: AddPatternMutationBody) => {
      const { operator_initials, ...pattern } = body;
      if (!pattern.training_window_end) {
        const attestation = await createOperatorAttestation({
          operator_initials: operator_initials ?? "",
          surface: "dashboard",
        });
        return createPattern({
          ...pattern,
          attestation_id: attestation.id,
        });
      }
      return createPattern(pattern);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      props.onClose();
    },
    onError: (err) => setSubmitError(errorMessage(err)),
  });

  function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!text.trim()) {
      setSubmitError("Text is required.");
      return;
    }
    if (!namespace.trim()) {
      setSubmitError("Namespace is required.");
      return;
    }
    const training_window_end = trainingEnd
      ? `${trainingEnd}T23:59:59Z`
      : undefined;
    if (!training_window_end && !attestNullWindow) {
      setSubmitError("Confirm null-window attestation before saving.");
      return;
    }
    if (!training_window_end && !operatorInitials.trim()) {
      setSubmitError("Operator initials are required for null-window Patterns.");
      return;
    }

    m.mutate({
      text: text.trim(),
      namespace: namespace.trim(),
      training_window_end,
      operator_initials: training_window_end
        ? undefined
        : operatorInitials.trim(),
    });
  }

  return (
    <div className="mt-3 w-full bg-surface-card border border-border rounded-lg shadow-sm">
        <form onSubmit={onSubmit} className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-sans font-medium text-[20px] tracking-tight text-text">
              Add Pattern
            </h2>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Patterns are operator-attested wisdom. The dispatcher recalls
              them on every cycle whose scenario starts after the training
              window end (if set).
            </p>
          </div>

          <div
            role="note"
            aria-label="Embedding provider requirement"
            className="px-3 py-2 rounded-sm border border-amber-500/40 bg-amber-500/5 text-[11.5px] text-amber-900 dark:text-amber-200 leading-snug"
          >
            Requires an embedding provider.{" "}
            Patterns are matched to decision context via vector similarity, so
            an agent's provider (or a configured default) must support
            embeddings. Without one, this Pattern is stored but never recalled —
            check Settings → Providers, or watch eval-review for a{" "}
            <code className="font-mono">memory_disabled_no_embedder</code>{" "}
            event after the next run.
          </div>

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Text
            </span>
            <textarea
              required
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={4}
              placeholder="e.g. Mean-revert entries fail on FOMC days; sit out announcement bars."
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
            />
          </label>

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Training data ends
            </span>
            <input
              type="date"
              value={trainingEnd}
              onChange={(e) => setTrainingEnd(e.target.value)}
              title="The latest date your training data covers. Normalized to end-of-day UTC at submit. Scenarios with start_date <= this date will EXCLUDE this Pattern (look-ahead protection); scenarios starting after will recall it."
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
            <div className="mt-1 space-y-0.5 text-[11px] text-text-3 leading-snug">
              <p className="m-0">
                Optional. The date your training data ends (end-of-day UTC at
                submit).
              </p>
              <p className="m-0">
                Scenarios starting <em>after</em> this date will recall this
                Pattern; scenarios overlapping or earlier exclude it
                (look-ahead protection).
              </p>
              <p className="m-0">
                Blank dates require operator attestation and recall in{" "}
                <em>every</em> scenario.
              </p>
            </div>
          </label>

          {!trainingEnd ? (
            <div className="space-y-3 rounded-sm border border-amber-500/40 bg-amber-500/5 px-3 py-2">
              <label className="flex items-start gap-2 text-[11.5px] text-amber-900 dark:text-amber-200 leading-snug">
                <input
                  type="checkbox"
                  checked={attestNullWindow}
                  onChange={(e) => setAttestNullWindow(e.target.checked)}
                  className="mt-0.5"
                />
                <span>
                  I attest this Pattern has no training window and may be
                  recalled in every scenario.
                </span>
              </label>
              <label className="block">
                <span className="block text-[11px] uppercase tracking-wide text-amber-900 dark:text-amber-200 mb-1">
                  Operator initials
                </span>
                <input
                  type="text"
                  value={operatorInitials}
                  onChange={(e) => setOperatorInitials(e.target.value)}
                  maxLength={12}
                  className="w-full px-3 py-2 bg-surface-panel border border-amber-500/40 rounded-sm text-[13px] text-text font-mono focus:outline-none focus:border-gold/40"
                />
              </label>
            </div>
          ) : null}

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Namespace
            </span>
            <select
              value={namespace}
              onChange={(e) => setNamespace(e.target.value)}
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            >
              {props.mode === "agent" ? (
                <>
                  <option value={agentNamespace(props.agentId)}>
                    agent:{props.agentId}
                  </option>
                  <option value="global">global</option>
                </>
              ) : (
                <option value="global">global</option>
              )}
            </select>
          </label>

          {submitError ? (
            <div className="text-danger text-[12.5px]">{submitError}</div>
          ) : null}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={props.onClose}
              className="px-3 py-1.5 rounded text-[12.5px] text-text-2 hover:text-text"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={m.isPending}
              className="px-3 py-1.5 rounded text-[12.5px] font-medium border border-border text-text hover:border-border-strong disabled:opacity-50"
            >
              {m.isPending ? "Saving…" : "Add Pattern"}
            </button>
          </div>
        </form>
    </div>
  );
}

// ── observations panel ─────────────────────────────────────────────────────

function ObservationsPanel(props: MemorySurfaceProps) {
  const [scenarioId, setScenarioId] = useState("");
  const [runId, setRunId] = useState("");

  const debouncedScenario = useDebounced(scenarioId, 250);
  const debouncedRun = useDebounced(runId, 250);

  const query = useQuery({
    queryKey: memoryKeys.list(
      props.mode === "agent"
        ? {
            tier: "observation",
            agent: props.agentId,
            scenario_id: debouncedScenario || undefined,
            run_id: debouncedRun || undefined,
          }
        : {
            tier: "observation",
            namespace: "global",
            scenario_id: debouncedScenario || undefined,
            run_id: debouncedRun || undefined,
          },
    ),
    queryFn: () =>
      listMemory(
        props.mode === "agent"
          ? {
              tier: "observation",
              agent: props.agentId,
              scenario_id: debouncedScenario || undefined,
              run_id: debouncedRun || undefined,
            }
          : {
              tier: "observation",
              namespace: "global",
              scenario_id: debouncedScenario || undefined,
              run_id: debouncedRun || undefined,
            },
      ),
  });

  const items = query.data?.items ?? [];

  const emptyCopy =
    props.mode === "agent"
      ? "No observations yet for this agent."
      : "No observations yet for the global namespace.";

  return (
    <div>
      <div className="flex flex-wrap items-end gap-3 mb-3">
        <label className="block">
          <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
            Scenario id
          </span>
          <input
            type="text"
            value={scenarioId}
            onChange={(e) => setScenarioId(e.target.value)}
            placeholder="filter by scenario"
            className="px-2.5 py-1.5 bg-surface-panel border border-border rounded-sm text-[12.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
        </label>
        <label className="block">
          <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
            Run id
          </span>
          <input
            type="text"
            value={runId}
            onChange={(e) => setRunId(e.target.value)}
            placeholder="filter by run"
            className="px-2.5 py-1.5 bg-surface-panel border border-border rounded-sm text-[12.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
        </label>
        <p className="m-0 ml-auto text-[11px] text-text-3">
          Observations are read-only. Use "Forget all memory" to clear.
        </p>
      </div>

      {query.isPending ? (
        <div className="text-text-3 text-[13px] py-6">Loading observations…</div>
      ) : query.isError ? (
        <div className="text-danger text-[13px] py-6">
          Couldn't load observations: {errorMessage(query.error)}
        </div>
      ) : items.length === 0 ? (
        <div className="text-text-3 text-[13px] py-6">{emptyCopy}</div>
      ) : (
        <ObservationList items={items} />
      )}
    </div>
  );
}

function ObservationList({ items }: { items: MemoryItem[] }) {
  return (
    <ul className="flex flex-col gap-2">
      {items.map((it) => (
        <li
          key={it.id}
          className="border border-border rounded-sm bg-surface-panel px-3 py-2"
        >
          <div className="text-[13px] text-text whitespace-pre-wrap">
            {it.text}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[10.5px] text-text-3 font-mono">
            <span>{it.created_at.slice(0, 19).replace("T", " ")}</span>
            {it.scenario_id ? (
              <>
                <span>·</span>
                <span>scenario={it.scenario_id}</span>
              </>
            ) : null}
            {it.run_id ? (
              <>
                <span>·</span>
                <span>run={it.run_id.slice(0, 12)}…</span>
              </>
            ) : null}
            {it.cycle_idx != null ? (
              <>
                <span>·</span>
                <span>cycle={it.cycle_idx}</span>
              </>
            ) : null}
          </div>
        </li>
      ))}
    </ul>
  );
}

// ── forget dialog ──────────────────────────────────────────────────────────

type ForgetPanelProps = MemorySurfaceProps & {
  itemCount: number;
  onClose: () => void;
};

function ForgetPanel(props: ForgetPanelProps) {
  const qc = useQueryClient();
  const m = useMutation({
    mutationFn: () =>
      props.mode === "agent"
        ? forgetMemory({ agent: props.agentId })
        : forgetMemory({ namespace: "global" }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      props.onClose();
    },
  });

  const title =
    props.mode === "agent"
      ? "Forget all memory for this agent?"
      : "Forget all global memory?";
  const namespaceCode =
    props.mode === "agent" ? agentNamespace(props.agentId) : "global";

  return (
    <div className="mt-3 w-full bg-surface-card border border-danger/30 rounded-lg shadow-sm">
      <div className="p-5 space-y-4">
        <div>
          <h2 className="m-0 font-sans font-medium text-[18px] tracking-tight text-text">
            {title}
          </h2>
          <p className="m-0 mt-2 text-text-2 text-[13px]">
            This will soft-delete{" "}
            <span className="font-mono text-text">{props.itemCount}</span>{" "}
            memory item{props.itemCount === 1 ? "" : "s"} from namespace{" "}
            <code className="font-mono text-text">{namespaceCode}</code>.
            Observations and Patterns alike. Items can be restored during the
            configured grace window.
          </p>
        </div>

        {m.isError ? (
          <div className="text-danger text-[12.5px]">
            {errorMessage(m.error)}
          </div>
        ) : null}

        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={props.onClose}
            disabled={m.isPending}
            className="px-3 py-1.5 rounded text-[12.5px] text-text-2 hover:text-text disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => m.mutate()}
            disabled={m.isPending}
            className="px-3 py-1.5 rounded text-[12.5px] font-medium border border-danger/40 text-danger hover:bg-danger/10 disabled:opacity-50"
          >
            {m.isPending ? "Forgetting…" : "Confirm forget"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── helpers ────────────────────────────────────────────────────────────────

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return err.message;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function shortHash(value: string | null | undefined): string {
  if (!value) return "none";
  const prefix = "sha256:";
  if (value.startsWith(prefix)) {
    return `${prefix}${value.slice(prefix.length, prefix.length + 12)}`;
  }
  return value.length > 18 ? `${value.slice(0, 18)}...` : value;
}

function formatDelta(value: number | null | undefined): string {
  return typeof value === "number" ? value.toFixed(3) : "n/a";
}

function parseScore(label: string, raw: string): number {
  const n = Number(raw);
  if (!Number.isFinite(n)) {
    throw new Error(`${label} must be a finite number.`);
  }
  return n;
}

function parseEmbeddingJson(raw: string): number[] {
  const parsed = JSON.parse(raw) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error("Embedding JSON must be an array.");
  }
  const out = parsed.map((value) => {
    const n = Number(value);
    if (!Number.isFinite(n)) {
      throw new Error("Embedding JSON values must be finite numbers.");
    }
    return n;
  });
  if (out.length === 0) {
    throw new Error("Embedding JSON must not be empty.");
  }
  return out;
}

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return debounced;
}

// Re-export for back-compat with Phase 3 callers that imported the
// item-count helper. Kept here so external consumers don't import from
// the per-agent component file directly.
export function useMemoryItemCount(items: MemoryItem[]): number {
  return useMemo(() => items.length, [items]);
}
