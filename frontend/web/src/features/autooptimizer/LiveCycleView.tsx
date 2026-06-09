import {
  type Dispatch,
  type RefObject,
  type SetStateAction,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useCycleEventStream, type EventRow } from "./hooks/useCycleEventStream";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card, CardHeader } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ModelPicker } from "@/components/ModelPicker";
import {
  type LineageNode,
  type StartRunCycleRequest,
  formatEventLabel,
  getCycleCost,
  getRunDefaults,
  startRunCycle,
  cancelRunCycle,
  useLineageNodes,
  useCycleRuns,
  useCycleRun,
  useCycleCost,
  type CycleRunSummary,
  autooptimizerKeys,
} from "./api";
import { LiveEvalHeatmap } from "./panels/LiveEvalHeatmap";
import {
  clearStoredJudgeModel,
  clearStoredJudgeProvider,
  clearStoredMutatorModel,
  clearStoredMutatorProvider,
  getStoredJudgeModel,
  getStoredMutatorModel,
  getStoredJudgeProvider,
  getStoredMutatorProvider,
  setStoredJudgeModel,
  setStoredMutatorModel,
  setStoredJudgeProvider,
  setStoredMutatorProvider,
} from "./preferences";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { listProviders, settingsKeys } from "@/api/settings";
import { isProviderConfigured } from "@/lib/providers";

type OptimizerModelSelection = {
  mutatorProvider: string | null;
  mutatorModel: string;
  judgeProvider: string | null;
  judgeModel: string;
};


// ─── Lineage helpers ──────────────────────────────────────────────────────────

type CycleGroup = {
  cycle_id: string | null;
  nodes: LineageNode[];
  activeCount: number;
  latestNode: LineageNode;
};

function buildCycleGroups(nodes: LineageNode[]): CycleGroup[] {
  const map = new Map<string | null, LineageNode[]>();
  for (let i = 0; i < nodes.length; i++) {
    const key = nodes[i].cycle_id ?? null;
    const arr = map.get(key);
    if (arr) arr.push(nodes[i]);
    else map.set(key, [nodes[i]]);
  }
  const groups: CycleGroup[] = [];
  for (const [cycle_id, cycleNodes] of map) {
    const sorted = [...cycleNodes].sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
    groups.push({
      cycle_id,
      nodes: sorted,
      activeCount: cycleNodes.filter((n) => n.status === "active").length,
      latestNode: sorted[0],
    });
  }
  return groups.sort(
    (a, b) =>
      new Date(b.latestNode.created_at).getTime() - new Date(a.latestNode.created_at).getTime(),
  );
}

function countKeptThisWeek(nodes: LineageNode[]): number {
  const weekAgo = Date.now() - 7 * 86_400_000;
  let count = 0;
  for (let i = 0; i < nodes.length; i++) {
    if (nodes[i].status === "active" && new Date(nodes[i].created_at).getTime() >= weekAgo) {
      count++;
    }
  }
  return count;
}

function countActiveLineages(nodes: LineageNode[]): number {
  const seen = new Set<string>();
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    if (n.status === "active" && n.cycle_id) seen.add(n.cycle_id);
  }
  return seen.size;
}

function formatRelativeDate(ts: string): string {
  try {
    const diffDays = Math.floor((Date.now() - new Date(ts).getTime()) / 86_400_000);
    if (diffDays === 0) return "today";
    if (diffDays === 1) return "yesterday";
    return `${diffDays}d ago`;
  } catch {
    return ts;
  }
}

// ─── Page header ──────────────────────────────────────────────────────────────

