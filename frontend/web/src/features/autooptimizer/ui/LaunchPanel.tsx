import {
  type Dispatch,
  type SetStateAction,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { ModelPicker } from "@/components/ModelPicker";
import { StrategyPicker } from "@/components/primitives/StrategyPicker";
import {
  type StartRunCycleRequest,
  getCycleCost,
  getRunDefaults,
  startRunCycle,
  autooptimizerKeys,
} from "../api";
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
} from "../preferences";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { listProviders, settingsKeys } from "@/api/settings";
import { isProviderConfigured } from "@/lib/providers";

type OptimizerModelSelection = {
  mutatorProvider: string | null;
  mutatorModel: string;
  judgeProvider: string | null;
  judgeModel: string;
};

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
      <div className="text-[12px] text-text-3">Parent strategy</div>
      <StrategyPicker
        strategies={strategies ?? []}
        value={strategyId}
        onChange={setStrategyId}
        loading={strategiesLoading}
        disabled={isRunning || noStrategies}
        placeholder={noStrategies ? "No strategies" : "— pick a strategy —"}
        className="h-9 min-h-9 w-full justify-between"
      />
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

/** Inline launch panel — the launch form + continuous-loop machinery extracted
 *  from the retired LiveCycleView (`launchOnly` branch). Owns the launch
 *  config, the auto-relaunch-on-cycle_finished loop (with maxCycles /
 *  totalBudgetUsd caps and cumulative cost accumulation), and the model
 *  override selection persisted in localStorage. */
export function LaunchPanel() {
  const queryClient = useQueryClient();
  const { events, isRunning } = useCycleEventStream();

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
