import { useEffect, useMemo, useState } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import { toVenuePair } from "@/lib/assets";
import { useAlpacaAssets } from "@/api/assets";
import {
  addStrategyAgent,
  deleteStrategy,
  getStrategy,
  getStrategyRequirements,
  patchStrategyMetadata,
  renameStrategyAgentRole,
  removeStrategyAgent,
  setMechanisticConfig,
  setRiskConfig,
  setStrategyPipeline,
  strategyKeys,
  validateDraft,
  type AgentRef,
  type ClosePolicy,
  type EntryDirection,
  type EntryRule,
  type PipelineDef,
  type PipelineKind,
  type RiskConfig,
  type Strategy,
  type StrategyRequirements,
} from "@/api/strategies";
import { createAgent, listAgents, type Agent } from "@/api/agents";
// `FiringSection` is still exported from `@/components/strategy` for the
// deferred per-agent filter composer; re-add it to this import when
// un-deferring (see authoring.tsx FilterCard wiring below).
import { FilterCard, StrategyRequirementChip } from "@/components/strategy";
import { listProviders, settingsKeys } from "@/api/settings";
import { getStrategyChart, strategyChartKeys } from "@/api/chart";
import { StrategyHistoryChartV2 } from "@/components/chart/v2/surfaces/StrategyHistoryChartV2";
import { ModelPicker } from "@/components/ModelPicker";
import { TimeframeSelect } from "@/components/TimeframeSelect";
import type { ProviderRow } from "@/api/types.gen";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";

const AGENT_COLLAPSE_KEY_PREFIX = "xvn:authoring:agent-collapsed";

function agentCollapseKey(strategyId: string, role: string): string {
  return `${AGENT_COLLAPSE_KEY_PREFIX}:${strategyId}:${role}`;
}

export function AuthoringRoute() {
  const params = useParams<{ id?: string }>();

  if (!params.id) {
    return <Navigate to="/strategies" replace />;
  }

  return <InspectorPage id={params.id} />;
}

function InspectorPage({ id }: { id: string }) {
  const strategyQ = useQuery({
    queryKey: strategyKeys.detail(id),
    queryFn: () => getStrategy(id),
  });

  return (
    <>
      <Topbar
        title={strategyQ.data?.manifest.display_name || "Strategy"}
        back={{ to: "/strategies", label: "Back to strategies" }}
        sub={
          strategyQ.data ? (
            <>
              <span>Strategy inspector</span>
              <span className="mx-1.5 text-text-3">·</span>
              <span>Strategy ID:</span>
              <span className="ml-1 break-all font-mono text-[12px] text-text-3">
                {id}
              </span>
            </>
          ) : (
            <span className="break-all font-mono text-[12px] text-text-3">
              {id}
            </span>
          )
        }
      />

      <InspectorActions strategyId={id} strategy={strategyQ.data ?? null} />

      <div className="space-y-5">
        <div className="space-y-5">
          {strategyQ.isPending ? (
            <Card>
              <LoadingSkeleton />
            </Card>
          ) : strategyQ.isError ? (
            <Card>
              <ErrorState
                err={strategyQ.error}
                onRetry={() => strategyQ.refetch()}
              />
            </Card>
          ) : strategyQ.data ? (
            <>
              <StrategyQuickPerformanceCard strategyId={id} />
              <StrategyEditor strategy={strategyQ.data} />
            </>
          ) : null}
        </div>
      </div>
    </>
  );
}

function StrategyQuickPerformanceCard({ strategyId }: { strategyId: string }) {
  const chart = useQuery({
    queryKey: strategyChartKeys.strategy(strategyId),
    queryFn: () => getStrategyChart(strategyId),
  });
  const chartPayload = chart.data
    ? {
        strategy_id: chart.data.strategy_id ?? strategyId,
        scenarios: chart.data.scenarios ?? [],
        run_series: Array.isArray(chart.data.run_series) ? chart.data.run_series : [],
      }
    : null;
  const summary = chartPayload
    ? summarizeStrategyRuns(chartPayload.run_series)
    : null;

  return (
    <Card>
      <SectionHeader
        label="Quick performance"
        hint="Completed eval history for this strategy, before setup details."
      />
      <div className="px-5 pt-4 pb-5">
        {chart.isPending && (
          <div className="text-text-3 text-[13px] py-4">Loading history…</div>
        )}
        {chart.isError && (
          <div className="text-danger text-[13px] py-4">
            Could not load chart.
          </div>
        )}
        {chartPayload && summary && (
          <div className="space-y-4">
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-2">
              <QuickMetric
                label="Completed evals"
                value={formatEvalCount(summary.evalCount)}
              />
              <QuickMetric label="Best PnL" value={fmtPnlUsd(summary.bestPnl)} />
              <QuickMetric label="Best Sharpe" value={fmtMetric(summary.bestSharpe)} />
              <QuickMetric
                label="Max drawdown"
                value={fmtSignedPct(summary.maxDrawdown)}
              />
            </div>
            <StrategyHistoryChartV2 payload={chartPayload} />
          </div>
        )}
      </div>
    </Card>
  );
}

function QuickMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-border-soft bg-surface-elev px-3 py-2">
      <div className="text-[11px] uppercase tracking-wide text-text-3">
        {label}
      </div>
      <div className="mt-1 font-mono text-[16px] text-text">{value}</div>
    </div>
  );
}

type StrategyRunSummary = {
  evalCount: number;
  bestPnl: number | null;
  bestSharpe: number | null;
  maxDrawdown: number | null;
};

function summarizeStrategyRuns(
  runs: Array<{
    final_pnl_usd?: number | null;
    sharpe?: number | null;
    max_drawdown_pct?: number | null;
  }>,
): StrategyRunSummary {
  return {
    evalCount: runs.length,
    bestPnl: maxFinite(runs.map((run) => run.final_pnl_usd)),
    bestSharpe: maxFinite(runs.map((run) => run.sharpe)),
    maxDrawdown: minFinite(runs.map((run) => run.max_drawdown_pct)),
  };
}

function maxFinite(values: Array<number | null | undefined>): number | null {
  const finite = values.filter((value): value is number => Number.isFinite(value));
  return finite.length > 0 ? Math.max(...finite) : null;
}

function minFinite(values: Array<number | null | undefined>): number | null {
  const finite = values.filter((value): value is number => Number.isFinite(value));
  return finite.length > 0 ? Math.min(...finite) : null;
}

function formatEvalCount(count: number): string {
  return `${count} ${count === 1 ? "eval" : "evals"}`;
}

function fmtMetric(value: number | null): string {
  return value === null ? "—" : value.toFixed(2);
}

