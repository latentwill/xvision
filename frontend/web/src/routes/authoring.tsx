import { useEffect, useMemo, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  addStrategyAgent,
  getStrategy,
  renameStrategyAgentRole,
  removeStrategyAgent,
  setRiskConfig,
  setStrategyPipeline,
  strategyKeys,
  type AgentRef,
  type PipelineDef,
  type PipelineKind,
  type RiskConfig,
  type Strategy,
} from "@/api/strategies";
import { createAgent, listAgents, type Agent } from "@/api/agents";
import { FiringSection } from "@/components/strategy";
import { listProviders, settingsKeys } from "@/api/settings";
import { getStrategyChart, strategyChartKeys } from "@/api/chart";
import { StrategyChart } from "@/components/chart/StrategyChart";
import { ModelPicker } from "@/components/ModelPicker";
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
        title="Inspector"
        back={{ to: "/strategies", label: "Back to strategies" }}
        sub={
          strategyQ.data ? (
            <>
              <span>{strategyQ.data.manifest.display_name}</span>
              <span className="mx-1.5 text-text-3">·</span>
              <span className="break-all font-mono text-[12px] text-text-3">
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

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-5">
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
            <StrategyEditor strategy={strategyQ.data} />
          ) : null}
          <PerformanceHistoryCard strategyId={id} />
        </div>

        <aside className="space-y-5">
          <BackLinkCard />
        </aside>
      </div>
    </>
  );
}

function PerformanceHistoryCard({ strategyId }: { strategyId: string }) {
  const chart = useQuery({
    queryKey: strategyChartKeys.strategy(strategyId),
    queryFn: () => getStrategyChart(strategyId),
  });

  return (
    <Card>
      <SectionHeader
        label="Performance history"
        hint="Equity curves from all completed eval runs, colour-coded by scenario."
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
        {chart.data && <StrategyChart payload={chart.data} />}
      </div>
    </Card>
  );
}