function LivePageHeader({
  nodes,
  isRunning,
  activeCycleId,
  onTabChange,
}: {
  nodes: LineageNode[];
  isRunning: boolean;
  activeCycleId: string | null;
  onTabChange?: (tab: string) => void;
}) {
  const keptThisWeek = countKeptThisWeek(nodes);
  const totalExperiments = nodes.length;
  const activeLineages = countActiveLineages(nodes);
  // F28: cancel an in-flight cycle (stops it before the next candidate).
  const cancelMutation = useMutation({ mutationFn: cancelRunCycle });
  const headline =
    isRunning && activeCycleId
      ? `Optimizer run in progress · ${activeCycleId}`
      : "No cycle running";
  return (
    <div className="flex items-start justify-between gap-4 pb-6 mb-2 border-b border-border">
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium">
            Optimizer
          </span>
          <Pill tone={isRunning ? "gold" : "default"} animated={isRunning}>
            {isRunning ? "Running" : "Idle"}
          </Pill>
        </div>
        <h1 className="text-2xl font-semibold tracking-[-0.025em] text-text">{headline}</h1>
        <p className="font-mono text-[11.5px] text-text-3">
          {keptThisWeek} kept this week · {totalExperiments} experiments total · {activeLineages} active lineages
        </p>
      </div>
      <div className="flex items-center gap-2 shrink-0 flex-wrap justify-end">
        {isRunning && activeCycleId && (
          <button
            type="button"
            onClick={() => cancelMutation.mutate(activeCycleId)}
            disabled={cancelMutation.isPending}
            className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/10 disabled:opacity-50 transition-colors"
          >
            {cancelMutation.isPending ? "Cancelling…" : "Cancel run"}
          </button>
        )}
        <a
          href="#optimizer-run-controls"
          className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 hover:text-text hover:border-border-strong transition-colors"
        >
          Configure run
        </a>
        <button
          type="button"
          onClick={() => onTabChange?.("genealogy")}
          className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 hover:text-text hover:border-border-strong transition-colors"
        >
          Genealogy
        </button>
        <button
          type="button"
          onClick={() => onTabChange?.("ladder")}
          className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90"
        >
          Writer ladder
        </button>
      </div>
    </div>
  );
}

// ─── Left column ──────────────────────────────────────────────────────────────

type LaunchConfig = StartRunCycleRequest & {
  maxCycles: number | null;
  totalBudgetUsd: number | null;
};

