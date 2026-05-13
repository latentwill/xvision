import { useEffect, useState, type FormEvent } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import { ApiError } from "@/api/client";
import { chartKeys, getRunChart } from "@/api/chart";
import { RunChart } from "@/components/chart/RunChart";
import {
  evalKeys,
  deleteRun,
  listRuns,
  startRun,
  type StartRunReq,
} from "@/api/eval";
import {
  listScenarios,
  scenarioKeys,
} from "@/api/scenarios";
import {
  getBrokers,
  listProviders,
  settingsKeys,
} from "@/api/settings";
import {
  listStrategies,
  strategyKeys,
  type StrategyListItem,
} from "@/api/strategies";
import type {
  BrokersReport,
  ProvidersReport,
  RunDetail,
  RunMode,
  RunSummary,
  Scenario,
} from "@/api/types.gen";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "default" | "info"> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

export function EvalRunsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const q = useQuery({
    queryKey: evalKeys.runs(),
    queryFn: listRuns,
    // Poll while any run is still in flight; stop once everything's
    // terminal. Background eval tasks drive in the dashboard process
    // and can take minutes — without this the list looks frozen.
    refetchInterval: (query) => {
      const items = query.state.data as RunSummary[] | undefined;
      if (!items) return false;
      const inflight = items.some(
        (r) => r.status === "queued" || r.status === "running",
      );
      return inflight ? 2000 : false;
    },
  });
  const navigate = useNavigate();
  const preselectedStrategy = searchParams.get("strategy") ?? "";
  const startRequested = searchParams.get("start") === "1";
  // Selection state for the Compare flow. Lifted here so the Topbar can
  // render the action button next to the run count.
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [startOpen, setStartOpen] = useState(startRequested);

  useEffect(() => {
    if (startRequested) {
      setStartOpen(true);
    }
  }, [startRequested]);
  const latestRunId = q.data?.[0]?.id ?? "";
  const latestChart = useQuery({
    queryKey: chartKeys.run(latestRunId),
    queryFn: () => getRunChart(latestRunId),
    enabled: !!latestRunId,
  });

  function toggleSelected(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function clearSelection() {
    setSelected(new Set());
  }

  function openCompare() {
    if (selected.size < 2) return;
    const ids = [...selected].join(",");
    navigate(`/eval-runs/compare?ids=${ids}`);
  }

  return (
    <>
      <Topbar title="Eval" sub={subtitleFor(q)} />

      <div className="mb-3 flex justify-end items-center gap-2">
        {selected.size > 0 ? (
          <CompareToolbar
            count={selected.size}
            onCompare={openCompare}
            onClear={clearSelection}
          />
        ) : null}
        <button
          type="button"
          onClick={() => setStartOpen(true)}
          className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
        >
          <Icon name="plus" size={13} /> Start eval
        </button>
      </div>
      {startOpen ? (
        <StartEvalDialog
          initialAgentId={preselectedStrategy}
          onClose={() => {
            setStartOpen(false);
            const next = new URLSearchParams(searchParams);
            next.delete("start");
            setSearchParams(next);
          }}
        />
      ) : null}

      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : q.data && q.data.length === 0 ? (
          <EmptyState />
        ) : (
          <RunsTable
            items={q.data ?? []}
            selected={selected}
            onToggle={toggleSelected}
          />
        )}
      </Card>

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Latest run chart
      </h2>
      <Card className="p-5">
        {q.isPending ? (
          <div className="text-text-3 text-[13px] text-center py-6">
            Loading runs…
          </div>
        ) : !latestRunId ? (
          <div className="text-text-3 text-[13px] text-center py-6">
            No runs yet. Start an eval to render chart history.
          </div>
        ) : latestChart.isPending ? (
          <div className="text-text-3 text-[13px] text-center py-6">
            Loading chart…
          </div>
        ) : latestChart.isError ? (
          <div className="text-danger text-[13px] text-center py-6">
            Chart unavailable for latest run.
          </div>
        ) : latestChart.data ? (
          <RunChart payload={latestChart.data} />
        ) : null}
      </Card>
    </>
  );
}

// ── Existing helpers ───────────────────────────────────────────────────────