function StrategyEditor({ strategy }: { strategy: Strategy }) {
  return (
    <>
      <ManifestCard strategy={strategy} />
      <AgentsCard strategy={strategy} />
      <RiskCard strategy={strategy} />
      <MechanicalParamsCard strategy={strategy} />
    </>
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
  const [newRole, setNewRole] = useState("");
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentRole, setNewAgentRole] = useState("");
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
      setNewRole("");
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
            max_tokens: null,
          },
        ],
      });
      await addStrategyAgent(strategy.manifest.id, {
        agent_id: agent.agent_id,
        role: newAgentRole.trim(),
      });
      return agent;
    },
    onSuccess: async () => {
      setNewAgentName("");
      setNewAgentRole("");
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
                : "Single requires one agent. Sequential runs refs in the order below."
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
                single
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
                ? "Attach at least one AgentRef before running this strategy."
                : pipeline.kind === "single"
                  ? "The first AgentRef is the only executable stage."
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
            No agents attached yet.
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
          newRole={newRole}
          setNewRole={setNewRole}
          newAgentName={newAgentName}
          setNewAgentName={setNewAgentName}
          newAgentRole={newAgentRole}
          setNewAgentRole={setNewAgentRole}
          newAgentProvider={newAgentProvider}
          setNewAgentProvider={setNewAgentProvider}
          newAgentModel={newAgentModel}
          setNewAgentModel={setNewAgentModel}
          newAgentPrompt={newAgentPrompt}
          setNewAgentPrompt={setNewAgentPrompt}
          onAttachExisting={() =>
            addMut.mutate({
              agent_id: newAgentId,
              role: newRole.trim(),
            })
          }
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
  newRole: string;
  setNewRole: (v: string) => void;
  newAgentName: string;
  setNewAgentName: (v: string) => void;
  newAgentRole: string;
  setNewAgentRole: (v: string) => void;
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
              <Field label="Existing agent role">
                <input
                  className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
                  value={props.newRole}
                  onChange={(e) => props.setNewRole(e.target.value)}
                  placeholder="Role name (e.g. trader)"
                />
              </Field>
              <button
                type="button"
                onClick={props.onAttachExisting}
                disabled={
                  !props.newAgentId ||
                  !props.newRole.trim() ||
                  props.attachExistingPending
                }
                className="px-3 py-1.5 rounded text-[12px] border border-border disabled:opacity-50"
              >
                {props.attachExistingPending ? "Adding..." : "Add Agent"}
              </button>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Field label="New agent name">
                  <input
                    className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
                    value={props.newAgentName}
                    onChange={(e) => props.setNewAgentName(e.target.value)}
                    placeholder="DeepSeek trader"
                  />
                </Field>
                <Field label="New agent role">
                  <input
                    className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
                    value={props.newAgentRole}
                    onChange={(e) => props.setNewAgentRole(e.target.value)}
                    placeholder="trader"
                  />
                </Field>
              </div>
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
                  className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
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
                  !props.newAgentRole.trim() ||
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
  allRefs,
  pipeline,
  filterCandidates,
  providers,
  onFiringChanged,
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
              <span className="break-all font-mono text-text">
                {agentRef.role}
              </span>
              {agent ? (
                <>
                  <span className="text-text-3"> · </span>
                  <span className="text-text">{agent.name}</span>
                </>
              ) : null}
              {modelLabel ? (
                <>
                  <span className="text-text-3"> · </span>
                  <span className="font-mono text-text-2 text-[12px]">
                    {modelLabel}
                  </span>
                </>
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
              <div className="font-mono text-text-2">{modelLabel}</div>
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
          {allRefs && pipeline && filterCandidates && providers && onFiringChanged ? (
            <FiringSection
              strategyId={strategyId}
              agentRef={agentRef}
              refs={allRefs}
              pipeline={pipeline}
              filterCandidates={filterCandidates}
              providers={providers}
              onMutated={onFiringChanged}
            />
          ) : null}
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

function ManifestCard({ strategy }: { strategy: Strategy }) {
  const m = strategy.manifest;
  return (
    <Card>
      <SectionHeader
        label="Manifest"
        hint="Direct edits are locked in the Inspector. Wizard changes appear here only after a save tool succeeds."
      />
      <dl className="grid grid-cols-[160px_1fr] gap-y-2 px-5 pt-4 pb-4 text-[13px]">
        <DT>Display name</DT>
        <DD>{m.display_name}</DD>
        <DT>Template</DT>
        <DD className="font-mono text-text-2">{m.template}</DD>
        <DT>Creator</DT>
        <DD className="font-mono text-text-2">{m.creator}</DD>
        <DT>Asset universe</DT>
        <DD>
          {m.asset_universe.length > 0
            ? m.asset_universe.map((a) => (
                <span
                  key={a}
                  className="inline-block mr-1.5 px-1.5 py-0.5 bg-surface-elev border border-border-soft rounded text-[12px] font-mono"
                >
                  {a}
                </span>
              ))
            : "(none)"}
        </DD>
        <DT>Cadence</DT>
        <DD>
          every <strong>{m.decision_cadence_minutes}</strong> min
        </DD>
        <DT>Risk basis</DT>
        <DD>{m.risk_preset_or_config}</DD>
      </dl>
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
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors"
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

function MechanicalParamsCard({ strategy }: { strategy: Strategy }) {
  const json = JSON.stringify(strategy.mechanical_params, null, 2);
  const empty =
    strategy.mechanical_params == null ||
    (typeof strategy.mechanical_params === "object" &&
      Object.keys(strategy.mechanical_params as object).length === 0);

  return (
    <Card>
      <SectionHeader
        label="Mechanical params"
        hint="Inspector read-only in v1. Tune through setup tools; this panel shows the saved JSON."
      />
      <div className="px-5 pt-4 pb-5">
        {empty ? (
          <p className="m-0 text-[13px] text-text-3">
            No mechanical params on this template.
          </p>
        ) : (
          <pre className="m-0 overflow-x-auto rounded border border-border-soft bg-surface-elev p-3 font-mono text-[12px] text-text-2">
            {json}
          </pre>
        )}
      </div>
    </Card>
  );
}

function BackLinkCard() {
  return (
    <Card>
      <div className="px-5 py-4 text-[13px] text-text-2">
        <Link to="/strategies" className="text-text hover:underline">
          ← Back to strategies
        </Link>
      </div>
    </Card>
  );
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
  if (!strategy) {
    return (
      <div className="flex items-center justify-end gap-3 mb-5">
        <span className="text-[12px] text-text-3">
          Checking eval readiness...
        </span>
      </div>
    );
  }

  if (!hasAttachedAgents(strategy)) {
    return (
      <div className="flex items-center justify-end gap-3 mb-5">
        <span className="text-[12px] text-danger">
          No strategy agent is attached yet.
        </span>
        <a
          href="#strategy-agents"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Go to agents
        </a>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-end gap-3 mb-5">
      <Link
        to={`/eval-runs?strategy=${encodeURIComponent(strategyId)}&start=1`}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
      >
        Run eval →
      </Link>
    </div>
  );
}

function hasAttachedAgents(strategy: Strategy | null): boolean {
  return (strategy?.agents ?? []).length > 0;
}

function agentSupportsFilter(agent: Agent): boolean {
  return agent.slots.some((slot) => slot.capabilities?.includes("filter"));
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
        <div className="font-serif italic text-[24px] text-text-3 mb-3">
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
      <div className="font-serif italic text-[24px] text-danger mb-3">
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