function LaunchStrip({
  modelSelection,
  onStartLoop,
  isRunning,
  loopActive,
  cyclesCompleted,
  cumulativeSpent,
  loopError,
  onStop,
}: {
  modelSelection: OptimizerModelSelection;
  onStartLoop: (config: LaunchConfig) => void;
  isRunning: boolean;
  loopActive: boolean;
  cyclesCompleted: number;
  cumulativeSpent: number;
  loopError: string | null;
  onStop: () => void;
}) {
  const [strategyId, setStrategyId] = useState("");
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [budgetUsd, setBudgetUsd] = useState("");
  const [experimentsPerCycle, setExperimentsPerCycle] = useState("");
  const [maxCycles, setMaxCycles] = useState("");
  const [totalBudgetUsd, setTotalBudgetUsd] = useState("");
  const [dayStart, setDayStart] = useState("");
  const [dayEnd, setDayEnd] = useState("");
  const [baselineStart, setBaselineStart] = useState("");
  const [baselineEnd, setBaselineEnd] = useState("");
  const { data: strategies, isPending: strategiesLoading } = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const handleLaunch = () => {
    const trimmed = strategyId.trim();
    if (!trimmed) { setLaunchError("Select a strategy"); return; }
    setLaunchError(null);
    let budget: number | null = null;
    if (budgetUsd.trim() !== "") {
      const n = Number(budgetUsd);
      if (!Number.isFinite(n) || n <= 0) { setLaunchError("Per-cycle budget must be a positive USD amount"); return; }
      budget = n;
    }
    let maxC: number | null = null;
    if (maxCycles.trim() !== "") {
      const n = parseInt(maxCycles, 10);
      if (!Number.isFinite(n) || n <= 0) { setLaunchError("Max cycles must be a positive integer"); return; }
      maxC = n;
    }
    let totalBudget: number | null = null;
    if (totalBudgetUsd.trim() !== "") {
      const n = Number(totalBudgetUsd);
      if (!Number.isFinite(n) || n <= 0) { setLaunchError("Total budget must be a positive USD amount"); return; }
      totalBudget = n;
    }
    let expPerCycle: number | null = null;
    if (experimentsPerCycle.trim() !== "") {
      const n = parseInt(experimentsPerCycle, 10);
      if (!Number.isFinite(n) || n < 1 || n > 64) { setLaunchError("Experiments per cycle must be between 1 and 64"); return; }
      expPerCycle = n;
    }
    const orNull = (s: string) => (s.trim() === "" ? null : s.trim());
    onStartLoop({
      strategy_id: trimmed,
      mutator_provider: modelSelection.mutatorProvider,
      mutator_model: modelSelection.mutatorModel || null,
      judge_provider: modelSelection.judgeProvider,
      judge_model: modelSelection.judgeModel || null,
      budget_usd: budget,
      day_start: orNull(dayStart),
      day_end: orNull(dayEnd),
      baseline_start: orNull(baselineStart),
      baseline_end: orNull(baselineEnd),
      experiments_per_cycle: expPerCycle,
      maxCycles: maxC,
      totalBudgetUsd: totalBudget,
    });
  };
  const inp = "min-h-9 bg-surface-elev border border-border rounded text-text text-[13px] px-2 py-1";
  const noStrategies = !strategiesLoading && (!strategies || strategies.length === 0);
  const disabled = isRunning || !strategyId.trim() || noStrategies;
  return (
    <div className="flex flex-col gap-2">
      <label htmlFor="optimizer-strategy" className="text-[12px] text-text-3">
        Parent strategy
      </label>
      <select
        id="optimizer-strategy"
        value={strategyId}
        onChange={(e) => setStrategyId(e.target.value)}
        disabled={isRunning || strategiesLoading || noStrategies}
        aria-label="Strategy"
        className={`${inp} w-full`}
      >
        {strategiesLoading ? (
          <option value="">Loading…</option>
        ) : noStrategies ? (
          <option value="">No strategies</option>
        ) : (
          <>
            <option value="">— pick a strategy —</option>
            {strategies!.map((s) => (
              <option key={s.agent_id} value={s.agent_id}>{s.display_name}</option>
            ))}
          </>
        )}
      </select>
      <label htmlFor="optimizer-budget" className="text-[12px] text-text-3 mt-1">
        Per-cycle budget cap (USD, optional)
      </label>
      <input
        id="optimizer-budget"
        type="number"
        min="0"
        step="0.01"
        inputMode="decimal"
        value={budgetUsd}
        onChange={(e) => setBudgetUsd(e.target.value)}
        disabled={isRunning}
        placeholder="no cap"
        aria-label="Per-cycle budget cap in USD"
        className={`${inp} w-full`}
      />
      <label htmlFor="optimizer-experiments" className="text-[12px] text-text-3 mt-1">
        Experiments per cycle (1–64)
      </label>
      <input
        id="optimizer-experiments"
        type="number"
        min="1"
        max="64"
        step="1"
        inputMode="numeric"
        value={experimentsPerCycle}
        onChange={(e) => setExperimentsPerCycle(e.target.value)}
        disabled={isRunning}
        placeholder="config default (5)"
        aria-label="Candidate experiments to generate per parent each cycle"
        className={`${inp} w-full`}
      />
        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-[12px] text-text-3">Max cycles</label>
            <input
              type="number"
              min="1"
            step="1"
            inputMode="numeric"
            value={maxCycles}
            onChange={(e) => setMaxCycles(e.target.value)}
            disabled={isRunning}
            placeholder="∞"
            aria-label="Maximum cycle count"
            className={`${inp} w-full`}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label className="text-[12px] text-text-3">Total budget (USD)</label>
          <input
            type="number"
            min="0"
            step="0.01"
            inputMode="decimal"
            value={totalBudgetUsd}
            onChange={(e) => setTotalBudgetUsd(e.target.value)}
            disabled={isRunning}
            placeholder="∞"
              aria-label="Total budget cap across all cycles"
              className={`${inp} w-full`}
            />
          </div>
        </div>
      <span className="text-[12px] text-text-3">Evaluation window (optional)</span>
      <div className="grid grid-cols-2 gap-2">
        <input type="date" value={dayStart} onChange={(e) => setDayStart(e.target.value)}
          disabled={isRunning} aria-label="Day window start" className={inp} />
        <input type="date" value={dayEnd} onChange={(e) => setDayEnd(e.target.value)}
          disabled={isRunning} aria-label="Day window end" className={inp} />
        <input type="date" value={baselineStart} onChange={(e) => setBaselineStart(e.target.value)}
          disabled={isRunning} aria-label="Baseline window start" className={inp} />
        <input type="date" value={baselineEnd} onChange={(e) => setBaselineEnd(e.target.value)}
          disabled={isRunning} aria-label="Baseline window end" className={inp} />
      </div>
      {loopActive && (
        <div className="rounded border border-border bg-surface-elev px-3 py-2 text-[12px] text-text-2 font-mono space-y-0.5">
          <div className="flex items-center gap-2">
            <span className="inline-block w-1.5 h-1.5 rounded-full bg-gold animate-pulse" />
            <span>Cycle {cyclesCompleted + (isRunning ? 1 : 0)} running</span>
          </div>
          {cumulativeSpent > 0 && (
            <div className="text-text-3">Spent: ${cumulativeSpent.toFixed(4)}</div>
          )}
        </div>
      )}
      <div className="flex gap-2">
        <button
          type="button"
          onClick={handleLaunch}
          disabled={disabled}
          className="flex-1 rounded border border-gold px-3 py-2.5 text-[14px] font-medium text-gold hover:bg-gold/10 disabled:opacity-50 transition-colors"
        >
          {isRunning && !loopActive ? "Starting…" : "Run optimizer"}
        </button>
        {loopActive && (
          <button
            type="button"
            onClick={onStop}
            className="rounded border border-danger/40 px-3 py-2.5 text-[13px] text-danger hover:bg-danger/10 transition-colors"
          >
            Stop
          </button>
        )}
      </div>
      <span className="text-[11.5px] text-text-3">
        Runs continuously until stopped, max cycles, or total budget reached.
      </span>
      {(launchError ?? loopError) !== null && (
        <span className="text-[13px] text-danger">{launchError ?? loopError}</span>
      )}
    </div>
  );
}