function fmtPnlUsd(value: number | null): string {
  if (value === null) return "—";
  const abs = `$${Math.abs(value).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
  if (value > 0) return `+${abs}`;
  if (value < 0) return `−${abs}`;
  return abs;
}

function fmtSignedPct(value: number | null): string {
  if (value === null) return "—";
  const abs = `${Math.abs(value).toFixed(2)}%`;
  if (value > 0) return `+${abs}`;
  if (value < 0) return `−${abs}`;
  return abs;
}

function StrategyEditor({ strategy }: { strategy: Strategy }) {
  const isMechanistic = strategy.decision_mode === "mechanistic";
  return (
    <>
      <ManifestCard strategy={strategy} />
      <RequirementsCard strategyId={strategy.manifest.id} />
      <FilterCard strategy={strategy} />
      {isMechanistic ? (
        <MechanisticConfigCard strategy={strategy} />
      ) : (
        <AgentsCard strategy={strategy} />
      )}
      <RiskCard strategy={strategy} />
      <ValidationCard strategy={strategy} />
    </>
  );
}

// QA #4 + Q1: a purchased strategy may reference models/skills the buyer
// hasn't configured locally. This full-width inline card (no right sidebar —
// chat-rail rule) lists each requirement as satisfied (✓) or missing (⚠) with
// a Configure CTA. Missing MODEL requirements gate the eval/go-live action in
// `InspectorActions`; skills warn and tools are informational.
function configureTargetFor(kind: string): string {
  // Models live under Settings → Providers; everything else (skills, tools)
  // routes to the Settings root.
  return kind === "model" ? "/settings/providers" : "/settings";
}

function RequirementsCard({ strategyId }: { strategyId: string }) {
  const requirementsQ = useQuery({
    queryKey: strategyKeys.requirements(strategyId),
    queryFn: () => getStrategyRequirements(strategyId),
    staleTime: 30_000,
  });

  if (requirementsQ.isPending) {
    return (
      <Card>
        <SectionHeader
          label="Requirements"
          hint="Checking the models and skills this strategy needs on your machine..."
        />
        <div className="px-5 pt-4 pb-5 text-[13px] text-text-3">Loading…</div>
      </Card>
    );
  }
  if (requirementsQ.isError) {
    return (
      <Card>
        <SectionHeader
          label="Requirements"
          hint="Could not load this strategy's requirements."
        />
        <div className="px-5 pt-4 pb-5 text-[13px] text-danger">
          {errorMessage(requirementsQ.error)}
        </div>
      </Card>
    );
  }

  const data = requirementsQ.data;
  const requirements = data.requirements ?? [];
  if (requirements.length === 0) {
    return (
      <Card>
        <SectionHeader
          label="Requirements"
          hint="This strategy declares no model, skill, or tool requirements."
        />
      </Card>
    );
  }

  const missing = requirements.filter((r) => !r.satisfied);
  const hint = data.all_models_satisfied
    ? "All required models are configured on this machine."
    : "Some required models are not configured — configure them before running eval.";

  return (
    <Card>
      <SectionHeader label="Requirements" hint={hint} />
      <div className="px-5 pt-4 pb-5 space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          {requirements.map((requirement) => (
            <StrategyRequirementChip
              key={`${requirement.kind}:${requirement.name}`}
              requirement={requirement}
            />
          ))}
        </div>
        {missing.length > 0 ? (
          <ul className="space-y-1.5">
            {missing.map((requirement) => (
              <li
                key={`missing:${requirement.kind}:${requirement.name}`}
                className="flex flex-wrap items-center gap-2 text-[12px] text-text-2"
              >
                <span className="font-mono text-text">{requirement.name}</span>
                {requirement.hint ? (
                  <span className="text-text-3">— {requirement.hint}</span>
                ) : null}
                <Link
                  to={configureTargetFor(requirement.kind)}
                  className="inline-flex items-center gap-1 rounded border border-border px-2 py-0.5 text-[12px] font-medium text-text hover:border-text-3"
                >
                  Configure
                </Link>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </Card>
  );
}

// L2 of `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`:
// the strategy editor surfaces the engine's soft warnings (today, the
// no-Filter warning) alongside errors so an operator sees the same
// signal whether they're using the CLI or the SPA. Renders nothing
// while validation is loading or when there are no
// errors/warnings to report.
function ValidationCard({ strategy }: { strategy: Strategy }) {
  const validation = useQuery({
    queryKey: strategyKeys.validate(strategy.manifest.id),
    queryFn: () => validateDraft(strategy.manifest.id),
    enabled: false,
    // The engine path is cheap (in-memory shape check + filesystem
    // load); a 30-second staleTime avoids refetching on every keystroke
    // while still picking up changes after an authoring mutation
    // invalidates the cache key.
    staleTime: 30_000,
  });

  if (!validation.isFetched) {
    return (
      <Card>
        <SectionHeader
          label="Eval readiness"
          hint="Run validation when you are ready to launch or check the strategy."
        />
        <div className="px-5 pt-4 pb-5">
          <button
            type="button"
            onClick={() => validation.refetch()}
            disabled={validation.isFetching}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 disabled:opacity-50"
          >
            {validation.isFetching ? "Checking..." : "Check eval readiness"}
          </button>
        </div>
      </Card>
    );
  }
  if (validation.isPending) return null;
  if (validation.isError) {
    return (
      <Card>
        <SectionHeader label="Eval readiness" hint="Validation request failed." />
        <div className="px-5 pt-4 pb-5 text-[13px] text-danger">
          {errorMessage(validation.error)}
        </div>
      </Card>
    );
  }
  const { errors = [], warnings = [] } = validation.data ?? {};
  if (errors.length === 0 && warnings.length === 0) {
    return (
      <Card>
        <SectionHeader label="Eval readiness" hint="No blocking validation issues." />
        <div className="px-5 pt-4 pb-5">
          <button
            type="button"
            onClick={() => validation.refetch()}
            disabled={validation.isFetching}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 disabled:opacity-50"
          >
            {validation.isFetching ? "Checking..." : "Recheck"}
          </button>
        </div>
      </Card>
    );
  }

  return (
    <Card>
      <SectionHeader
        label="Validation"
        hint={
          errors.length > 0
            ? "Resolve before launching an eval."
            : "Soft signals — the strategy is still saveable."
        }
      />
      <div className="px-5 pt-4 pb-5">
        <ul className="space-y-2">
          {errors.map((message, i) => (
            <ValidationItem key={`err-${i}`} severity="error" message={message} />
          ))}
          {warnings.map((message, i) => (
            <ValidationItem
              key={`warn-${i}`}
              severity="warning"
              message={message}
            />
          ))}
        </ul>
      </div>
    </Card>
  );
}

function ValidationItem({
  severity,
  message,
}: {
  severity: "error" | "warning";
  message: string;
}) {
  const tone =
    severity === "error"
      ? "bg-danger/10 text-danger border-danger/30"
      : "bg-warn/10 text-warn border-warn/30";
  const label = severity === "error" ? "Error" : "Warning";
  return (
    <li className="flex items-start gap-2.5 text-[13px]">
      <span
        className={`inline-flex items-center px-1.5 py-0.5 text-[10px] uppercase tracking-wide rounded-sm border mt-0.5 ${tone}`}
      >
        {label}
      </span>
      <div className="flex-1 text-text leading-relaxed">{message}</div>
    </li>
  );
}

function AgentsCard({ strategy }: { strategy: Strategy }) {
  const qc = useQueryClient();
  // Pass `scope=<strategy_id>` so agents scoped to this strategy
  // (the "Save as reusable agent" toggle = OFF flow) merge into the
  // picker alongside workspace-visible agents. Strategy-detail
  // endpoints are the documented home for the merged view per
  // `team/contracts/agent-firing-filter-strategy-composer.md`.
  const agentPool = useQuery({
    queryKey: ["agents", "pool", strategy.manifest.id],
    queryFn: () =>
      listAgents({
        include_archived: false,
        limit: 200,
        scope: strategy.manifest.id,
      }),
  });
  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });
  const [newAgentId, setNewAgentId] = useState("");
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentProvider, setNewAgentProvider] = useState<string | null>(null);
  const [newAgentModel, setNewAgentModel] = useState("");
  const [newAgentPrompt, setNewAgentPrompt] = useState("");
  const [renameRoleFrom, setRenameRoleFrom] = useState<string | null>(null);
  const [renameRoleTo, setRenameRoleTo] = useState("");

  const attached = strategy.agents ?? [];
  const pipeline = strategy.pipeline ?? { kind: "single" as const, edges: [] };
  const agentById = useMemo(() => {
    return new Map(
      (agentPool.data ?? []).map((agent) => [agent.agent_id, agent]),
    );
  }, [agentPool.data]);
  const available = (agentPool.data ?? []).filter(
    (a) => !attached.some((r) => r.agent_id === a.agent_id),
  );
  const filterCandidates = (agentPool.data ?? []).filter(agentSupportsFilter);
  const graphEdges = pipeline.edges ?? [];

  function invalidateStrategy() {
    qc.invalidateQueries({ queryKey: strategyKeys.detail(strategy.manifest.id) });
    qc.invalidateQueries({
      queryKey: strategyKeys.validate(strategy.manifest.id),
    });
  }

  const addMut = useMutation({
    mutationFn: (payload: { agent_id: string; role: string }) =>
      addStrategyAgent(strategy.manifest.id, payload),
    onSuccess: () => {
      setNewAgentId("");
      invalidateStrategy();
    },
  });

  const createAttachMut = useMutation({
    mutationFn: async () => {
      if (!newAgentProvider || !newAgentModel) {
        throw new Error("Pick a provider/model for the new agent.");
      }
      const agent = await createAgent({
        name: newAgentName.trim(),
        description: "",
        tags: [],
        slots: [
          {
            name: "main",
            provider: newAgentProvider,
            model: newAgentModel,
            system_prompt: newAgentPrompt.trim(),
            skill_ids: [],
    allowed_tools: [],
            max_tokens: null,
          },
        ],
      });
      const derivedRole = nameToRole(newAgentName);
      await addStrategyAgent(strategy.manifest.id, {
        agent_id: agent.agent_id,
        role: isReservedAgentRole(derivedRole) ? `${derivedRole}-1` : derivedRole,
      });
      return agent;
    },
    onSuccess: async () => {
      setNewAgentName("");
      setNewAgentProvider(null);
      setNewAgentModel("");
      setNewAgentPrompt("");
      await qc.invalidateQueries({ queryKey: ["agents", "pool"] });
      invalidateStrategy();
    },
  });

  const removeMut = useMutation({
    mutationFn: (role: string) => removeStrategyAgent(strategy.manifest.id, role),
    onSuccess: () => {
      invalidateStrategy();
    },
  });

  const renameMut = useMutation({
    mutationFn: (payload: { role: string; newRole: string }) =>
      renameStrategyAgentRole(strategy.manifest.id, payload.role, payload.newRole),
    onSuccess: () => {
      invalidateStrategy();
    },
  });

  const pipelineMut = useMutation({
    mutationFn: (kind: PipelineKind) =>
      setStrategyPipeline(strategy.manifest.id, { kind, edges: [] }),
    onSuccess: invalidateStrategy,
  });

  function renameRole() {
    if (!renameRoleFrom || !renameRoleTo.trim()) return;
    renameMut.mutate({
      role: renameRoleFrom,
      newRole: renameRoleTo.trim(),
    });
    setRenameRoleFrom(null);
    setRenameRoleTo("");
  }

  function onPipelineChange(kind: PipelineKind) {
    if (kind === pipeline.kind || kind === "graph") return;
    pipelineMut.mutate(kind);
  }

  return (
    <Card id="strategy-agents">
      <SectionHeader
        label="Strategy agents"
        hint="Attach reusable AgentRefs and define the pipeline that executes them."
      />
      <div className="px-5 pt-4 pb-5 space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-[220px_1fr] gap-3 items-start border border-border-soft rounded p-3">
          <Field
            label="Pipeline kind"
            hint={
              pipeline.kind === "graph"
                ? "Graph strategies are view-only here; graph runtime intentionally errors today."
                : "Filter-gated agent uses one trader AgentRef. Sequential runs refs in the order below."
            }
          >
            <select
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={pipeline.kind}
              onChange={(e) =>
                onPipelineChange(e.target.value as PipelineKind)
              }
              disabled={pipeline.kind === "graph" || pipelineMut.isPending}
            >
              <option value="single" disabled={attached.length > 1}>
                filter-gated agent
              </option>
              <option value="sequential">sequential</option>
              <option value="graph" disabled>
                graph
              </option>
            </select>
          </Field>
          <div className="text-[12px] text-text-2 leading-snug">
            <div>
              Current:{" "}
              <span className="font-mono text-text">{pipeline.kind}</span>
              {pipelineMut.isPending ? " (saving...)" : ""}
            </div>
            <div className="mt-1">
              {attached.length === 0
                ? strategy.activation_mode === "compiled_rules"
                  ? "No agents attached. OK: rules-only mechanical mode does not call a trader agent."
                  : "No agents attached. This strategy is not eval-ready until a complete trader agent is attached."
                : pipeline.kind === "single"
                  ? "The first AgentRef is the gated trader."
                  : pipeline.kind === "sequential"
                    ? "Execution order follows the AgentRef list from top to bottom."
                    : "Graph edges are preserved from the backend, but editing is intentionally deferred."}
            </div>
            {pipelineMut.isError ? (
              <div className="mt-2 text-danger">
                {errorMessage(pipelineMut.error)}
              </div>
            ) : null}
          </div>
        </div>

        {attached.length === 0 ? (
          <p className="m-0 text-[13px] text-text-3">
            {strategy.activation_mode === "compiled_rules"
              ? "No agents attached. OK for rules-only mechanical mode."
              : "No agents attached yet. Agent-backed modes need a trader agent before eval launch."}
          </p>
        ) : (
          <div className="space-y-2">
            {attached.map((a, idx) => (
              <AttachedAgentRow
                key={`${a.agent_id}:${a.role}`}
                strategyId={strategy.manifest.id}
                agentRef={a}
                index={idx + 1}
                agent={agentById.get(a.agent_id)}
                onRenameRole={() => {
                  setRenameRoleFrom(a.role);
                  setRenameRoleTo(a.role);
                }}
                onRemove={() => removeMut.mutate(a.role)}
                allRefs={attached}
                pipeline={pipeline}
                filterCandidates={filterCandidates}
                providers={providers.data?.providers ?? []}
                onFiringChanged={async () => {
                  await qc.invalidateQueries({ queryKey: ["agents", "pool"] });
                  invalidateStrategy();
                }}
              />
            ))}
          </div>
        )}

        {pipeline.kind === "graph" && graphEdges.length > 0 ? (
          <div className="border border-border-soft rounded p-3">
            <div className="text-[12px] text-text-2 mb-2">Graph edges</div>
            <div className="flex flex-wrap gap-2">
              {graphEdges.map((edge) => (
                <span
                  key={`${edge.from_role}:${edge.to_role}`}
                  className="px-2 py-1 rounded border border-border-soft bg-surface-elev text-[12px] font-mono text-text-2"
                >
                  {edge.from_role} → {edge.to_role}
                </span>
              ))}
            </div>
          </div>
        ) : null}

        {renameRoleFrom && (
          <div className="border border-border-soft rounded p-3 space-y-2">
            <div className="text-[12px] text-text-2">
              Renaming role <code className="break-all">{renameRoleFrom}</code>
            </div>
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={renameRoleTo}
              onChange={(e) => setRenameRoleTo(e.target.value)}
            />
            <div className="flex gap-2">
              <button
                onClick={renameRole}
                className="px-3 py-1.5 rounded text-[12px] border border-border"
              >
                Save role
              </button>
              <button
                onClick={() => setRenameRoleFrom(null)}
                className="px-3 py-1.5 rounded text-[12px] border border-border-soft text-text-2"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Merged attach surface. Per qa-strategy-popup-to-accordion: one
            inline box with a Pick existing / Create new toggle, no separate
            "Attach existing" + "Create and attach" cards. */}
        <AddAgentAccordion
          available={available}
          providers={providers}
          newAgentId={newAgentId}
          setNewAgentId={setNewAgentId}
          newAgentName={newAgentName}
          setNewAgentName={setNewAgentName}
          newAgentProvider={newAgentProvider}
          setNewAgentProvider={setNewAgentProvider}
          newAgentModel={newAgentModel}
          setNewAgentModel={setNewAgentModel}
          newAgentPrompt={newAgentPrompt}
          setNewAgentPrompt={setNewAgentPrompt}
          onAttachExisting={() => {
            const agent = (agentPool.data ?? []).find((a) => a.agent_id === newAgentId);
            const derivedRole = agent ? nameToRole(agent.name) : "agent";
            addMut.mutate({
              agent_id: newAgentId,
              role: isReservedAgentRole(derivedRole) ? `${derivedRole}-1` : derivedRole,
            });
          }}
          attachExistingPending={addMut.isPending}
          onCreateAndAttach={() => createAttachMut.mutate()}
          createPending={createAttachMut.isPending}
          createError={createAttachMut.isError ? createAttachMut.error : null}
        />
      </div>
    </Card>
  );
}

type AddAgentAccordionProps = {
  available: Agent[];
  providers: { data: { providers: ProviderRow[] } | undefined; isPending: boolean };
  newAgentId: string;
  setNewAgentId: (v: string) => void;
  newAgentName: string;
  setNewAgentName: (v: string) => void;
  newAgentProvider: string | null;
  setNewAgentProvider: (v: string | null) => void;
  newAgentModel: string;
  setNewAgentModel: (v: string) => void;
  newAgentPrompt: string;
  setNewAgentPrompt: (v: string) => void;
  onAttachExisting: () => void;
  attachExistingPending: boolean;
  onCreateAndAttach: () => void;
  createPending: boolean;
  createError: unknown;
};

/**
 * Single inline accordion for adding an agent to the strategy. Replaces the
 * previous two-card layout ("Attach existing" + "Create and attach") with a
 * mode toggle. No overlay / popup — the create form expands in place.
 */
function AddAgentAccordion(props: AddAgentAccordionProps) {
  const [mode, setMode] = useState<"existing" | "create">("existing");
  const [open, setOpen] = useState(true);

  return (
    <div
      data-testid="add-agent-accordion"
      className="border border-border-soft rounded"
    >
      <div className="flex items-center justify-between gap-2 px-3 py-2">
        <button
          type="button"
          aria-expanded={open}
          aria-controls="add-agent-accordion-panel"
          onClick={() => setOpen((v) => !v)}
          className="inline-flex items-center gap-2 text-[12px] text-text-2 hover:text-text"
        >
          <span className="inline-flex items-center justify-center w-5 h-5 rounded text-[11px] border border-transparent">
            {open ? "▼" : "▶"}
          </span>
          <span>Add agent</span>
        </button>
        {open ? (
          <div
            className="flex items-center gap-1"
            role="group"
            aria-label="Add agent mode"
          >
            <button
              type="button"
              aria-pressed={mode === "existing"}
              onClick={() => setMode("existing")}
              className={`px-2 py-1 text-[11px] rounded border ${
                mode === "existing"
                  ? "border-border text-text bg-surface-elev"
                  : "border-transparent text-text-3 hover:text-text"
              }`}
            >
              Pick existing
            </button>
            <button
              type="button"
              aria-pressed={mode === "create"}
              onClick={() => setMode("create")}
              className={`px-2 py-1 text-[11px] rounded border ${
                mode === "create"
                  ? "border-border text-text bg-surface-elev"
                  : "border-transparent text-text-3 hover:text-text"
              }`}
            >
              Create new
            </button>
          </div>
        ) : null}
      </div>

      {open ? (
        <div id="add-agent-accordion-panel" className="border-t border-border-soft px-3 py-3 space-y-3">
          {mode === "existing" ? (
            <div className="space-y-2">
              <Field label="Existing agent">
                <select
                  className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
                  value={props.newAgentId}
                  onChange={(e) => props.setNewAgentId(e.target.value)}
                >
                  <option value="">Select agent…</option>
                  {props.available.map((a) => (
                    <option key={a.agent_id} value={a.agent_id}>
                      {a.name} · {a.agent_id}
                    </option>
                  ))}
                </select>
              </Field>
              <button
                type="button"
                onClick={props.onAttachExisting}
                disabled={
                  !props.newAgentId ||
                  props.attachExistingPending
                }
                className="px-3 py-1.5 rounded text-[12px] border border-border disabled:opacity-50"
              >
                {props.attachExistingPending ? "Adding..." : "Add Agent"}
              </button>
            </div>
          ) : (
            <div className="space-y-3">
              <Field label="New agent name">
                <input
                  className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
                  value={props.newAgentName}
                  onChange={(e) => props.setNewAgentName(e.target.value)}
                  placeholder="DeepSeek trader"
                />
              </Field>
              <Field label="New agent model">
                <ModelPicker
                  rows={props.providers.data?.providers ?? []}
                  loading={props.providers.isPending}
                  provider={props.newAgentProvider}
                  model={props.newAgentModel}
                  onChange={(provider, model) => {
                    props.setNewAgentProvider(provider);
                    props.setNewAgentModel(model);
                  }}
                  className="w-full"
                  ariaLabel="New agent model"
                  emptyHint="No enabled models for agent creation"
                />
              </Field>
              <Field label="New agent system prompt">
                <textarea
                  className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono leading-relaxed"
                  value={props.newAgentPrompt}
                  onChange={(e) => props.setNewAgentPrompt(e.target.value)}
                  rows={3}
                  placeholder="Trade with discipline."
                />
              </Field>
              <button
                type="button"
                onClick={props.onCreateAndAttach}
                disabled={
                  !props.newAgentName.trim() ||
                  !props.newAgentProvider ||
                  !props.newAgentModel ||
                  props.createPending
                }
                className="px-3 py-1.5 rounded text-[12px] border border-border text-text disabled:opacity-50"
              >
                {props.createPending ? "Creating..." : "Create and attach agent"}
              </button>
              {props.createError ? (
                <div className="text-[12px] text-danger">
                  {errorMessage(props.createError)}
                </div>
              ) : null}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

export type AttachedAgentRowProps = {
  strategyId: string;
  agentRef: AgentRef;
  index: number;
  agent: Agent | undefined;
  onRenameRole: () => void;
  onRemove: () => void;
  /// All AgentRefs on the strategy — needed by `FiringSection` so it
  /// can resolve the upstream Filter ref for any incoming gating
  /// edge. Pass-through prop: parents that don't render the firing
  /// section can leave it undefined.
  allRefs?: AgentRef[];
  /// Current pipeline. Same rationale as `allRefs`.
  pipeline?: PipelineDef;
  /// Workspace + strategy-scoped Filter-capable agents the inline
  /// composer can pick from.
  filterCandidates?: Agent[];
  /// Available providers for the inline author-new-agent flow.
  providers?: import("@/api/types.gen/ProviderRow").ProviderRow[];
  /// Strategy mutated — parent should invalidate strategy + agents
  /// queries. Called after a filter add/remove succeeds.
  onFiringChanged?: () => void;
};

export function AttachedAgentRow({
  strategyId,
  agentRef,
  index,
  agent,
  onRenameRole,
  onRemove,
}: AttachedAgentRowProps) {
  const storageKey = agentCollapseKey(strategyId, agentRef.role);
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    return safeStorageGet(storageKey) === "1";
  });

  // The React key for each row is `${agent_id}:${role}` — stable across
  // strategies that attach the same agent under the same role. When the
  // user navigates between such strategies, the same component instance
  // is reused, so we must resync `collapsed` from the new strategy-scoped
  // storage key rather than relying on the lazy `useState` initializer.
  useEffect(() => {
    setCollapsed(safeStorageGet(storageKey) === "1");
  }, [storageKey]);
  const primarySlot = agent?.slots[0];
  const modelLabel = primarySlot
    ? `${primarySlot.provider} / ${primarySlot.model}`
    : null;

  function toggleCollapsed() {
    setCollapsed((prev) => {
      const next = !prev;
      safeStorageSet(storageKey, next ? "1" : "0");
      return next;
    });
  }

  return (
    <div
      data-testid={`attached-agent-row-${agentRef.role}`}
      className="border border-border-soft rounded"
    >
      <div className="flex items-center justify-between gap-2 px-3 py-2">
        <div className="flex items-center gap-3 text-[13px] min-w-0">
          <button
            type="button"
            aria-label={collapsed ? "Expand agent" : "Collapse agent"}
            aria-expanded={!collapsed}
            onClick={toggleCollapsed}
            className="inline-flex items-center justify-center w-5 h-5 rounded text-text-2 hover:text-text border border-transparent hover:border-border-soft text-[11px]"
          >
            {collapsed ? "▶" : "▼"}
          </button>
          <span className="inline-flex items-center justify-center w-6 h-6 rounded-full border border-border-soft text-[12px] text-text-2 font-mono shrink-0">
            {index}
          </span>
          <div className="min-w-0">
            <div className="truncate">
              <span className="font-mono text-text">
                {agent ? agent.name : agentRef.agent_id}
              </span>
              {modelLabel ? (
                <span className="ml-2 inline-flex items-center rounded-sm border border-gold/30 bg-gold/[0.1] px-1.5 py-0.5 align-middle font-mono text-[11px] leading-none text-gold">
                  {modelLabel}
                </span>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      {!collapsed ? (
        // Inline detail panel — replaces the previous fullscreen "Open in
        // window" dialog (qa-strategy-popup-to-accordion, 2026-05-17).
        // Per dashboard no-popups rule (CLAUDE.md adopted 2026-05-17),
        // detail expansion happens in place, not as an overlay.
        <div className="border-t border-border-soft px-3 py-3 space-y-3 text-[12px] text-text-2">
          <div>
            <div className="text-[11px] uppercase tracking-wide text-text-3">
              Agent id
            </div>
            <div className="break-all font-mono text-text-2">{agentRef.agent_id}</div>
          </div>
          {modelLabel ? (
            <div>
              <div className="text-[11px] uppercase tracking-wide text-text-3">
                Model
              </div>
              <div className="mt-1">
                <span className="inline-flex items-center rounded-sm border border-gold/30 bg-gold/[0.1] px-2 py-0.5 font-mono text-[12px] leading-none text-gold">
                  {modelLabel}
                </span>
              </div>
            </div>
          ) : null}
          {primarySlot?.system_prompt ? (
            <div>
              <div className="text-[11px] uppercase tracking-wide text-text-3">
                System prompt
              </div>
              <pre className="whitespace-pre-wrap font-mono text-[12px] text-text-2 bg-surface-elev border border-border-soft rounded p-2 max-h-[40vh] overflow-y-auto">
                {primarySlot.system_prompt}
              </pre>
            </div>
          ) : null}
          {/* Per-agent filter composer deferred — see docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md L4. Filter is now per-strategy via FilterCard. */}
          {null}
          <div className="flex items-center gap-3 pt-1">
            <button
              type="button"
              className="text-[12px] text-text-2 hover:text-text"
              onClick={onRenameRole}
            >
              Rename role
            </button>
            <Link
              className="text-[12px] text-text-2 hover:text-text"
              to={`/agents/${encodeURIComponent(agentRef.agent_id)}`}
            >
              Edit agent
            </Link>
            <button
              type="button"
              className="text-[12px] text-danger"
              onClick={onRemove}
            >
              Remove
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function EntryRulesEditor({
  rules,
  onChange,
}: {
  rules: EntryRule[];
  onChange: (r: EntryRule[]) => void;
}) {
  function addRule() {
    onChange([...rules, { signal_name: "", direction: "long" }]);
  }
  function removeRule(i: number) {
    onChange(rules.filter((_, idx) => idx !== i));
  }
  function updateRule(i: number, patch: Partial<EntryRule>) {
    onChange(rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  }
  return (
    <div className="space-y-2">
      <div className="text-[12px] text-text-2 font-medium">Entry rules</div>
      {rules.length === 0 && (
        <p className="text-[12px] text-text-3">No entry rules configured.</p>
      )}
      {rules.map((rule, i) => (
        <div
          key={i}
          className="flex items-center gap-2 border border-border-soft rounded px-3 py-2"
        >
          <input
            className="flex-1 bg-surface-elev border border-border rounded px-2 py-1 text-[12px] text-text font-mono"
            value={rule.signal_name}
            onChange={(e) => updateRule(i, { signal_name: e.target.value })}
            placeholder="signal_name"
            aria-label={`Entry rule ${i + 1} signal name`}
          />
          <select
            className="bg-surface-elev border border-border rounded px-2 py-1 text-[12px] text-text font-mono"
            value={rule.direction}
            onChange={(e) =>
              updateRule(i, { direction: e.target.value as EntryDirection })
            }
            aria-label={`Entry rule ${i + 1} direction`}
          >
            <option value="long">Long</option>
            <option value="short">Short</option>
          </select>
          <button
            type="button"
            onClick={() => removeRule(i)}
            aria-label={`Remove entry rule ${i + 1}`}
            className="text-[12px] text-danger hover:opacity-70"
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={addRule}
        className="px-3 py-1 rounded text-[12px] border border-border text-text-2 hover:border-text-3"
      >
        + Add rule
      </button>
    </div>
  );
}

function policyValue(p: ClosePolicy): number {
  if (p.kind === "time_exit") return p.bars;
  if (p.kind === "target_pnl") return p.usd;
  return p.pct;
}

const POLICY_DEFAULTS: Record<ClosePolicy["kind"], ClosePolicy> = {
  stop_loss: { kind: "stop_loss", pct: 2.0 },
  take_profit: { kind: "take_profit", pct: 5.0 },
  trailing_stop: { kind: "trailing_stop", pct: 1.5 },
  time_exit: { kind: "time_exit", bars: 20 },
  target_pnl: { kind: "target_pnl", usd: 100 },
};

function policyWithValue(p: ClosePolicy, n: number): ClosePolicy {
  if (p.kind === "time_exit") return { kind: "time_exit", bars: Math.round(n) };
  if (p.kind === "target_pnl") return { kind: "target_pnl", usd: n };
  if (p.kind === "stop_loss") return { kind: "stop_loss", pct: n };
  if (p.kind === "take_profit") return { kind: "take_profit", pct: n };
  return { kind: "trailing_stop", pct: n };
}

function ClosePoliciesEditor({
  policies,
  onChange,
}: {
  policies: ClosePolicy[];
  onChange: (p: ClosePolicy[]) => void;
}) {
  function addPolicy() {
    onChange([...policies, { kind: "stop_loss", pct: 2.0 }]);
  }
  function removePolicy(i: number) {
    onChange(policies.filter((_, idx) => idx !== i));
  }
  function updateKind(i: number, kind: ClosePolicy["kind"]) {
    onChange(policies.map((p, idx) => (idx === i ? POLICY_DEFAULTS[kind] : p)));
  }
  function updateValue(i: number, raw: string) {
    const p = policies[i];
    if (!p) return;
    const n = Number(raw);
    if (!Number.isFinite(n) || n < 0) return;
    onChange(policies.map((x, idx) => (idx === i ? policyWithValue(p, n) : x)));
  }
  return (
    <div className="space-y-2">
      <div className="text-[12px] text-text-2 font-medium">Close policies</div>
      {policies.length === 0 && (
        <p className="text-[12px] text-text-3">No close policies configured.</p>
      )}
      {policies.map((p, i) => (
        <div
          key={i}
          className="flex items-center gap-2 border border-border-soft rounded px-3 py-2"
        >
          <select
            className="bg-surface-elev border border-border rounded px-2 py-1 text-[12px] text-text font-mono"
            value={p.kind}
            onChange={(e) => updateKind(i, e.target.value as ClosePolicy["kind"])}
            aria-label={`Close policy ${i + 1} kind`}
          >
            <option value="stop_loss">Stop Loss (%)</option>
            <option value="take_profit">Take Profit (%)</option>
            <option value="trailing_stop">Trailing Stop (%)</option>
            <option value="time_exit">Time Exit (bars)</option>
            <option value="target_pnl">Target PnL ($)</option>
          </select>
          <input
            type="number"
            className="w-24 bg-surface-elev border border-border rounded px-2 py-1 text-[12px] text-text font-mono"
            value={policyValue(p)}
            onChange={(e) => updateValue(i, e.target.value)}
            min={0}
            step={p.kind === "time_exit" ? 1 : 0.1}
            aria-label={`Close policy ${i + 1} value`}
          />
          <button
            type="button"
            onClick={() => removePolicy(i)}
            aria-label={`Remove close policy ${i + 1}`}
            className="text-[12px] text-danger hover:opacity-70"
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={addPolicy}
        className="px-3 py-1 rounded text-[12px] border border-border text-text-2 hover:border-text-3"
      >
        + Add policy
      </button>
    </div>
  );
}

function MechanisticConfigCard({ strategy }: { strategy: Strategy }) {
  const qc = useQueryClient();
  const initial = strategy.mechanistic_config ?? {
    entry_rules: [],
    close_policies: [],
  };
  const [rules, setRules] = useState<EntryRule[]>(initial.entry_rules);
  const [policies, setPolicies] = useState<ClosePolicy[]>(
    initial.close_policies,
  );
  const [savedFlash, setSavedFlash] = useState(false);

  useEffect(() => {
    const cfg = strategy.mechanistic_config ?? {
      entry_rules: [],
      close_policies: [],
    };
    setRules(cfg.entry_rules);
    setPolicies(cfg.close_policies);
  }, [strategy.mechanistic_config]);

  const save = useMutation({
    mutationFn: () =>
      setMechanisticConfig(strategy.manifest.id, {
        decision_mode: "mechanistic",
        mechanistic_config: { entry_rules: rules, close_policies: policies },
      }),
    onSuccess: (updated) => {
      setSavedFlash(true);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.setQueryData(strategyKeys.detail(strategy.manifest.id), updated);
      qc.invalidateQueries({ queryKey: strategyKeys.validate(strategy.manifest.id) });
    },
  });

  return (
    <Card id="strategy-mechanistic">
      <SectionHeader
        label="Mechanistic config"
        hint="Deterministic entry rules and close policies — no LLM agents required."
      />
      <div className="px-5 pt-4 pb-5 space-y-5">
        <EntryRulesEditor rules={rules} onChange={setRules} />
        <ClosePoliciesEditor policies={policies} onChange={setPolicies} />
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => save.mutate()}
            disabled={save.isPending}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 transition-colors motion-safe:active:scale-[0.96]"
          >
            {save.isPending ? "Saving..." : "Save config"}
          </button>
          {savedFlash ? (
            <span className="text-[12px] text-success">Saved.</span>
          ) : save.isError ? (
            <span className="text-[12px] text-danger">
              {errorMessage(save.error)}
            </span>
          ) : null}
        </div>
      </div>
    </Card>
  );
}

function ManifestCard({ strategy }: { strategy: Strategy }) {
  const qc = useQueryClient();
  const m = strategy.manifest;
  const [displayName, setDisplayName] = useState(m.display_name);
  const [plainSummary, setPlainSummary] = useState(m.plain_summary);
  // Asset universe is held as the parsed pair array so the multi-asset chip
  // editor can add/remove pairs; it is comma-joined only at render/compare.
  const [assetUniverse, setAssetUniverse] = useState<string[]>(m.asset_universe);
  const [timeframeMinutes, setTimeframeMinutes] = useState(m.decision_cadence_minutes);
  const [savedFlash, setSavedFlash] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  // Fetch the live asset list from /api/assets (Alpaca-only for the backtest
  // chip editor). Falls back gracefully to an empty array while loading.
  const alpacaAssets = useAlpacaAssets();

  useEffect(() => {
    setDisplayName(m.display_name);
    setPlainSummary(m.plain_summary);
    setAssetUniverse(m.asset_universe);
    setTimeframeMinutes(m.decision_cadence_minutes);
    setLocalError(null);
  }, [m.display_name, m.plain_summary, m.asset_universe, m.decision_cadence_minutes]);

  function removeAsset(venue: string) {
    setAssetUniverse((prev) => prev.filter((a) => a !== venue));
  }

  function addAsset(ticker: string) {
    const pair = toVenuePair(ticker);
    setAssetUniverse((prev) => (prev.includes(pair) ? prev : [...prev, pair]));
  }

  // Build addable tickers from the live API list (Alpaca assets only).
  // Excludes any ticker whose venue pair is already in assetUniverse.
  const addableTickers = alpacaAssets.data
    .map((a) => a.symbol)
    .filter((t) => !assetUniverse.includes(toVenuePair(t)));

  const patch = useMutation({
    mutationFn: () => {
      if (!Number.isInteger(timeframeMinutes) || timeframeMinutes <= 0) {
        throw new Error("Time frame must be a positive whole number of minutes.");
      }
      if (assetUniverse.length === 0) {
        throw new Error("Assets must include at least one SYMBOL/QUOTE pair.");
      }
      return patchStrategyMetadata(m.id, {
        display_name: displayName,
        plain_summary: plainSummary,
        asset_universe: assetUniverse,
        decision_cadence_minutes: timeframeMinutes,
      });
    },
    onSuccess: (updated) => {
      setSavedFlash(true);
      setLocalError(null);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.setQueryData(strategyKeys.detail(m.id), updated);
      qc.invalidateQueries({ queryKey: strategyKeys.validate(m.id) });
    },
    onError: (err) => {
      setLocalError(errorMessage(err));
    },
  });

  const sameAssets =
    assetUniverse.length === m.asset_universe.length &&
    assetUniverse.every((a, i) => a === m.asset_universe[i]);
  const dirty =
    displayName !== m.display_name ||
    plainSummary !== m.plain_summary ||
    !sameAssets ||
    timeframeMinutes !== m.decision_cadence_minutes;

  return (
    <Card>
      <SectionHeader
        label="Manifest"
        hint="Editable strategy identity and run defaults. The strategy ID stays stable for eval history."
      />
      <div className="px-5 pt-4 pb-5 space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Field label="Display name">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
            />
          </Field>
          <Field label="Time frame">
            <TimeframeSelect
              valueMinutes={timeframeMinutes}
              onChange={setTimeframeMinutes}
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            />
          </Field>
          <Field
            label="Strategy ID"
            hint="Stable identifier used by eval runs, traces, and API links."
          >
            <input
              className="w-full bg-surface-elev border border-border-soft rounded px-3 py-2 text-[13px] text-text-2 font-mono"
              value={m.id}
              readOnly
              aria-label={`Strategy ID ${m.id}`}
              onFocus={(e) => e.currentTarget.select()}
            />
          </Field>
        </div>
        {/* Assets editor: removable green chips + add buttons. */}
        <Field
          label="Assets"
          hint="The SYMBOL/QUOTE pairs this strategy may trade. Add or remove below; saved with the manifest."
        >
          <div className="space-y-2">
            {/* Removable green chips for currently selected assets */}
            <div className="flex flex-wrap gap-1.5">
              {assetUniverse.length > 0 ? (
                assetUniverse.map((a) => (
                  <span
                    key={a}
                    className="inline-flex items-center gap-1 px-2 py-0.5 bg-gold/10 border border-gold/30 text-gold rounded-full text-[12px] font-mono"
                  >
                    {a}
                    <button
                      type="button"
                      aria-label={`Remove ${a}`}
                      onClick={() => removeAsset(a)}
                      disabled={patch.isPending}
                      className="ml-0.5 text-gold/70 hover:text-danger disabled:opacity-40 leading-none"
                    >
                      ×
                    </button>
                  </span>
                ))
              ) : (
                <span className="text-text-3 text-[12px]">(none)</span>
              )}
            </div>

            {/* Inline add-asset buttons for unselected tickers */}
            {addableTickers.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {addableTickers.map((t) => (
                  <button
                    key={t}
                    type="button"
                    onClick={() => addAsset(t)}
                    disabled={patch.isPending}
                    className="inline-flex items-center px-2 py-0.5 rounded-full border border-border-soft text-[11px] font-mono text-text-2 hover:border-gold/30 hover:bg-gold/10 hover:text-gold disabled:opacity-40 transition-colors"
                  >
                    + {t}
                  </button>
                ))}
              </div>
            )}
          </div>
        </Field>
        <Field label="Description">
          <textarea
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text leading-relaxed"
            value={plainSummary}
            onChange={(e) => setPlainSummary(e.target.value)}
            rows={3}
          />
        </Field>
        <dl className="grid grid-cols-[120px_1fr] gap-y-2 text-[13px]">
          <DT>Template</DT>
          <DD className="font-mono text-text-2">{m.template}</DD>
          <DT>Creator</DT>
          <DD className="font-mono text-text-2">{m.creator}</DD>
          <DT>Risk basis</DT>
          <DD>{m.risk_preset_or_config}</DD>
        </dl>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => {
              setLocalError(null);
              patch.mutate();
            }}
            disabled={!dirty || patch.isPending}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors motion-safe:active:scale-[0.96]"
          >
            {patch.isPending ? "Saving..." : "Save manifest"}
          </button>
          {savedFlash ? (
            <span className="text-[12px] text-success">Saved.</span>
          ) : localError ? (
            <span className="text-[12px] text-danger">{localError}</span>
          ) : null}
        </div>
      </div>
    </Card>
  );
}

function RiskCard({ strategy }: { strategy: Strategy }) {
  const qc = useQueryClient();
  const [form, setForm] = useState(() => riskFormFromConfig(strategy.risk));
  const [savedFlash, setSavedFlash] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    setForm(riskFormFromConfig(strategy.risk));
    setLocalError(null);
  }, [strategy.risk]);

  const apply = useMutation({
    mutationFn: (explicit: RiskConfig) =>
      setRiskConfig(strategy.manifest.id, { explicit }),
    onSuccess: () => {
      setSavedFlash(true);
      setLocalError(null);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({
        queryKey: strategyKeys.detail(strategy.manifest.id),
      });
    },
  });

  const r = strategy.risk;
  const currentBasis = strategy.manifest.risk_preset_or_config;
  const dirty =
    form.risk_pct_per_trade !== (r.risk_pct_per_trade * 100).toFixed(2) ||
    form.max_concurrent_positions !== String(r.max_concurrent_positions) ||
    form.max_leverage !== String(r.max_leverage) ||
    form.stop_loss_atr_multiple !== String(r.stop_loss_atr_multiple) ||
    form.daily_loss_kill_pct !== (r.daily_loss_kill_pct * 100).toFixed(2);

  function onChange<K extends keyof RiskFormState>(key: K, value: string) {
    setForm((prev) => ({ ...prev, [key]: value }));
    setLocalError(null);
  }

  function onSave() {
    const riskPerTrade = Number(form.risk_pct_per_trade);
    const maxConcurrentPositions = Number(form.max_concurrent_positions);
    const maxLeverage = Number(form.max_leverage);
    const stopLossAtr = Number(form.stop_loss_atr_multiple);
    const dailyLossKill = Number(form.daily_loss_kill_pct);

    if (!Number.isFinite(riskPerTrade) || riskPerTrade <= 0) {
      setLocalError("Risk per trade must be > 0");
      return;
    }
    if (
      !Number.isFinite(maxConcurrentPositions) ||
      maxConcurrentPositions < 1
    ) {
      setLocalError("Max concurrent positions must be at least 1");
      return;
    }
    if (!Number.isFinite(maxLeverage) || maxLeverage <= 0) {
      setLocalError("Max leverage must be > 0");
      return;
    }
    if (!Number.isFinite(stopLossAtr) || stopLossAtr <= 0) {
      setLocalError("Stop-loss ATR multiple must be > 0");
      return;
    }
    if (!Number.isFinite(dailyLossKill) || dailyLossKill <= 0) {
      setLocalError("Daily loss kill must be > 0");
      return;
    }

    apply.mutate({
      risk_pct_per_trade: riskPerTrade / 100,
      max_concurrent_positions: maxConcurrentPositions,
      max_leverage: maxLeverage,
      stop_loss_atr_multiple: stopLossAtr,
      daily_loss_kill_pct: dailyLossKill / 100,
    });
  }

  return (
    <Card>
      <SectionHeader label="Risk" hint={`Currently: ${currentBasis}`} />
      <div className="px-5 pt-4 pb-5 space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Field label="Risk per trade (%)">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={form.risk_pct_per_trade}
              onChange={(e) => onChange("risk_pct_per_trade", e.target.value)}
            />
          </Field>
          <Field label="Max concurrent positions">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={form.max_concurrent_positions}
              onChange={(e) =>
                onChange("max_concurrent_positions", e.target.value)
              }
            />
          </Field>
          <Field label="Max leverage">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={form.max_leverage}
              onChange={(e) => onChange("max_leverage", e.target.value)}
            />
          </Field>
          <Field label="Stop-loss ATR multiple">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={form.stop_loss_atr_multiple}
              onChange={(e) =>
                onChange("stop_loss_atr_multiple", e.target.value)
              }
            />
          </Field>
          <Field label="Daily loss kill (%)">
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={form.daily_loss_kill_pct}
              onChange={(e) => onChange("daily_loss_kill_pct", e.target.value)}
            />
          </Field>
        </div>

        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onSave}
            disabled={!dirty || apply.isPending}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors motion-safe:active:scale-[0.96]"
          >
            {apply.isPending ? "Saving…" : "Save risk"}
          </button>
          {savedFlash ? (
            <span className="text-[12px] text-success">Saved.</span>
          ) : localError ? (
            <span className="text-[12px] text-danger">{localError}</span>
          ) : apply.isError ? (
            <span className="text-[12px] text-danger">
              {errorMessage(apply.error)}
            </span>
          ) : null}
        </div>
      </div>
    </Card>
  );
}

type RiskFormState = {
  risk_pct_per_trade: string;
  max_concurrent_positions: string;
  max_leverage: string;
  stop_loss_atr_multiple: string;
  daily_loss_kill_pct: string;
};

function riskFormFromConfig(risk: RiskConfig): RiskFormState {
  return {
    risk_pct_per_trade: (risk.risk_pct_per_trade * 100).toFixed(2),
    max_concurrent_positions: String(risk.max_concurrent_positions),
    max_leverage: String(risk.max_leverage),
    stop_loss_atr_multiple: String(risk.stop_loss_atr_multiple),
    daily_loss_kill_pct: (risk.daily_loss_kill_pct * 100).toFixed(2),
  };
}

function SectionHeader({ label, hint }: { label: string; hint?: string }) {
  return (
    <header className="px-5 pt-4 pb-3 border-b border-border-soft">
      <div className="text-[12px] uppercase tracking-wide text-text-3">
        {label}
      </div>
      {hint ? (
        <div className="text-[12px] text-text-2 mt-0.5">{hint}</div>
      ) : null}
    </header>
  );
}

function InspectorActions({
  strategyId,
  strategy,
}: {
  strategyId: string;
  strategy: Strategy | null;
}) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const deleteMut = useMutation({
    mutationFn: () => deleteStrategy(strategyId),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: strategyKeys.all });
      navigate("/strategies");
    },
  });
  // QA #4 + Q1: gate the eval/go-live action when a required MODEL is
  // unsatisfied. Inspect/edit stays allowed. While this query is still
  // loading we keep the action enabled (don't flash a gate) — the gate only
  // engages once we KNOW a model is unconfigured.
  const requirementsQ = useQuery({
    queryKey: strategyKeys.requirements(strategyId),
    queryFn: () => getStrategyRequirements(strategyId),
    staleTime: 30_000,
  });
  const requirements: StrategyRequirements | undefined = requirementsQ.data;
  const modelsBlocked =
    requirements !== undefined && requirements.all_models_satisfied === false;

  function onDelete() {
    const label = strategy?.manifest.display_name || strategyId;
    if (!window.confirm(`Delete strategy "${label}"? This cannot be undone.`)) {
      return;
    }
    deleteMut.mutate();
  }

  const deleteButton = (
    <button
      type="button"
      onClick={onDelete}
      disabled={deleteMut.isPending}
      aria-label={`Delete strategy ${strategyId}`}
      className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-danger/40 text-danger hover:border-danger disabled:opacity-50"
    >
      {deleteMut.isPending ? "Deleting..." : "Delete"}
    </button>
  );

  if (!strategy) {
    return (
      <div className="flex items-center justify-end gap-3 mb-5">
        <span className="text-[12px] text-text-3">
          Checking eval readiness...
        </span>
        {deleteButton}
      </div>
    );
  }

  if (!hasAttachedAgents(strategy)) {
    return (
      <div className="flex items-center justify-end gap-3 mb-5">
        {deleteMut.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(deleteMut.error)}
          </span>
        ) : null}
        <span className="text-[12px] text-danger">
          No strategy agent is attached yet.
        </span>
        <a
          href="#strategy-agents"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Go to agents
        </a>
        {deleteButton}
      </div>
    );
  }

  if (modelsBlocked) {
    return (
      <div className="flex flex-wrap items-center justify-end gap-3 mb-5">
        {deleteMut.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(deleteMut.error)}
          </span>
        ) : null}
        <span className="text-[12px] text-amber-700 dark:text-amber-300">
          Configure the required model(s) before running.
        </span>
        <Link
          to="/settings/providers"
          className="inline-flex items-center gap-1 rounded border border-border px-3.5 py-2 text-[13px] font-medium text-text hover:border-text-3"
        >
          Configure
        </Link>
        {deleteButton}
        <button
          type="button"
          disabled
          aria-disabled="true"
          title="Configure the required model(s) before running"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold/40 text-bg cursor-not-allowed"
        >
          Run eval →
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-end gap-3 mb-5">
      {deleteMut.isError ? (
        <span className="text-[12px] text-danger">
          {errorMessage(deleteMut.error)}
        </span>
      ) : null}
      {deleteButton}
      <Link
        to={`/eval-runs?strategy=${encodeURIComponent(strategyId)}&start=1`}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors motion-safe:active:scale-[0.96]"
      >
        Run eval →
      </Link>
    </div>
  );
}

function hasAttachedAgents(strategy: Strategy | null): boolean {
  if (strategy?.decision_mode === "mechanistic") return true;
  return (strategy?.agents ?? []).length > 0;
}

function agentSupportsFilter(agent: Agent): boolean {
  return agent.slots.some((slot) => slot.allowed_tools?.includes("indicator_panel"));
}

function isReservedAgentRole(role: string): boolean {
  return role.trim().toLowerCase() === "filter";
}

function nameToRole(name: string): string {
  return (
    name
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "") || "agent"
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="text-[12px] text-text-2 mb-1 block">{label}</span>
      {children}
      {hint ? (
        <span className="text-[11px] text-text-3 mt-1 block">{hint}</span>
      ) : null}
    </label>
  );
}

function DT({ children }: { children: React.ReactNode }) {
  return <dt className="text-text-3">{children}</dt>;
}

function DD({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <dd className={`m-0 min-w-0 break-words text-text ${className ?? ""}`}>
      {children}
    </dd>
  );
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-48 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-32 rounded bg-surface-elev animate-pulse" />
        </div>
      ))}
    </div>
  );
}

function ErrorState({ err, onRetry }: { err: unknown; onRetry: () => void }) {
  if (err instanceof ApiError && err.code === "not_found") {
    return (
      <div className="px-6 py-12 text-center">
        <div className="font-sans font-semibold text-[24px] text-text-3 mb-3">
          draft not found
        </div>
        <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
          This draft id doesn't exist on the engine.
        </p>
        <Link
          to="/strategies"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Back to strategies
        </Link>
      </div>
    );
  }
  return (
    <div className="px-6 py-12 text-center">
      <div className="font-sans font-semibold text-[24px] text-danger mb-3">
        couldn't load draft
      </div>
      <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
        <code className="break-all font-mono text-[12px] text-danger">
          {errorMessage(err)}
        </code>
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
      >
        Retry
      </button>
    </div>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
