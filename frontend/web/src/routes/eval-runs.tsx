import { useEffect, useState, type FormEvent } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import {
  ListPagination,
  useListPagination,
} from "@/components/primitives/ListPagination";
import { ApiError } from "@/api/client";
import { chartKeys, getRunChart } from "@/api/chart";
import { RunChart } from "@/components/chart/RunChart";
import {
  evalKeys,
  cancelRun,
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
  type ProviderModelPair,
  type StrategyListItem,
} from "@/api/strategies";
import { isInflightRunStatus } from "@/lib/run-status";
import {
  displayScenarioName,
  displayStrategyName,
  evalRunDisambiguator,
} from "@/lib/run-display";
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
  const strategyFilter = searchParams.get("strategy")?.trim() ?? "";
  const q = useQuery({
    queryKey: evalKeys.runs({
      agent_id: strategyFilter || undefined,
    }),
    queryFn: () =>
      listRuns({
        agent_id: strategyFilter || undefined,
      }),
    // Poll while any run is still in flight; stop once everything's
    // terminal. Background eval tasks drive in the dashboard process
    // and can take minutes — without this the list looks frozen.
    refetchInterval: (query) => {
      const items = query.state.data as RunSummary[] | undefined;
      if (!items) return false;
      const inflight = items.some((r) => isInflightRunStatus(r.status));
      return inflight ? 2000 : false;
    },
  });
  const navigate = useNavigate();
  const preselectedStrategy = strategyFilter;
  const startRequested = searchParams.get("start") === "1";
  // Selection state for the Compare flow. Lifted here so the Topbar can
  // render the action button next to the run count.
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [startOpen, setStartOpen] = useState(startRequested);
  const hasInflight =
    q.data?.some((r) => isInflightRunStatus(r.status)) ??
    false;
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (startRequested) {
      setStartOpen(true);
    }
  }, [startRequested]);
  useEffect(() => {
    if (!hasInflight) {
      setNowMs(Date.now());
      return;
    }
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [hasInflight]);
  // Page-size + page-nav state (QA-round-7 list wave / F-4). Operates on
  // the already-fetched, server-sorted run list — the engine returns runs
  // most-recent-first (eval/store.rs ORDER BY started_at DESC), so a
  // simple client-side slice gives the user a page-size picker without
  // teaching every backend endpoint to paginate. The unified list
  // component planned in team/intake/2026-05-19-list-component-design-intake.md
  // will swap this for proper server-side pagination later.
  const runs = q.data ?? [];
  const pagination = useListPagination(runs);
  const latestRunId = runs[0]?.id ?? "";
  const latestChart = useQuery({
    queryKey: chartKeys.run(latestRunId),
    queryFn: () => getRunChart(latestRunId),
    enabled: !!latestRunId,
  });
  const strategiesQ = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const scenariosQ = useQuery({
    queryKey: scenarioKeys.list(),
    queryFn: () => listScenarios(),
  });
  const strategyFilterLabel = strategyFilter
    ? displayStrategyName(strategyFilter, strategiesQ.data ?? [])
    : "";

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
      <Topbar title="Eval" sub={subtitleFor(q, strategyFilterLabel)} />

      <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
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
          className="inline-flex w-full items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:w-auto"
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

      {strategyFilter ? (
        <div className="mb-3 px-3 py-2 rounded border border-border-soft bg-surface-elev text-[12px] text-text-2 flex items-center justify-between gap-2">
          <span>
            Filtering runs for strategy{" "}
            <span className="text-text">{strategyFilterLabel}</span>
            <code
              className="ml-2 font-mono text-text-3 break-all"
              title={strategyFilter}
            >
              {strategyFilter}
            </code>
          </span>
          <button
            type="button"
            onClick={() => {
              const next = new URLSearchParams(searchParams);
              next.delete("strategy");
              setSearchParams(next);
            }}
            className="text-text-3 hover:text-text underline-offset-2 hover:underline"
          >
            Clear filter
          </button>
        </div>
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
            items={pagination.visible}
            allItems={runs}
            selected={selected}
            onToggle={toggleSelected}
            nowMs={nowMs}
            strategies={strategiesQ.data ?? []}
            scenarios={scenariosQ.data ?? []}
          />
        )}
      </Card>

      <ListPagination
        total={pagination.total}
        page={pagination.page}
        pageSize={pagination.pageSize}
        onPageChange={pagination.setPage}
        onPageSizeChange={pagination.setPageSize}
        itemLabel="runs"
      />

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

function subtitleFor(q: ReturnType<typeof useQuery>, strategyFilterLabel: string) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load runs";
  const data = q.data as { length: number } | undefined;
  if (!data) return "";
  const n = data.length;
  const base = `${n} ${n === 1 ? "run" : "runs"}`;
  return strategyFilterLabel ? `${base} for ${strategyFilterLabel}` : base;
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
    <div className="flex flex-wrap items-center justify-end gap-2">
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
  allItems,
  selected,
  onToggle,
  nowMs,
  strategies,
  scenarios,
}: {
  items: RunSummary[];
  /** Full unpaginated set — used as the sibling pool for the
   *  evalRunDisambiguator ordinal so "Run #3/7" stays stable across
   *  pages instead of resetting per-page. */
  allItems: RunSummary[];
  selected: Set<string>;
  onToggle: (id: string) => void;
  nowMs: number;
  strategies: StrategyListItem[];
  scenarios: Scenario[];
}) {
  const strategyName = (id: string) => displayStrategyName(id, strategies);
  const scenarioName = (id: string) => displayScenarioName(id, scenarios);
  const navigate = useNavigate();
  const qc = useQueryClient();
  const remove = useMutation({
    mutationFn: deleteRun,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evalKeys.all });
    },
  });
  const cancel = useMutation({
    mutationFn: cancelRun,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evalKeys.all });
    },
  });

  function go(id: string) {
    navigate(`/eval-runs/${id}`);
  }

  return (
    <>
      <div className="divide-y divide-border-soft md:hidden">
        {items.map((row) => {
          const isChecked = selected.has(row.id);
          return (
            <article
              key={row.id}
              className="px-4 py-3"
              role="link"
              tabIndex={0}
              onClick={() => go(row.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  go(row.id);
                }
              }}
            >
              <div className="mb-2 flex items-start justify-between gap-2">
                <label
                  className="inline-flex items-center gap-2 text-[12px] text-text-2"
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
                    className="cursor-pointer accent-gold"
                  />
                  Select
                </label>
                <StatusPill status={row.status} />
              </div>

              <div className="text-[14px] text-text font-medium truncate">
                {strategyName(row.agent_id)}
              </div>
              <div className="mt-1 text-[12px] text-text-2 truncate">
                {scenarioName(row.scenario_id)}
              </div>
              <div className="mt-1 font-mono text-[11px] text-text-3">
                <span className="text-text-2">
                  {evalRunDisambiguator(row, allItems)}
                </span>
                <span className="mx-1.5 text-text-4">·</span>
                <span>{row.mode}</span>
              </div>
              <div
                className="mt-1 font-mono text-[11px] text-text-3 break-all select-all"
                aria-label={`Run id ${row.id}`}
              >
                {row.id}
              </div>
              <div className="mt-2 grid grid-cols-2 gap-2 text-[12px] min-[420px]:grid-cols-5">
                <div className="text-text-2">
                  <div className="text-[11px] text-text-3">Sharpe</div>
                  <div className="font-mono text-text">{fmtNumber(row.sharpe)}</div>
                </div>
                <div className="text-text-2">
                  <div className="text-[11px] text-text-3">Max DD</div>
                  <div className="font-mono text-text">{fmtPct(row.max_drawdown_pct)}</div>
                </div>
                <div className="text-text-2">
                  <div className="text-[11px] text-text-3">Return</div>
                  <div className="font-mono text-text">{fmtPct(row.total_return_pct)}</div>
                </div>
                <div className="text-text-2">
                  <div className="text-[11px] text-text-3">Duration</div>
                  <div className="font-mono text-text">
                    {fmtDuration(row.started_at, row.completed_at, nowMs)}
                  </div>
                </div>
                <div className="text-text-2">
                  <div className="text-[11px] text-text-3">Tokens</div>
                  <div className="font-mono text-text">{fmtTokens(row)}</div>
                </div>
              </div>

              <div
                className="mt-2 flex justify-end gap-3"
                onClick={(e) => e.stopPropagation()}
              >
                {isInflight(row) ? (
                  <button
                    type="button"
                    aria-label={`Cancel run ${row.id}`}
                    onClick={() => cancel.mutate(row.id)}
                    disabled={cancel.variables === row.id && cancel.isPending}
                    className="text-[12px] text-warn hover:text-text disabled:opacity-50"
                  >
                    {cancel.variables === row.id && cancel.isPending
                      ? "Cancelling..."
                      : "Cancel"}
                  </button>
                ) : null}
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
              </div>
            </article>
          );
        })}
      </div>

      <div className="relative hidden md:block">
        <div className="overflow-x-auto">
        <table
          data-testid="eval-runs-desktop-table"
          className="min-w-[980px] w-full"
        >
          <thead>
            <tr className="border-b border-border-soft text-left text-[12px] text-text-2">
              <th className="w-8 py-2.5 pl-5 pr-2 font-normal"></th>
              <th className="px-3 py-2.5 font-normal">Run</th>
              <th className="px-3 py-2.5 font-normal">Scenario</th>
              <th className="px-3 py-2.5 font-normal">Mode</th>
              <th className="px-3 py-2.5 font-normal">Status</th>
              <th className="px-3 py-2.5 text-right font-normal">Sharpe</th>
              <th className="px-3 py-2.5 text-right font-normal">Max DD</th>
              <th className="px-3 py-2.5 text-right font-normal">Return</th>
              <th className="px-3 py-2.5 text-right font-normal">Tokens</th>
              <th className="px-3 py-2.5 text-right font-normal">Duration</th>
              <th className="px-5 py-2.5 font-normal">Started</th>
              <th className="px-5 py-2.5 text-right font-normal"></th>
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
                  className="cursor-pointer border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover focus:bg-surface-hover focus:outline-none"
                >
                  <td
                    className="w-8 py-3 pl-5 pr-2"
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
                      className="cursor-pointer accent-gold"
                    />
                  </td>
                  <td className="px-3 py-3">
                    <div className="text-[13px] text-text font-medium">
                      {strategyName(row.agent_id)}
                    </div>
                    <div className="mt-0.5 text-[11px] text-text-2">
                      {evalRunDisambiguator(row, allItems)}
                    </div>
                    <div
                      className="mt-0.5 font-mono text-[11px] text-text-3 break-all select-all"
                      aria-label={`Run id ${row.id}`}
                    >
                      {row.id}
                    </div>
                  </td>
                  <td className="px-3 py-3 text-text-2">{scenarioName(row.scenario_id)}</td>
                  <td className="px-3 py-3 text-text-2">{row.mode}</td>
                  <td className="px-3 py-3">
                    <StatusPill status={row.status} />
                  </td>
                  <td className="px-3 py-3 text-right font-mono">
                    {fmtNumber(row.sharpe)}
                  </td>
                  <td className="px-3 py-3 text-right font-mono">
                    {fmtPct(row.max_drawdown_pct)}
                  </td>
                  <td className="px-3 py-3 text-right font-mono">
                    {fmtPct(row.total_return_pct)}
                  </td>
                  <td className="px-3 py-3 text-right font-mono">
                    {fmtTokens(row)}
                  </td>
                  <td className="px-3 py-3 text-right font-mono">
                    {fmtDuration(row.started_at, row.completed_at, nowMs)}
                  </td>
                  <td className="px-5 py-3 text-[12px] text-text-3">
                    {fmtTime(row.started_at)}
                  </td>
                  <td
                    className="px-5 py-3 text-right"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <div className="flex justify-end gap-3">
                      {isInflight(row) ? (
                        <button
                          type="button"
                          aria-label={`Cancel run ${row.id}`}
                          onClick={() => cancel.mutate(row.id)}
                          disabled={cancel.variables === row.id && cancel.isPending}
                          className="text-[12px] text-warn hover:text-text disabled:opacity-50"
                        >
                          {cancel.variables === row.id && cancel.isPending
                            ? "Cancelling..."
                            : "Cancel"}
                        </button>
                      ) : null}
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
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        </div>
        {/*
          Edge fade gradient hints at horizontal overflow without
          painting over the rightmost column when there's no overflow.
          The gradient color uses the card surface so it blends in both
          light and dark themes (per the dark-mode borders rule in
          CLAUDE.md — no hard whites here).
        */}
        <div
          aria-hidden
          data-testid="eval-runs-scroll-fade"
          className="pointer-events-none absolute inset-y-0 right-0 w-8"
          style={{
            background:
              "linear-gradient(to right, transparent, var(--surface-card))",
          }}
        />
      </div>
    </>
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
  const selectedStrategy = (strategies.data ?? []).find(
    (s) => s.agent_id === agentId,
  );
  const displayedError =
    preflightError ?? (start.isError ? errorDetail(start.error) : null);
  const setupAction = preflightSetupAction(displayedError);

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    const blocked = evalPreflightError({
      mode,
      providers,
      brokers,
      strategy: selectedStrategy,
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
                  {s.display_name || "Untitled strategy"}
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

          {displayedError ? (
            <div className="space-y-2">
              <p className="m-0 text-[12px] text-rose-300 font-mono">
                {displayedError}
              </p>
              {setupAction ? (
                <Link
                  to={setupAction.to}
                  className="inline-flex items-center justify-center rounded border border-border px-3 py-1.5 text-[12px] text-text-2 hover:border-text-3 hover:text-text"
                >
                  {setupAction.label}
                </Link>
              ) : null}
            </div>
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
  strategy,
}: {
  mode: RunMode;
  providers: UseQueryResult<ProvidersReport>;
  brokers: UseQueryResult<BrokersReport>;
  strategy?: StrategyListItem;
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
  if (!hasEnabledModel) {
    return "Enable a provider model in Settings -> Providers before running eval.";
  }

  if (strategy) {
    const requiredPairs = requiredRuntimePairs(strategy);
    if (requiredPairs.length === 0) {
      return "Pick a provider/model for the strategy agent before running eval.";
    }
    for (const pair of requiredPairs) {
      const row = rows.find((candidate) => candidate.name === pair.provider);
      if (!row) {
        return `provider '${pair.provider}' is not configured. Pick a configured provider/model for the strategy agent before running eval.`;
      }
      const noAuthProvider = row.api_key_env.trim().length === 0;
      if (!row.api_key_set && !noAuthProvider) {
        return `provider '${pair.provider}' has no API key set. Add it in Settings -> Providers before running eval.`;
      }
      if (!row.enabled_models.includes(pair.model)) {
        return `model '${pair.model}' is not enabled for provider '${pair.provider}'. Enable it in Settings -> Providers before running eval.`;
      }
    }
  }

  const alpacaConfigured = brokers.data?.alpaca.configured === true;
  if (mode === "paper" && !alpacaConfigured) {
    return "Configure Alpaca paper credentials in Settings -> Brokers before running a paper eval.";
  }

  return null;
}

function requiredRuntimePairs(strategy: StrategyListItem): ProviderModelPair[] {
  if ((strategy.provider_models?.length ?? 0) > 0) {
    return (strategy.provider_models ?? []).reduce<ProviderModelPair[]>((acc, pair) => {
      const provider = pair.provider.trim();
      const model = pair.model.trim();
      if (provider && model) {
        acc.push({ provider, model });
      }
      return acc;
    }, []);
  }

  const providers = strategy.providers ?? [];
  const models = strategy.models ?? [];
  const pairs: ProviderModelPair[] = [];
  const count = Math.min(providers.length, models.length);
  for (let i = 0; i < count; i++) {
    const provider = providers[i]?.trim();
    const model = models[i]?.trim();
    if (provider && model) {
      pairs.push({ provider, model });
    }
  }
  return pairs;
}

function preflightSetupAction(
  error: string | null,
): { to: string; label: string } | null {
  if (!error) return null;
  if (error.includes("Settings -> Providers") || error.includes("Settings → Providers")) {
    return { to: "/settings/providers", label: "Settings -> Providers" };
  }
  if (error.includes("Settings -> Brokers") || error.includes("Settings → Brokers")) {
    return { to: "/settings/brokers", label: "Settings -> Brokers" };
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
    <Pill tone={tone} animated={isInflightRunStatus(status)}>
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

function isInflight(row: RunSummary): boolean {
  return isInflightRunStatus(row.status);
}

function fmtTokens(row: RunSummary): string {
  const total =
    (row.actual_input_tokens ?? 0) + (row.actual_output_tokens ?? 0);
  return total > 0 ? total.toLocaleString() : "—";
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

function fmtDuration(
  startedAt: string,
  completedAt: string | null | undefined,
  nowMs: number,
): string {
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return "—";
  const end = completedAt ? Date.parse(completedAt) : nowMs;
  if (Number.isNaN(end)) return "—";
  const totalSeconds = Math.max(0, Math.floor((end - start) / 1000));
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) return `${totalMinutes}m`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
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