function ModelSelectRow({
  selection,
  onSelectionChange,
}: {
  selection: OptimizerModelSelection;
  onSelectionChange: Dispatch<SetStateAction<OptimizerModelSelection>>;
}) {
  const providers = useQuery({ queryKey: settingsKeys.providers(), queryFn: listProviders });
  const rows = providers.data?.providers ?? [];
  const defaults = useQuery({
    queryKey: autooptimizerKeys.runDefaults(),
    queryFn: getRunDefaults,
  });
  const optionKeys = useMemo(
    () =>
      new Set(
        rows
          .filter(isProviderConfigured)
          .flatMap((r) => r.enabled_models.map((m) => `${r.name}::${m}`)),
      ),
    [rows],
  );
  useEffect(() => {
    if (providers.isLoading) return;
    const mutatorKey =
      selection.mutatorProvider && selection.mutatorModel
        ? `${selection.mutatorProvider}::${selection.mutatorModel}`
        : "";
    const judgeKey =
      selection.judgeProvider && selection.judgeModel
        ? `${selection.judgeProvider}::${selection.judgeModel}`
        : "";

    if (mutatorKey && !optionKeys.has(mutatorKey)) {
      clearStoredMutatorProvider();
      clearStoredMutatorModel();
      onSelectionChange((s) => ({ ...s, mutatorProvider: null, mutatorModel: "" }));
    }
    if (judgeKey && !optionKeys.has(judgeKey)) {
      clearStoredJudgeProvider();
      clearStoredJudgeModel();
      onSelectionChange((s) => ({ ...s, judgeProvider: null, judgeModel: "" }));
    }
  }, [onSelectionChange, optionKeys, providers.isLoading, selection]);

  const fallbackSource = defaults.data?.config_exists ? "optimizer config" : "built-in fallback";
  const mutatorFallback = defaults.data
    ? `${defaults.data.mutator_provider} / ${defaults.data.mutator_model}`
    : "loading fallback...";
  const writerOverrideFallback =
    selection.mutatorProvider && selection.mutatorModel
      ? `${selection.mutatorProvider} / ${selection.mutatorModel}`
      : null;
  const judgeFallback = writerOverrideFallback
    ? writerOverrideFallback
    : defaults.data
      ? `${defaults.data.judge_provider} / ${defaults.data.judge_model}`
      : "loading fallback...";
  const judgeFallbackSource = writerOverrideFallback ? "experiment writer" : fallbackSource;
  return (
    <div className="space-y-3 pt-3 border-t border-border">
      <div className="space-y-1.5">
        <span className="text-text-3 text-[12px] block">
          Experiment writer model override
        </span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={selection.mutatorProvider}
          model={selection.mutatorModel}
          onChange={(p, m) => {
            onSelectionChange((s) => ({ ...s, mutatorProvider: p, mutatorModel: m }));
            if (p === null || m === "") {
              clearStoredMutatorProvider();
              clearStoredMutatorModel();
            } else {
              setStoredMutatorProvider(p);
              setStoredMutatorModel(m);
            }
          }}
          className="w-full"
          ariaLabel="Experiment writer model override"
          placeholder="No override"
        />
        <p className="text-[11px] text-text-3">
          No override uses {fallbackSource}: <span className="font-mono">{mutatorFallback}</span>.
        </p>
      </div>
      <div className="space-y-1.5">
        <span className="text-text-3 text-[12px] block">
          Reviewer model override
        </span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={selection.judgeProvider}
          model={selection.judgeModel}
          onChange={(p, m) => {
            onSelectionChange((s) => ({ ...s, judgeProvider: p, judgeModel: m }));
            if (p === null || m === "") {
              clearStoredJudgeProvider();
              clearStoredJudgeModel();
            } else {
              setStoredJudgeProvider(p);
              setStoredJudgeModel(m);
            }
          }}
          className="w-full"
          ariaLabel="Reviewer model override"
          placeholder="No override"
        />
        <p className="text-[11px] text-text-3">
          No override reviews with {judgeFallbackSource}: <span className="font-mono">{judgeFallback}</span>.
        </p>
      </div>
      {providers.isError && (
        <p className="text-[12px] text-danger">Could not load provider models.</p>
      )}
    </div>
  );
}