function subtitleFor(q: ReturnType<typeof useQuery>) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load runs";
  const data = q.data as { length: number } | undefined;
  if (!data) return "";
  const n = data.length;
  return `${n} ${n === 1 ? "run" : "runs"}`;
}

function CompareToolbar({
  count,
  onCompare,
  onClear,
}: {
  count: number;
  onCompare: () => void;
  onClear: () => void;
}) {
  const ready = count >= 2;
  return (
    <div className="flex items-center gap-2">
      <span className="text-[12px] text-text-2">
        {count} selected
      </span>
      <button
        type="button"
        onClick={onClear}
        className="text-[12px] text-text-3 hover:text-text underline-offset-2 hover:underline"
      >
        clear
      </button>
      <button
        type="button"
        onClick={onCompare}
        disabled={!ready}
        title={ready ? "" : "Select at least two runs to compare"}
        className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border transition-colors ${
          ready
            ? "border-gold text-gold hover:bg-gold/10"
            : "border-border text-text-3 cursor-not-allowed opacity-60"
        }`}
      >
        Compare {ready ? `(${count})` : ""} →
      </button>
    </div>
  );
}

function RunsTable({
  items,
  selected,
  onToggle,
}: {
  items: RunSummary[];
  selected: Set<string>;
  onToggle: (id: string) => void;
}) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const remove = useMutation({
    mutationFn: deleteRun,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evalKeys.runs() });
    },
  });

  function go(id: string) {
    navigate(`/eval-runs/${id}`);
  }

  return (
    <table className="w-full">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 pl-5 pr-2 w-8"></th>
          <th className="font-normal py-2.5 px-3">ID</th>
          <th className="font-normal py-2.5 px-3">Strategy</th>
          <th className="font-normal py-2.5 px-3">Scenario</th>
          <th className="font-normal py-2.5 px-3">Mode</th>
          <th className="font-normal py-2.5 px-3">Status</th>
          <th className="font-normal py-2.5 px-3 text-right">Sharpe</th>
          <th className="font-normal py-2.5 px-3 text-right">Max DD</th>
          <th className="font-normal py-2.5 px-3 text-right">Return</th>
          <th className="font-normal py-2.5 px-5">Started</th>
          <th className="font-normal py-2.5 px-5 text-right"></th>
        </tr>
      </thead>
      <tbody>
        {items.map((row) => {
          const isChecked = selected.has(row.id);
          return (
            <tr
              key={row.id}
              role="link"
              tabIndex={0}
              onClick={() => go(row.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  go(row.id);
                }
              }}
              className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover focus:bg-surface-hover focus:outline-none transition-colors cursor-pointer"
            >
              <td
                className="py-3 pl-5 pr-2 w-8"
                // Stop the checkbox cell from triggering row navigation. The
                // input below stops its own events too, but covering the
                // surrounding cell makes the entire 32px tap target safe.
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="checkbox"
                  aria-label={`Select run ${row.id.slice(0, 8)}`}
                  checked={isChecked}
                  onChange={(e) => {
                    e.stopPropagation();
                    onToggle(row.id);
                  }}
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => e.stopPropagation()}
                  className="accent-gold cursor-pointer"
                />
              </td>
              <td className="py-3 px-3 font-mono text-text text-[12px]">
                {row.id.slice(0, 12)}…
              </td>
              <td className="py-3 px-3 font-mono text-text-2 text-[12px]">
                {row.strategy_bundle_hash.slice(0, 8)}
              </td>
              <td className="py-3 px-3 text-text-2">{row.scenario_id}</td>
              <td className="py-3 px-3 text-text-2">{row.mode}</td>
              <td className="py-3 px-3">
                <StatusPill status={row.status} />
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtNumber(row.sharpe)}
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtPct(row.max_drawdown_pct)}
              </td>
              <td className="py-3 px-3 text-right font-mono">
                {fmtPct(row.total_return_pct)}
              </td>
              <td className="py-3 px-5 text-text-3 text-[12px]">
                {fmtTime(row.started_at)}
              </td>
              <td
                className="py-3 px-5 text-right"
                onClick={(e) => e.stopPropagation()}
              >
                <button
                  type="button"
                  onClick={() => remove.mutate(row.id)}
                  disabled={remove.variables === row.id && remove.isPending}
                  className="text-[12px] text-text-3 hover:text-danger disabled:opacity-50"
                >
                  {remove.variables === row.id && remove.isPending
                    ? "Deleting…"
                    : "Delete"}
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function StartEvalDialog({
  initialAgentId,
  onClose,
}: {
  initialAgentId: string;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const scenarios = useQuery({
    queryKey: scenarioKeys.list(),
    queryFn: () => listScenarios(),
  });
  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });
  const brokers = useQuery({
    queryKey: settingsKeys.brokers(),
    queryFn: getBrokers,
  });

  const [agentId, setAgentId] = useState<string>(initialAgentId);
  const [scenarioId, setScenarioId] = useState<string>("");
  const [mode, setMode] = useState<RunMode>("backtest");
  const [preflightError, setPreflightError] = useState<string | null>(null);

  useEffect(() => {
    setAgentId(initialAgentId);
  }, [initialAgentId]);

  const start = useMutation<RunDetail, unknown, StartRunReq>({
    mutationFn: startRun,
    onSuccess: (detail) => {
      qc.invalidateQueries({ queryKey: evalKeys.runs() });
      onClose();
      navigate(`/eval-runs/${encodeURIComponent(detail.summary.id)}`);
    },
  });

  const ready =
    agentId.length > 0 && scenarioId.length > 0 && !start.isPending;

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    const blocked = evalPreflightError({
      mode,
      providers,
      brokers,
    });
    if (blocked) {
      setPreflightError(blocked);
      return;
    }
    setPreflightError(null);
    start.mutate({
      agent_id: agentId,
      scenario_id: scenarioId,
      mode,
      params_override: null,
    });
  }

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center pt-24 px-4 bg-bg/80 backdrop-blur-sm"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="w-full max-w-md bg-surface border border-border rounded-lg shadow-xl"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Start eval"
      >
        <form onSubmit={onSubmit} className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-serif font-medium text-[20px] tracking-tight">
              Start eval
            </h2>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Picks a strategy + scenario, queues the run, and drops you
              on its detail page so you can watch progress.
            </p>
          </div>

          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              Strategy
            </label>
            <select
              aria-label="Strategy"
              value={agentId}
              onChange={(e) => {
                setAgentId(e.target.value);
                setPreflightError(null);
              }}
              disabled={strategies.isPending}
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono focus:outline-none focus:border-text-3"
            >
              <option value="">— pick a strategy —</option>
              {(strategies.data ?? []).map((s: StrategyListItem) => (
                <option key={s.agent_id} value={s.agent_id}>
                  {s.display_name} · {s.agent_id}
                </option>
              ))}
            </select>
            {strategies.isError ? (
              <p className="m-0 mt-1 text-[12px] text-rose-300">
                couldn't load strategies — try refreshing
              </p>
            ) : null}
          </div>

          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              Scenario
            </label>
            <select
              aria-label="Scenario"
              value={scenarioId}
              onChange={(e) => {
                setScenarioId(e.target.value);
                setPreflightError(null);
              }}
              disabled={scenarios.isPending}
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] focus:outline-none focus:border-text-3"
            >
              <option value="">— pick a scenario —</option>
              {(scenarios.data ?? []).map((s: Scenario) => (
                <option key={s.id} value={s.id}>
                  {s.display_name} · {scenarioWindowLabel(s)}
                </option>
              ))}
            </select>
            {scenarios.isError ? (
              <p className="m-0 mt-1 text-[12px] text-rose-300">
                couldn't load scenarios — try refreshing
              </p>
            ) : null}
          </div>

          <fieldset>
            <legend className="block text-[12px] text-text-2 mb-1.5 px-0">
              Mode
            </legend>
            <div className="flex items-center gap-3">
              <label className="inline-flex items-center gap-2 text-[13px] text-text-2">
                <input
                  type="radio"
                  name="mode"
                  value="paper"
                  checked={mode === "paper"}
                  onChange={() => {
                    setMode("paper");
                    setPreflightError(null);
                  }}
                  className="accent-gold"
                />
                paper
              </label>
              <label className="inline-flex items-center gap-2 text-[13px] text-text-2">
                <input
                  type="radio"
                  name="mode"
                  value="backtest"
                  checked={mode === "backtest"}
                  onChange={() => {
                    setMode("backtest");
                    setPreflightError(null);
                  }}
                  className="accent-gold"
                />
                backtest
              </label>
            </div>
            <p className="m-0 mt-1.5 text-[11px] text-text-3 leading-snug">
              Paper trades against Alpaca paper credentials (Settings → Brokers).
              Backtest replays the scenario's parquet fixture in-process.
            </p>
          </fieldset>

          {preflightError || start.isError ? (
            <p className="m-0 text-[12px] text-rose-300 font-mono">
              {preflightError ?? errorDetail(start.error)}
            </p>
          ) : null}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded text-[13px] text-text-2 hover:text-text"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!ready}
              className="px-3 py-1.5 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {start.isPending ? "Starting…" : "Start"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function evalPreflightError({
  mode,
  providers,
  brokers,
}: {
  mode: RunMode;
  providers: UseQueryResult<ProvidersReport>;
  brokers: UseQueryResult<BrokersReport>;
}): string | null {
  if (providers.isPending || brokers.isPending) {
    return "Still loading eval prerequisites.";
  }
  if (providers.isError) {
    return "Couldn't load LLM providers. Refresh and try again.";
  }
  if (brokers.isError) {
    return "Couldn't load broker settings. Refresh and try again.";
  }

  const rows = providers.data?.providers ?? [];
  const hasCredentialedProvider = rows.some((row) => {
    const noAuthProvider = row.api_key_env.trim().length === 0;
    return row.api_key_set || noAuthProvider;
  });
  if (!hasCredentialedProvider) {
    return "Add a provider/API key in Settings -> Providers before running eval.";
  }

  const hasEnabledModel = rows.some((row) => row.enabled_models.length > 0);
  const defaultModel = providers.data?.default_model;
  if (!hasEnabledModel && !defaultModel) {
    return "Enable a provider model in Settings -> Providers before running eval.";
  }

  const alpacaConfigured = brokers.data?.alpaca.configured === true;
  if (mode === "paper" && !alpacaConfigured) {
    return "Configure Alpaca paper credentials in Settings -> Brokers before running a paper eval.";
  }

  return null;
}

function scenarioWindowLabel(s: Scenario): string {
  const start = Date.parse(s.time_window.start);
  const end = Date.parse(s.time_window.end);
  if (Number.isNaN(start) || Number.isNaN(end) || end <= start) {
    return s.granularity;
  }
  const days = Math.max(1, Math.round((end - start) / 86_400_000));
  return `${s.granularity} · ${days}d`;
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}

function StatusPill({ status }: { status: string }) {
  const tone = STATUS_TONE[status] ?? "default";
  return (
    <Pill tone={tone}>
      <span className="w-1.5 h-1.5 rounded-full" style={dotColor(tone)} />
      {status}
    </Pill>
  );
}

function dotColor(tone: "gold" | "warn" | "danger" | "default" | "info") {
  return {
    gold: { background: "var(--gold)" },
    warn: { background: "var(--warn)" },
    danger: { background: "var(--danger)" },
    info: { background: "var(--info)" },
    default: { background: "var(--text-3)" },
  }[tone];
}

function fmtNumber(n: number | null | undefined): string {
  if (n == null) return "—";
  return n.toFixed(2);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-32 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-24 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-20 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-16 rounded bg-surface-elev animate-pulse" />
        </div>
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="px-6 py-16 text-center text-text-2">
      <div className="font-serif italic text-[28px] text-text-3 mb-3">
        no runs yet
      </div>
      <p className="m-0 max-w-md mx-auto leading-snug">
        Use the launcher above to start a run, or trigger one via{" "}
        <code className="text-text font-mono">xvn ab-compare</code>.
      </p>
    </div>
  );
}

function ErrorState({ err, onRetry }: { err: unknown; onRetry: () => void }) {
  const detail =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);
  return (
    <div className="px-6 py-12 text-center">
      <div className="font-serif italic text-[24px] text-danger mb-3">
        couldn't load runs
      </div>
      <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
        <code className="text-danger font-mono text-[12px]">{detail}</code>
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
