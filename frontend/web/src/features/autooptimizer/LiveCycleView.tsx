import { type RefObject, useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card, CardHeader } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ModelPicker } from "@/components/ModelPicker";
import { ApiError } from "@/api/client";
import {
  type CycleProgressEvent,
  type LineageNode,
  formatEventLabel,
  startRunCycle,
  cancelRunCycle,
  useLineageNodes,
  useCycleRuns,
  useCycleCost,
  type CycleRunSummary,
  autooptimizerKeys,
} from "./api";
import {
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

type EventRow = CycleProgressEvent & { _row_id: number };

let nextRowId = 1;

const SSE_EVENT_NAMES = [
  "cycle_started",
  "parent_selected",
  "mutation_proposed",
  "no_candidate",
  "mutation_gated",
  "honesty_check_run",
  "judge_finding",
  "cycle_finished",
  "lagged",
] as const;


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

function deriveCycleState(
  events: EventRow[],
): { isRunning: boolean; activeCycleId: string | null } {
  for (let i = events.length - 1; i >= 0; i--) {
    const et = events[i].event_type ?? events[i].type ?? events[i].kind ?? "";
    if (et === "cycle_finished") {
      return { isRunning: false, activeCycleId: null };
    }
    if (et === "cycle_started") return { isRunning: true, activeCycleId: events[i].cycle_id ?? null };
  }
  return { isRunning: false, activeCycleId: null };
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

function LaunchStrip() {
  const queryClient = useQueryClient();
  const [strategyId, setStrategyId] = useState("");
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [launchMessage, setLaunchMessage] = useState<string | null>(null);
  // F28: optional budget cap + evaluation window. Empty = no cap / config default.
  const [budgetUsd, setBudgetUsd] = useState("");
  const [dayStart, setDayStart] = useState("");
  const [dayEnd, setDayEnd] = useState("");
  const [baselineStart, setBaselineStart] = useState("");
  const [baselineEnd, setBaselineEnd] = useState("");
  const { data: strategies, isPending: strategiesLoading } = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const launchMutation = useMutation({
    mutationFn: startRunCycle,
    onSuccess: async (resp) => {
      setLaunchError(null);
      setLaunchMessage(resp.message);
      await queryClient.invalidateQueries({ queryKey: autooptimizerKeys.lineage() });
    },
    onError: (err) => {
      setLaunchMessage(null);
      if (err instanceof ApiError) {
        setLaunchError(err.field ? `${err.field}: ${err.message}` : err.message);
      } else {
        setLaunchError(err instanceof Error ? err.message : "Network error");
      }
    },
  });
  const handleLaunch = async () => {
    const trimmed = strategyId.trim();
    if (!trimmed) { setLaunchError("Select a strategy"); return; }
    setLaunchError(null);
    setLaunchMessage(null);
    // F28: parse + validate the optional budget cap.
    let budget: number | null = null;
    if (budgetUsd.trim() !== "") {
      const n = Number(budgetUsd);
      if (!Number.isFinite(n) || n <= 0) {
        setLaunchError("Budget must be a positive USD amount");
        return;
      }
      budget = n;
    }
    const orNull = (s: string) => (s.trim() === "" ? null : s.trim());
    launchMutation.mutate({
      strategy_id: trimmed,
      mutator_provider: getStoredMutatorProvider(),
      mutator_model: getStoredMutatorModel(),
      judge_provider: getStoredJudgeProvider(),
      judge_model: getStoredJudgeModel(),
      budget_usd: budget,
      day_start: orNull(dayStart),
      day_end: orNull(dayEnd),
      baseline_start: orNull(baselineStart),
      baseline_end: orNull(baselineEnd),
    });
  };
  const isRunning = launchMutation.isPending;
  const inp = "min-h-9 bg-surface-elev border border-border rounded text-text text-[13px] px-2 py-1";
  const noStrategies = !strategiesLoading && (!strategies || strategies.length === 0);
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
      {/* F28: budget cap + evaluation window — bound a UI cycle's cost/length.
          Empty fields use no cap / the config default window. */}
      <label htmlFor="optimizer-budget" className="text-[12px] text-text-3 mt-1">
        Budget cap (USD, optional)
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
        aria-label="Budget cap in USD"
        className={`${inp} w-full`}
      />
      <span className="text-[12px] text-text-3 mt-1">Evaluation window (optional)</span>
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
      <button
        type="button"
        onClick={() => { void handleLaunch(); }}
        disabled={isRunning || !strategyId.trim() || noStrategies}
        className="w-full rounded bg-accent px-3 py-3 text-[14px] font-medium text-on-accent hover:opacity-90 disabled:opacity-50"
      >
        {isRunning ? "Starting…" : "Run optimizer"}
      </button>
      {/* F29: a run performs one propose → gate → commit against the chosen
          parent — the dashboard equivalent of `xvn optimizer mutate-once`. */}
      <span className="text-[11.5px] text-text-3">
        Runs one gated experiment on the parent (≡ <code>mutate-once</code>).
      </span>
      {launchError !== null && (
        <span className="text-[13px] text-danger">{launchError}</span>
      )}
      {launchMessage !== null && (
        <span className="text-[13px] text-green-500">{launchMessage}</span>
      )}
    </div>
  );
}

function ModelSelectRow() {
  const providers = useQuery({ queryKey: settingsKeys.providers(), queryFn: listProviders });
  const rows = providers.data?.providers ?? [];
  const [mutatorProvider, setMutatorProvider] = useState<string | null>(() => getStoredMutatorProvider());
  const [mutatorModel, setMutatorModel] = useState<string>(() => getStoredMutatorModel() ?? "");
  const [judgeProvider, setJudgeProvider] = useState<string | null>(() => getStoredJudgeProvider());
  const [judgeModel, setJudgeModel] = useState<string>(() => getStoredJudgeModel() ?? "");
  const sel = "min-h-9 bg-surface-elev border border-border rounded text-text text-[13px] px-2 py-1";
  return (
    <div className="space-y-3 pt-3 border-t border-border">
      <div className="space-y-1.5">
        <span className="text-text-3 text-[12px] block">
          Experiment writer
        </span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={mutatorProvider}
          model={mutatorModel}
          onChange={(p, m) => { setMutatorProvider(p); setMutatorModel(m); if (p !== null) setStoredMutatorProvider(p); if (m) setStoredMutatorModel(m); }}
          className={`${sel} w-full`}
          ariaLabel="Experiment writer model"
          placeholder="Use config default"
        />
      </div>
      <div className="space-y-1.5">
        <span className="text-text-3 text-[12px] block">
          Reviewer
        </span>
        <ModelPicker
          rows={rows}
          loading={providers.isLoading}
          provider={judgeProvider}
          model={judgeModel}
          onChange={(p, m) => { setJudgeProvider(p); setJudgeModel(m); if (p !== null) setStoredJudgeProvider(p); if (m) setStoredJudgeModel(m); }}
          className={`${sel} w-full`}
          ariaLabel="Reviewer model"
          placeholder="Use writer provider/default"
        />
      </div>
      {providers.isError && (
        <p className="text-[12px] text-danger">Could not load provider models.</p>
      )}
    </div>
  );
}

function CycleLeftCard() {
  return (
    <div
      id="optimizer-run-controls"
      className="rounded-md border border-gold/30 bg-gradient-to-b from-gold/5 to-transparent p-5 space-y-4 scroll-mt-24"
    >
      <span className="uppercase tracking-[0.22em] text-[9.5px] text-gold font-medium block">
        Optimizer Run
      </span>
      <Pill tone="default">No cycle running</Pill>
      <LaunchStrip />
      <ModelSelectRow />
    </div>
  );
}

// ─── Right column ─────────────────────────────────────────────────────────────

function KeptNextCard({ nodes }: { nodes: LineageNode[] }) {
  const kept = nodes.filter((n) => n.status === "active");
  const recent = [...kept]
    .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
    .slice(0, 3);
  return (
    <div className="rounded-md border border-border p-5 space-y-4">
      <div>
        <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
          Kept
        </span>
        <span className="font-mono text-3xl font-semibold text-gold">{kept.length}</span>
        <p className="text-[12px] text-text-3 mt-1">experiments kept this week</p>
      </div>
      {recent.length > 0 && (
        <div className="space-y-1.5">
          {recent.map((n) => (
            <div key={n.bundle_hash} className="flex items-center justify-between gap-2">
              <span className="font-mono text-[11px] text-text-2">{n.bundle_hash.slice(0, 8)}</span>
              <span className="text-[11px] text-text-3">{formatRelativeDate(n.created_at)}</span>
            </div>
          ))}
        </div>
      )}
      <div className="border-t border-border pt-4">
        <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium block">
          Next
        </span>
        <p className="text-[13px] text-text-2 mt-1">No scheduled run</p>
      </div>
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

export function LiveCycleView({ onTabChange }: { onTabChange?: (tab: string) => void } = {}) {
  const [events, setEvents] = useState<EventRow[]>([]);
  const [connected, setConnected] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const { data: lineageNodes = [] } = useLineageNodes();
  const { isRunning, activeCycleId } = deriveCycleState(events);

  const appendEvent = (event: CycleProgressEvent) => {
    setEvents((prev) => {
      const row: EventRow = { ...event, _row_id: nextRowId++ };
      const next = prev.length >= 200 ? prev.slice(1) : prev;
      return [...next, row];
    });
  };

  useEffect(() => {
    const source = new EventSource("/api/autooptimizer/events");
    const handleMessage = (ev: Event) => {
      const event = parseSsePayload((ev as MessageEvent).data, ev.type);
      if (event) appendEvent(event);
    };
    source.addEventListener("open", () => { setConnected(true); });
    source.addEventListener("message", handleMessage);
    for (const name of SSE_EVENT_NAMES) source.addEventListener(name, handleMessage);
    source.addEventListener("error", () => { setConnected(false); });
    return () => {
      source.removeEventListener("message", handleMessage);
      for (const eventName of SSE_EVENT_NAMES) {
        source.removeEventListener(eventName, handleMessage);
      }
      source.close();
      setConnected(false);
    };
  }, []);

  useEffect(() => {
    if (typeof bottomRef.current?.scrollIntoView === "function") {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [events.length]);

  return (
    <div className="space-y-6">
      <LivePageHeader
        nodes={lineageNodes}
        isRunning={isRunning}
        activeCycleId={activeCycleId}
        onTabChange={onTabChange}
      />
      <div className="flex items-center gap-3">
        <span
          className={[
            "inline-block w-2 h-2 rounded-full",
            connected ? "bg-green-500" : "bg-text-3",
          ].join(" ")}
          aria-label={connected ? "Connected" : "Disconnected"}
        />
        <span className="text-[13px] text-text-2">
          {connected ? "Live" : "Waiting for connection…"}
        </span>
      </div>
      <LiveCostTicker activeCycleId={activeCycleId} isRunning={isRunning} />
      <div className="grid grid-cols-1 xl:grid-cols-[300px_1fr_260px] gap-6">
        <CycleLeftCard />
        <EventLogCard events={events} bottomRef={bottomRef} />
        <KeptNextCard nodes={lineageNodes} />
      </div>
      <ActiveLineagesSectionFull nodes={lineageNodes} />
      <RecentCyclesSectionFull nodes={lineageNodes} onTabChange={onTabChange} />
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

function parseSsePayload(raw: unknown, fallbackKind: string): CycleProgressEvent | null {
  if (typeof raw !== "string" || raw.trim() === "") return null;
  const parsed = parseJsonObject(raw);
  if (!parsed) return null;
  if ("dropped" in parsed) return null;

  const data = isRecord(parsed.data) ? parsed.data : parsed;
  const kind =
    stringValue(data.event_type) ??
    stringValue(data.type) ??
    stringValue(parsed.kind) ??
    fallbackKind;
  if (kind === "message") return null;
  return {
    ...data,
    event_type: kind,
    kind,
    display_label: stringValue(parsed.display_label) ?? stringValue(data.display_label),
    ts: stringValue(data.ts) ?? new Date().toISOString(),
    cycle_id: stringValue(data.cycle_id),
    bundle_hash: stringValue(data.bundle_hash),
    parent_hash: stringValue(data.parent_hash),
    child_hash: stringValue(data.child_hash),
  };
}

function parseJsonObject(raw: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(raw) as unknown;
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