function CycleLeftCard({
  isRunning,
  loopActive,
  cyclesCompleted,
  cumulativeSpent,
  loopError,
  onStartLoop,
  onStop,
}: {
  isRunning: boolean;
  loopActive: boolean;
  cyclesCompleted: number;
  cumulativeSpent: number;
  loopError: string | null;
  onStartLoop: (config: LaunchConfig) => void;
  onStop: () => void;
}) {
  const [selection, setSelection] = useState<OptimizerModelSelection>({
    mutatorProvider: getStoredMutatorProvider(),
    mutatorModel: getStoredMutatorModel() ?? "",
    judgeProvider: getStoredJudgeProvider(),
    judgeModel: getStoredJudgeModel() ?? "",
  });

  return (
    <div
      id="optimizer-run-controls"
      className="rounded-md border border-border p-5 space-y-4 scroll-mt-24"
    >
      <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
        Optimizer Run
      </span>
      <LaunchStrip
        modelSelection={selection}
        onStartLoop={onStartLoop}
        isRunning={isRunning}
        loopActive={loopActive}
        cyclesCompleted={cyclesCompleted}
        cumulativeSpent={cumulativeSpent}
        loopError={loopError}
        onStop={onStop}
      />
      <ModelSelectRow selection={selection} onSelectionChange={setSelection} />
    </div>
  );
}

// ─── Middle column ────────────────────────────────────────────────────────────

