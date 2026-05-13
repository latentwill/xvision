import { useEffect, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  addStrategyAgent,
  getStrategy,
  renameStrategyAgentRole,
  removeStrategyAgent,
  setRiskConfig,
  setStrategyPipeline,
  strategyKeys,
  validateDraft,
  type PipelineKind,
  type RiskConfig,
  type Strategy,
  type ValidateDraftOut,
} from "@/api/strategies";
import { listAgents } from "@/api/agents";
import { getStrategyChart, strategyChartKeys } from "@/api/chart";
import { StrategyChart } from "@/components/chart/StrategyChart";

export function AuthoringRoute() {
  const params = useParams<{ id?: string }>();

  if (!params.id) {
    return <Navigate to="/strategies" replace />;
  }

  return <InspectorPage id={params.id} />;
}

function InspectorPage({ id }: { id: string }) {
  const qc = useQueryClient();
  const bundleQ = useQuery({
    queryKey: strategyKeys.detail(id),
    queryFn: () => getStrategy(id),
  });
  const validateQ = useQuery({
    queryKey: strategyKeys.validate(id),
    queryFn: () => validateDraft(id),
    enabled: bundleQ.isSuccess,
  });

  // Re-validate after the bundle changes (e.g. after a slot save).
  useEffect(() => {
    if (bundleQ.dataUpdatedAt > 0) {
      qc.invalidateQueries({ queryKey: strategyKeys.validate(id) });
    }
  }, [bundleQ.dataUpdatedAt, id, qc]);

  return (
    <>
      <Topbar
        title="Inspector"
        sub={
          bundleQ.data
            ? `${bundleQ.data.manifest.display_name} · ${id}`
            : id
        }
      />

      <InspectorActions strategyId={id} />

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-5">
        <div className="space-y-5">
          {bundleQ.isPending ? (
            <Card>
              <LoadingSkeleton />
            </Card>
          ) : bundleQ.isError ? (
            <Card>
              <ErrorState
                err={bundleQ.error}
                onRetry={() => bundleQ.refetch()}
              />
            </Card>
          ) : bundleQ.data ? (
            <BundleEditor bundle={bundleQ.data} />
          ) : null}
          <PerformanceHistoryCard strategyId={id} />
        </div>

        <aside className="space-y-5">
          <ValidationCard query={validateQ} />
          <RunEvalCard agentId={id} />
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
      <div className="px-5 pb-5">
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

function BundleEditor({ bundle }: { bundle: Strategy }) {
  return (
    <>
      <ManifestCard bundle={bundle} />
      <AgentsCard bundle={bundle} />
      <RiskCard bundle={bundle} />
      <MechanicalParamsCard bundle={bundle} />
    </>
  );
}

function AgentsCard({ bundle }: { bundle: Strategy }) {
  const qc = useQueryClient();
  const agentPool = useQuery({
    queryKey: ["agents", "pool"],
    queryFn: () => listAgents({ include_archived: false, limit: 200 }),
  });
  const [newAgentId, setNewAgentId] = useState("");
  const [newRole, setNewRole] = useState("");
  const [renameRoleFrom, setRenameRoleFrom] = useState<string | null>(null);
  const [renameRoleTo, setRenameRoleTo] = useState("");

  const attached = bundle.agents ?? [];
  const pipeline = bundle.pipeline ?? { kind: "single" as const, edges: [] };
  const available = (agentPool.data ?? []).filter(
    (a) => !attached.some((r) => r.agent_id === a.agent_id),
  );
  const graphEdges = pipeline.edges ?? [];

  function invalidateStrategy() {
    qc.invalidateQueries({ queryKey: strategyKeys.detail(bundle.manifest.id) });
    qc.invalidateQueries({
      queryKey: strategyKeys.validate(bundle.manifest.id),
    });
  }

  const addMut = useMutation({
    mutationFn: (payload: { agent_id: string; role: string }) =>
      addStrategyAgent(bundle.manifest.id, payload),
    onSuccess: () => {
      setNewAgentId("");
      setNewRole("");
      invalidateStrategy();
    },
  });

  const removeMut = useMutation({
    mutationFn: (role: string) => removeStrategyAgent(bundle.manifest.id, role),
    onSuccess: () => {
      invalidateStrategy();
    },
  });

  const renameMut = useMutation({
    mutationFn: (payload: { role: string; newRole: string }) =>
      renameStrategyAgentRole(bundle.manifest.id, payload.role, payload.newRole),
    onSuccess: () => {
      invalidateStrategy();
    },
  });

  const pipelineMut = useMutation({
    mutationFn: (kind: PipelineKind) =>
      setStrategyPipeline(bundle.manifest.id, { kind, edges: [] }),
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
    <Card>
      <SectionHeader
        label="Strategy agents"
        hint="Attach reusable AgentRefs and define the pipeline that executes them."
      />
      <div className="px-5 pb-5 space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-[220px_1fr] gap-3 items-start border border-border-soft rounded p-3">
          <Field
            label="Pipeline kind"
            hint={
              pipeline.kind === "graph"
                ? "Graph bundles are view-only here; graph runtime intentionally errors today."
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
            {attached.map((a) => (
              <div
                key={`${a.agent_id}:${a.role}`}
                className="border border-border-soft rounded p-3"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-3 text-[13px]">
                    <span className="inline-flex items-center justify-center w-6 h-6 rounded-full border border-border-soft text-[12px] text-text-2 font-mono">
                      {attached.indexOf(a) + 1}
                    </span>
                    <div>
                      <span className="text-text font-mono">{a.role}</span>
                      <span className="text-text-3"> · </span>
                      <span className="text-text-2 font-mono">
                        {a.agent_id}
                      </span>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      className="text-[12px] text-text-2 hover:text-text"
                      onClick={() => {
                        setRenameRoleFrom(a.role);
                        setRenameRoleTo(a.role);
                      }}
                    >
                      Rename role
                    </button>
                    <Link
                      className="text-[12px] text-text-2 hover:text-text"
                      to={`/agents/${encodeURIComponent(a.agent_id)}`}
                    >
                      Edit agent
                    </Link>
                    <button
                      className="text-[12px] text-danger"
                      onClick={() => removeMut.mutate(a.role)}
                    >
                      Remove
                    </button>
                  </div>
                </div>
              </div>
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
              Renaming role <code>{renameRoleFrom}</code>
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

        <div className="border border-border-soft rounded p-3 space-y-2">
          <div className="text-[12px] text-text-2">Add agent</div>
          <select
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
            value={newAgentId}
            onChange={(e) => setNewAgentId(e.target.value)}
          >
            <option value="">Select agent…</option>
            {available.map((a) => (
              <option key={a.agent_id} value={a.agent_id}>
                {a.name} · {a.agent_id}
              </option>
            ))}
          </select>
          <input
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            value={newRole}
            onChange={(e) => setNewRole(e.target.value)}
            placeholder="Role name (e.g. trader)"
          />
          <button
            onClick={() =>
              addMut.mutate({
                agent_id: newAgentId,
                role: newRole.trim(),
              })
            }
            disabled={!newAgentId || !newRole.trim() || addMut.isPending}
            className="px-3 py-1.5 rounded text-[12px] border border-border disabled:opacity-50"
          >
            Add Agent
          </button>
        </div>
      </div>
    </Card>
  );
}

function ManifestCard({ bundle }: { bundle: Strategy }) {
  const m = bundle.manifest;
  return (
    <Card>
      <SectionHeader label="Manifest" hint="Read-only metadata for v1." />
      <dl className="grid grid-cols-[160px_1fr] gap-y-2 px-5 pb-4 text-[13px]">
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

function RiskCard({ bundle }: { bundle: Strategy }) {
  const qc = useQueryClient();
  const [form, setForm] = useState(() => riskFormFromConfig(bundle.risk));
  const [savedFlash, setSavedFlash] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    setForm(riskFormFromConfig(bundle.risk));
    setLocalError(null);
  }, [bundle.risk]);

  const apply = useMutation({
    mutationFn: (explicit: RiskConfig) =>
      setRiskConfig(bundle.manifest.id, { explicit }),
    onSuccess: () => {
      setSavedFlash(true);
      setLocalError(null);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({
        queryKey: strategyKeys.detail(bundle.manifest.id),
      });
    },
  });

  const r = bundle.risk;
  const currentBasis = bundle.manifest.risk_preset_or_config;
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
      <div className="px-5 pb-5 space-y-4">
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

function MechanicalParamsCard({ bundle }: { bundle: Strategy }) {
  const json = JSON.stringify(bundle.mechanical_params, null, 2);
  const empty =
    bundle.mechanical_params == null ||
    (typeof bundle.mechanical_params === "object" &&
      Object.keys(bundle.mechanical_params as object).length === 0);

  return (
    <Card>
      <SectionHeader
        label="Mechanical params"
        hint="Read-only in v1; per-field editor lands with the LLM split editor."
      />
      <div className="px-5 pb-5">
        {empty ? (
          <p className="m-0 text-[13px] text-text-3">
            No mechanical params on this template.
          </p>
        ) : (
          <pre className="m-0 p-3 bg-surface-elev border border-border-soft rounded text-[12px] text-text-2 overflow-x-auto font-mono">
            {json}
          </pre>
        )}
      </div>
    </Card>
  );
}

function ValidationCard({
  query,
}: {
  query: ReturnType<typeof useQuery<ValidateDraftOut>>;
}) {
  return (
    <Card>
      <SectionHeader label="Validation" />
      <div className="px-5 pb-5 text-[13px]">
        {query.isPending ? (
          <p className="m-0 text-text-3">Validating…</p>
        ) : query.isError ? (
          <p className="m-0 text-danger">{errorMessage(query.error)}</p>
        ) : query.data ? (
          query.data.ok ? (
            <Pill tone="gold">
              <span className="w-1.5 h-1.5 rounded-full bg-gold" /> valid
            </Pill>
          ) : (
            <div className="space-y-2">
              <Pill tone="danger">
                <span className="w-1.5 h-1.5 rounded-full bg-danger" />
                {" "}
                invalid
              </Pill>
              <ul className="m-0 pl-4 list-disc text-text-2 space-y-1">
                {query.data.errors.map((err, i) => (
                  <li key={i}>{err}</li>
                ))}
              </ul>
            </div>
          )
        ) : null}
        <div className="mt-4">
          <button
            onClick={() => query.refetch()}
            disabled={query.isFetching}
            className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-50 transition-colors"
          >
            {query.isFetching ? "Re-validating…" : "Re-validate"}
          </button>
        </div>
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

function RunEvalCard({ agentId }: { agentId: string }) {
  // v1 launches eval runs via CLI; the dashboard surfaces results. This
  // card gives the operator a copy-pasteable command + a direct link to
  // the runs list so the loop "edit → eval → inspect" is reachable from
  // inside the Inspector instead of requiring a route hop.
  const cliCommand = `xvn eval run --strategy ${agentId} --scenario crypto-bull-q1-2025 --mode backtest`;
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(cliCommand);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // Clipboard API can fail in non-secure contexts; silently no-op.
      // The user can still triple-click to select the command text.
    }
  }

  return (
    <Card>
      <SectionHeader
        label="Run eval"
        hint="Launch via CLI; results render in /eval-runs."
      />
      <div className="px-5 py-4 space-y-3">
        <div className="relative">
          <pre className="m-0 px-3 py-2 bg-surface-elev border border-border-soft rounded text-[11.5px] font-mono text-text overflow-x-auto whitespace-pre">
{cliCommand}
          </pre>
          <button
            type="button"
            onClick={copy}
            className="absolute top-1.5 right-1.5 px-2 py-0.5 text-[11px] text-text-3 hover:text-text bg-surface-card border border-border rounded"
            title="Copy command"
          >
            {copied ? "copied" : "copy"}
          </button>
        </div>
        <p className="m-0 text-[12px] text-text-3 leading-snug">
          Swap <code className="font-mono text-text-2">crypto-bull-q1-2025</code> for any{" "}
          <code className="font-mono text-text-2">xvn eval scenarios</code> id. Use{" "}
          <code className="font-mono text-text-2">--mode paper</code> for Alpaca paper trading.
        </p>
        <Link
          to={`/eval-runs?strategy=${encodeURIComponent(agentId)}&start=1`}
          className="inline-flex items-center gap-1 text-[13px] text-text hover:text-gold"
        >
          Open launcher →
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

function InspectorActions({ strategyId }: { strategyId: string }) {
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
  return <dd className={`m-0 text-text ${className ?? ""}`}>{children}</dd>;
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
        <code className="text-danger font-mono text-[12px]">
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