function EventLogCard({ events, bottomRef }: { events: EventRow[]; bottomRef: RefObject<HTMLDivElement> }) {
  return (
    <Card>
      <CardHeader title="Live progress · cycle events" />
      {events.length === 0 ? (
        <div className="px-5 pb-5 pt-2 text-[13px] text-text-3">
          Waiting for cycle…
        </div>
      ) : (
        <div
          className="overflow-y-auto max-h-[480px] pb-4"
          role="log"
          aria-live="polite"
          aria-label="Cycle event feed"
        >
          <table className="w-full text-[13px] border-collapse">
            <thead>
              <tr className="sticky top-0 bg-surface-card border-b border-border">
                <th className="text-left font-medium text-text-3 px-5 py-2 w-[140px]">Time</th>
                <th className="text-left font-medium text-text-3 px-5 py-2">Event</th>
                <th className="text-left font-medium text-text-3 px-5 py-2 w-[160px]">Cycle</th>
              </tr>
            </thead>
            <tbody>
              {events.map((ev) => (
                <tr
                  key={ev._row_id}
                  className="border-b border-border last:border-0 hover:bg-surface-elev/40"
                >
                  <td className="px-5 py-2 text-text-3 font-mono text-[12px] whitespace-nowrap">
                    {formatEventTime(ev.ts)}
                  </td>
                  <td className="px-5 py-2 text-text">{formatEventLabel(ev)}</td>
                  <td className="px-5 py-2 text-text-3 font-mono text-[11px] truncate max-w-[160px]">
                    {ev.cycle_id ?? "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div ref={bottomRef} />
        </div>
      )}
    </Card>
  );
}

// ─── Active lineages section ──────────────────────────────────────────────────

function LineageCard({ group }: { group: CycleGroup }) {
  const isActive = group.activeCount > 0;
  return (
    <div className="rounded-md border border-border p-4 space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-[12px] text-text truncate">
          {group.cycle_id ? group.cycle_id.slice(0, 12) : "—"}
        </span>
        <Pill tone={isActive ? "gold" : "default"}>{isActive ? "Active" : "Cooled"}</Pill>
      </div>
      <div className="flex items-center gap-3 text-[12px] text-text-3">
        <span>{group.nodes.length} experiments</span>
        {group.activeCount > 0 && (
          <span className="text-gold font-mono">{group.activeCount} kept</span>
        )}
      </div>
      {group.latestNode.diversity_score != null && (
        <p className="font-mono text-[11px] text-text-3">
          div: {group.latestNode.diversity_score.toFixed(3)}
        </p>
      )}
    </div>
  );
}

function ActiveLineagesSectionFull({ nodes }: { nodes: LineageNode[] }) {
  const groups = buildCycleGroups(nodes).slice(0, 6);
  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-base font-semibold text-text">Active lineages</h2>
        <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
          Strategy populations currently evolving
        </p>
      </div>
      {groups.length === 0 ? (
        <div className="rounded-md border border-border px-5 py-4">
          <p className="text-[13px] text-text-3">No experiments yet</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {groups.map((g) => (
            <LineageCard key={g.cycle_id ?? "null"} group={g} />
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Recent cycles section ────────────────────────────────────────────────────

// F23: compact token count, e.g. 1_935_625 → "1.9M".
function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function CycleRow({ group, cost }: { group: CycleGroup; cost?: CycleRunSummary }) {
  const firstSeen = group.nodes[group.nodes.length - 1]?.created_at ?? "";
  const diversityScores = group.nodes
    .map((n) => n.diversity_score)
    .filter((s): s is number => s != null);
  const bestDiversity = diversityScores.length > 0 ? Math.max(...diversityScores) : null;
  const totalTokens =
    cost?.input_tokens != null && cost?.output_tokens != null
      ? cost.input_tokens + cost.output_tokens
      : null;
  return (
    <tr className="border-b border-border last:border-0 hover:bg-surface-elev/40">
      <td className="px-4 py-2 font-mono text-[12px] text-text">
        {group.cycle_id ? group.cycle_id.slice(0, 12) : "—"}
      </td>
      <td className="px-4 py-2 text-right font-mono text-text-2">{group.nodes.length}</td>
      <td className={`px-4 py-2 text-right font-mono ${group.activeCount > 0 ? "text-gold" : "text-text-3"}`}>
        {group.activeCount}
      </td>
      <td className="px-4 py-2 text-right font-mono text-text-3">
        {cost?.cost_usd != null ? (
          <span title={cost.unpriced_calls ? `${cost.unpriced_calls} call(s) unpriced` : undefined}>
            ${cost.cost_usd.toFixed(4)}
            {cost.unpriced_calls ? "+" : ""}
          </span>
        ) : (
          "—"
        )}
      </td>
      <td className="px-4 py-2 text-right font-mono text-text-3">
        {totalTokens != null ? formatTokens(totalTokens) : "—"}
      </td>
      <td className="px-4 py-2 text-right font-mono text-text-3">
        {bestDiversity != null ? bestDiversity.toFixed(3) : "—"}
      </td>
      <td className="px-4 py-2 text-text-3">{formatRelativeDate(firstSeen)}</td>
    </tr>
  );
}

function CycleTable({
  groups,
  costByCycle,
}: {
  groups: CycleGroup[];
  costByCycle: Map<string, CycleRunSummary>;
}) {
  return (
    <div className="rounded-md border border-border overflow-hidden">
      <table className="w-full text-[13px] border-collapse">
        <thead>
          <tr className="bg-surface-card border-b border-border">
            <th className="text-left font-medium text-text-3 px-4 py-2">Cycle ID</th>
            <th className="text-right font-medium text-text-3 px-4 py-2">Experiments</th>
            <th className="text-right font-medium text-text-3 px-4 py-2">Kept</th>
            <th className="text-right font-medium text-text-3 px-4 py-2">Cost</th>
            <th className="text-right font-medium text-text-3 px-4 py-2">Tokens</th>
            <th className="text-right font-medium text-text-3 px-4 py-2">Best diversity</th>
            <th className="text-left font-medium text-text-3 px-4 py-2">First seen</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((g) => (
            <CycleRow
              key={g.cycle_id ?? "null"}
              group={g}
              cost={g.cycle_id ? costByCycle.get(g.cycle_id) : undefined}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function RecentCyclesSectionFull({
  nodes,
  onTabChange,
}: {
  nodes: LineageNode[];
  onTabChange?: (tab: string) => void;
}) {
  const groups = buildCycleGroups(nodes).slice(0, 10);
  const { data: cycleRuns = [] } = useCycleRuns();
  const costByCycle = new Map<string, CycleRunSummary>(cycleRuns.map((c) => [c.cycle_id, c]));
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-base font-semibold text-text">Recent cycles</h2>
          <p className="font-mono text-[11.5px] text-text-3 mt-0.5">
            History of completed optimization cycles
          </p>
        </div>
        {nodes.length > 0 && (
          <button
            type="button"
            onClick={() => onTabChange?.("genealogy")}
            className="text-[13px] text-gold hover:opacity-80 transition-opacity"
          >
            View genealogy →
          </button>
        )}
      </div>
      {groups.length === 0 ? (
        <div className="rounded-md border border-border px-5 py-4">
          <p className="text-[13px] text-text-3">No cycles yet</p>
        </div>
      ) : (
        <CycleTable groups={groups} costByCycle={costByCycle} />
      )}
    </div>
  );
}

// ─── Live cost ticker (F35.3) ───────────────────────────────────────────────

/** Live cost/tokens strip for the running cycle. Polls `/cycles/:id/cost` (the
 *  background ticker persists it every ~10s) so spend streams during the run —
 *  including before the first candidate commits, the runaway-token case. Inline
 *  full-width strip (no right-side box) per the dashboard layout rule. */
function LiveCostTicker({
  activeCycleId,
  isRunning,
}: {
  activeCycleId: string | null;
  isRunning: boolean;
}) {
  const { data: cost } = useCycleCost(activeCycleId, isRunning);
  if (!isRunning || !activeCycleId) return null;

  const totalTokens =
    cost?.input_tokens != null || cost?.output_tokens != null
      ? (cost?.input_tokens ?? 0) + (cost?.output_tokens ?? 0)
      : null;
  const costLabel = cost?.cost_usd != null ? `$${cost.cost_usd.toFixed(4)}` : "$0.0000";
  const tokensLabel = totalTokens != null ? formatTokens(totalTokens) : "—";

  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-1 rounded-md border border-border bg-surface-card px-4 py-2 text-[13px]">
      <span className="font-mono text-[11.5px] uppercase tracking-wide text-text-3">
        Live spend
      </span>
      <span className="text-text-2">
        Cost{" "}
        <span
          className="font-mono text-text"
          title={cost?.unpriced_calls ? `${cost.unpriced_calls} call(s) unpriced` : undefined}
        >
          {costLabel}
          {cost?.unpriced_calls ? "+" : ""}
        </span>
      </span>
      <span className="text-text-2">
        Tokens <span className="font-mono text-text">{tokensLabel}</span>
      </span>
      {cost?.input_tokens != null && (
        <span className="font-mono text-[11.5px] text-text-3">
          {formatTokens(cost.input_tokens)} in · {formatTokens(cost.output_tokens ?? 0)} out
        </span>
      )}
      {!cost?.recorded && (
        <span className="font-mono text-[11.5px] text-text-3">accruing…</span>
      )}
    </div>
  );
}

// ─── Root export ──────────────────────────────────────────────────────────────

export function LiveCycleView({ onTabChange, embedded = false, activeTab = "home", launchOnly = false }: { onTabChange?: (tab: string) => void; embedded?: boolean; activeTab?: string; launchOnly?: boolean } = {}) {
  const queryClient = useQueryClient();
  const { events, connected, isRunning, activeCycleId } = useCycleEventStream();
  const bottomRef = useRef<HTMLDivElement>(null);
  const { data: lineageNodes = [] } = useLineageNodes();
  // Active cycle's per-regime node detail feeds the live experiments×regimes
  // heatmap. Polls via useCycleRun; cells flip to "done" as results land.
  const { data: activeCycle } = useCycleRun(activeCycleId ?? undefined);
  const heatmapNodes = activeCycle?.nodes ?? [];

  // Continuous loop state
  const loopConfigRef = useRef<LaunchConfig | null>(null);
  const stopRequestedRef = useRef(false);
  const [loopActive, setLoopActive] = useState(false);
  const [cyclesCompleted, setCyclesCompleted] = useState(0);
  const [cumulativeSpent, setCumulativeSpent] = useState(0);
  const [loopError, setLoopError] = useState<string | null>(null);
  const lastProcessedRowId = useRef<number>(-1);

  const stopLoop = useCallback(() => {
    stopRequestedRef.current = true;
    loopConfigRef.current = null;
    setLoopActive(false);
  }, []);

  const startLoop = useCallback(async (config: LaunchConfig) => {
    stopRequestedRef.current = false;
    setLoopError(null);
    loopConfigRef.current = config;
    setCyclesCompleted(0);
    setCumulativeSpent(0);
    lastProcessedRowId.current = -1;
    setLoopActive(true);
    try {
      await startRunCycle({
        strategy_id: config.strategy_id,
        mutator_provider: config.mutator_provider,
        mutator_model: config.mutator_model,
        judge_provider: config.judge_provider,
        judge_model: config.judge_model,
        budget_usd: config.budget_usd,
        day_start: config.day_start,
        day_end: config.day_end,
        baseline_start: config.baseline_start,
        baseline_end: config.baseline_end,
        experiments_per_cycle: config.experiments_per_cycle,
      });
      await queryClient.invalidateQueries({ queryKey: autooptimizerKeys.lineage() });
    } catch (err) {
      loopConfigRef.current = null;
      setLoopActive(false);
      setLoopError(err instanceof Error ? err.message : "Failed to start optimizer cycle");
    }
  }, [queryClient]);

  // Auto-relaunch after cycle_finished when loop is active
  useEffect(() => {
    if (!loopActive || stopRequestedRef.current) return;
    const lastFinished = [...events].reverse().find((ev) => {
      const et = ev.event_type ?? ev.type ?? ev.kind ?? "";
      return et === "cycle_finished";
    });
    if (!lastFinished || lastFinished._row_id <= lastProcessedRowId.current) return;
    lastProcessedRowId.current = lastFinished._row_id;

    const loop = loopConfigRef.current;
    if (!loop || stopRequestedRef.current) return;

    const newCyclesCompleted = cyclesCompleted + 1;
    setCyclesCompleted(newCyclesCompleted);

    const relaunch = (accumulatedSpent: number) => {
      if (!loopConfigRef.current || stopRequestedRef.current) return;
      const l = loopConfigRef.current;
      if (l.maxCycles !== null && newCyclesCompleted >= l.maxCycles) {
        loopConfigRef.current = null;
        setLoopActive(false);
        return;
      }
      if (l.totalBudgetUsd !== null && accumulatedSpent >= l.totalBudgetUsd) {
        loopConfigRef.current = null;
        setLoopActive(false);
        return;
      }
      void startRunCycle({
        strategy_id: l.strategy_id,
        mutator_provider: l.mutator_provider,
        mutator_model: l.mutator_model,
        judge_provider: l.judge_provider,
        judge_model: l.judge_model,
        budget_usd: l.budget_usd,
        day_start: l.day_start,
        day_end: l.day_end,
        baseline_start: l.baseline_start,
        baseline_end: l.baseline_end,
        experiments_per_cycle: l.experiments_per_cycle,
      }).then(() => {
        void queryClient.invalidateQueries({ queryKey: autooptimizerKeys.lineage() });
      }).catch((err: unknown) => {
        loopConfigRef.current = null;
        setLoopActive(false);
        setLoopError(err instanceof Error ? err.message : "Optimizer cycle failed");
      });
    };

    if (lastFinished.cycle_id) {
      getCycleCost(lastFinished.cycle_id).then((cost) => {
        const spent = cumulativeSpent + (cost.cost_usd ?? 0);
        setCumulativeSpent(spent);
        relaunch(spent);
      }).catch(() => {
        relaunch(cumulativeSpent);
      });
    } else {
      relaunch(cumulativeSpent);
    }
  }, [events, loopActive, cyclesCompleted, cumulativeSpent, queryClient]);

  useEffect(() => {
    if (typeof bottomRef.current?.scrollIntoView === "function") {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [events.length]);

  // launchOnly: render just the launch form panel, no event feed or live status bar.
  if (launchOnly) {
    return (
      <CycleLeftCard
        isRunning={isRunning}
        loopActive={loopActive}
        cyclesCompleted={cyclesCompleted}
        cumulativeSpent={cumulativeSpent}
        loopError={loopError}
        onStartLoop={(config) => { void startLoop(config); }}
        onStop={stopLoop}
      />
    );
  }

  return (
    <div className="space-y-6">
      {!embedded && (
        <LivePageHeader
          nodes={lineageNodes}
          isRunning={isRunning}
          activeCycleId={activeCycleId}
          onTabChange={onTabChange}
        />
      )}
      <div className="flex items-center gap-3">
        <span
          className={[
            "inline-block w-2 h-2 rounded-full transition-all",
            connected && isRunning ? "bg-gold animate-pulse" : connected ? "bg-gold" : "bg-text-3",
          ].join(" ")}
          aria-label={connected ? (isRunning ? "Running" : "Connected") : "Disconnected"}
        />
        <span className="text-[13px] text-text-2">
          {isRunning ? "Live · cycle in progress" : connected ? "Live" : "Waiting for connection…"}
        </span>
        {loopActive && !isRunning && (
          <span className="text-[12px] text-text-3 font-mono">next cycle queued…</span>
        )}
      </div>
      <LiveCostTicker activeCycleId={activeCycleId} isRunning={isRunning} />
      {/* Zone 3 — 2-up live band: experiments×regimes heatmap (left) +
          event feed (right). Replaces the old 3-col [300·1fr·260] grid; the
          launch form now lives in the command-bar drawer (launchOnly) and the
          Kept/Next rail folded into Zone 4. */}
      <div className="grid grid-cols-1 xl:grid-cols-[1.3fr_1fr] gap-6 items-stretch">
        <LiveEvalHeatmap nodes={heatmapNodes} isRunning={isRunning} />
        <EventLogCard events={events} bottomRef={bottomRef} />
      </div>
      {activeTab === "genealogy" && <ActiveLineagesSectionFull nodes={lineageNodes} />}
      {!embedded && (
        <RecentCyclesSectionFull nodes={lineageNodes} onTabChange={onTabChange} />
      )}
    </div>
  );
}

function formatEventTime(ts: string | undefined): string {
  if (!ts) return "—";
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

