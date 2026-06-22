import { useEffect, useMemo, useState, type FormEvent } from "react";
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
import { StrategyPicker } from "@/components/primitives/StrategyPicker";
import { SignalSearchableSelectMenu } from "@/components/primitives/SignalMenu";
import {
  ServerPagerStrip,
  useServerPagination,
} from "@/components/primitives/useServerPagination";
import {
  ResponsiveListCard,
  useListState,
  useListColumns,
  useListUrlState,
  type FilterDef,
  type SortOption,
} from "@/components/lists";
import { MListRow } from "@/components/lists/MListRow";
import { ApiError } from "@/api/client";
import { chartKeys, getRunChart } from "@/api/chart";
import { RunChartV2 } from "@/components/chart/v2/surfaces/RunChartV2";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
import {
  evalKeys,
  cancelRun,
  deleteRun,
  listRunsPaged,
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
import { drawdownToneClass } from "@/lib/metric-tone";
import { isProviderConfigured } from "@/lib/providers";
import {
  displayScenarioName,
  displayStrategyName,
  evalRunDisambiguator,
} from "@/lib/run-display";
import type {
  BrokersReport,
  LiveConfig,
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

const SORT_OPTIONS: SortOption[] = [
  { value: "started", label: "Recently started" },
  { value: "completed", label: "Recently completed" },
  { value: "strategy", label: "Strategy A → Z" },
  { value: "status", label: "Status" },
];

const MODE_FILTER: FilterDef = {
  id: "mode",
  label: "Mode",
  options: [
    { value: "all", label: "All modes" },
    { value: "backtest", label: "Backtest" },
    { value: "live", label: "Forward test" },
  ],
};

const STATUS_FILTER: FilterDef = {
  id: "status",
  label: "Status",
  options: [
    { value: "all", label: "All statuses" },
    { value: "completed", label: "Completed" },
    { value: "running", label: "Running" },
    { value: "queued", label: "Queued" },
    { value: "failed", label: "Failed" },
    { value: "cancelled", label: "Cancelled" },
  ],
};

export function EvalRunsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const strategyFilterUrl = searchParams.get("strategy")?.trim() ?? "";
  // QA-round-7 backend-pagination follow-up (#386 gap): `limit`/`offset`
  // drive the query key so each page change is a fresh request.
  const [totalFromServer, setTotalFromServer] = useState(0);
  const pager = useServerPagination(totalFromServer);
  const listParams = {
    agent_id: strategyFilterUrl || undefined,
    limit: pager.limit,
    offset: pager.offset,
  };
  const q = useQuery({
    queryKey: evalKeys.runs(listParams),
    queryFn: () => listRunsPaged(listParams),
    placeholderData: (prev) => prev,
    // Poll while any run on the current page is still in flight; stop
    // once everything visible is terminal. Background eval tasks drive
    // in the dashboard process and can take minutes — without this the
    // list looks frozen.
    refetchInterval: (query) => {
      const data = query.state.data as { items?: RunSummary[] } | undefined;
      const items = data?.items;
      if (!items) return false;
      const inflight = items.some((r) => isInflightRunStatus(r.status));
      return inflight ? 2000 : false;
    },
  });
  useEffect(() => {
    if (q.data?.total !== undefined && q.data.total !== totalFromServer) {
      setTotalFromServer(q.data.total);
    }
  }, [q.data?.total, totalFromServer]);
  const navigate = useNavigate();
  const preselectedStrategy = strategyFilterUrl;
  const startRequested = searchParams.get("start") === "1";
  // Selection state for the Compare flow. Lifted here so the Topbar can
  // render the action button next to the run count.
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [startOpen, setStartOpen] = useState(startRequested);
  const runs = q.data?.items ?? [];
  const hasInflight = runs.some((r) => isInflightRunStatus(r.status));
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
  // "Latest run chart" still wants the first row of the current page.
  // When the user is on page 1 of the recency-sorted list that's
  // identical to the previous behavior (the engine returns runs
  // ORDER BY started_at DESC, id DESC). When they paginate further
  // it surfaces the newest run of THAT page — acceptable for v1; the
  // unified-list-component spec will move "latest run" to its own
  // dedicated query.
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
  const strategies = strategiesQ.data ?? [];
  const scenarios = scenariosQ.data ?? [];

  // Strategy filter options are derived from the loaded strategies plus
  // any agent_id observed on the current page that isn't already in the
  // strategy library (defensive against stale strategy lists).
  const strategyFilter: FilterDef = useMemo(() => {
    const known = new Set<string>();
    const options: { value: string; label: string }[] = [
      { value: "all", label: "All strategies" },
    ];
    strategies.forEach((s) => {
      if (s.agent_id && !known.has(s.agent_id)) {
        known.add(s.agent_id);
        options.push({
          value: s.agent_id,
          label: s.display_name || s.agent_id,
        });
      }
    });
    runs.forEach((r) => {
      if (r.agent_id && !known.has(r.agent_id)) {
        known.add(r.agent_id);
        options.push({
          value: r.agent_id,
          label: displayStrategyName(r.agent_id, strategies),
        });
      }
    });
    return { id: "strategy", label: "Strategy", options };
  }, [strategies, runs]);

  const list = useListState<RunSummary>({
    rows: runs,
    filters: [strategyFilter, MODE_FILTER, STATUS_FILTER],
    sortOptions: SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const strategyVal = values.strategy ?? "all";
      if (strategyVal !== "all" && row.agent_id !== strategyVal) return false;
      const modeVal = values.mode ?? "all";
      if (modeVal !== "all" && row.mode !== modeVal) return false;
      const statusVal = values.status ?? "all";
      if (statusVal !== "all" && row.status !== statusVal) return false;
      const q = query.trim().toLowerCase();
      if (q.length === 0) return true;
      const name = displayStrategyName(row.agent_id, strategies).toLowerCase();
      const scenarioName = displayScenarioName(
        row.scenario_id,
        scenarios,
      ).toLowerCase();
      const shortId = row.id.slice(0, 8).toLowerCase();
      return name.includes(q) || scenarioName.includes(q) || shortId.includes(q);
    },
    sortFn: (rows, key) => {
      switch (key) {
        case "completed":
          return rows.sort((a, b) =>
            compareIsoDesc(a.completed_at, b.completed_at),
          );
        case "strategy":
          return rows.sort((a, b) =>
            displayStrategyName(a.agent_id, strategies).localeCompare(
              displayStrategyName(b.agent_id, strategies),
            ),
          );
        case "status":
          return rows.sort((a, b) => a.status.localeCompare(b.status));
        case "started":
        default:
          return rows.sort((a, b) => compareIsoDesc(a.started_at, b.started_at));
      }
    },
  });
  useListUrlState("eval-runs", list);

  // Bridge the strategy filter ↔ ?strategy= URL param so the existing
  // backend `agent_id` query keeps working. The unified `useListUrlState`
  // hook owns the URL writes; this effect only enforces the back-edge
  // (when the strategy filter changes, mirror it onto ?strategy=).
  useEffect(() => {
    const next = new URLSearchParams(searchParams);
    const val = list.filters.find((f) => f.def.id === "strategy")?.value;
    if (val && val !== "all") {
      if (next.get("strategy") !== val) next.set("strategy", val);
    } else {
      if (next.has("strategy")) next.delete("strategy");
    }
    if (next.toString() !== searchParams.toString()) {
      setSearchParams(next, { replace: true });
    }
  }, [list.filters, searchParams, setSearchParams]);

  // Also hydrate from ?strategy= on first mount in case the URL had it
  // before useListUrlState saw it (?q= takes precedence already).
  useEffect(() => {
    if (!strategyFilterUrl) return;
    const f = list.filters.find((ff) => ff.def.id === "strategy");
    if (!f) return;
    if (f.value !== strategyFilterUrl) {
      f.setValue(strategyFilterUrl);
    }
    // run-once on initial mount + when strategyFilterUrl changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [strategyFilterUrl]);

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

  const subtitle = subtitleFor(q, q.data?.total ?? 0, list.rows.length);

  const desktopColumns = [
    { key: "select",   label: "",          width: 32,    essential: true,  estWidth: 42 },
    { key: "run",      label: "Run",                     essential: true,  estWidth: 210 },
    { key: "strategy", label: "Strategy",                essential: true,  estWidth: 160 },
    { key: "scenario", label: "Scenario",                essential: true,  estWidth: 150 },
    { key: "status",   label: "Status",                  essential: true,  estWidth: 100 },
    { key: "return",   label: "Return",  align: "right" as const, essential: true, estWidth: 90 },
    { key: "sharpe",   label: "Sharpe",  align: "right" as const, essential: true, estWidth: 90 },
    { key: "drawdown", label: "Max DD",  align: "right" as const, priority: 4, estWidth: 90 },
    { key: "mode",     label: "Mode",                    priority: 3,      estWidth: 90 },
    { key: "tokens",   label: "Tokens",  align: "right" as const, priority: 2, estWidth: 80 },
    { key: "duration", label: "Duration",align: "right" as const, priority: 1, estWidth: 90 },
    { key: "started",  label: "Started",                 priority: 0,      estWidth: 130 },
    { key: "actions",  label: "",                        essential: true,  estWidth: 80 },
  ];
  const columnState = useListColumns("eval-runs", desktopColumns);

  return (
    <>
      <Topbar title="Eval" sub={subtitle} />

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
          className="inline-flex w-full items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:w-auto motion-safe:active:scale-[0.96]"
        >
          <Icon name="plus" size={13} /> Start eval
        </button>
      </div>
      {startOpen ? (
        <>
          <StartEvalPanel
            initialAgentId={preselectedStrategy}
            onClose={() => {
              setStartOpen(false);
              const next = new URLSearchParams(searchParams);
              next.delete("start");
              setSearchParams(next);
            }}
          />
          <hr className="border-border-soft" />
        </>
      ) : null}

      <ResponsiveListCard<RunSummary>
        listId="eval-runs"
        title="Runs"
        count={q.data?.total ?? 0}
        toolbar={{
          search: { ...list.search, placeholder: "Search runs…" },
          filters: list.filters,
          sort: list.sort,
          clearAll: list.clearAll,
        }}
        columns={desktopColumns}
        columnState={columnState}
        rows={list.rows}
        loading={q.isPending}
        error={
          q.isError
            ? {
                message: errorDetail(q.error),
                retry: () => q.refetch(),
              }
            : null
        }
        empty="No runs match these filters."
        emptyAction={
          <button
            type="button"
            onClick={() => setStartOpen(true)}
            className="inline-flex items-center gap-1.5 rounded border border-gold px-3 py-1.5 text-[12px] font-medium text-gold hover:bg-gold/10"
          >
            <Icon name="plus" size={11} /> Start eval
          </button>
        }
        renderRow={(row, _i, visibleKeys) => (
          <DesktopRow
            key={row.id}
            row={row}
            allRows={runs}
            visibleKeys={visibleKeys}
            isChecked={selected.has(row.id)}
            onToggle={toggleSelected}
            onGo={go}
            onDelete={(id) => remove.mutate(id)}
            onCancel={(id) => cancel.mutate(id)}
            deletePending={remove.variables === row.id && remove.isPending}
            cancelPending={cancel.variables === row.id && cancel.isPending}
            nowMs={nowMs}
            strategies={strategies}
            scenarios={scenarios}
          />
        )}
        renderMobileRow={(row) => (
          <MListRow
            key={row.id}
            onClick={() => go(row.id)}
            title={displayStrategyName(row.agent_id, strategies)}
            badge={row.status}
            badgeColor={badgeColorFor(row.status)}
            subtitle={displayScenarioName(row.scenario_id, scenarios)}
            meta={`${evalRunDisambiguator(row, runs)} · ${row.mode}`}
            rightTop={fmtPct(row.total_return_pct)}
            rightSub={fmtDuration(row.started_at, row.completed_at, nowMs, row.status)}
            rightTone={signedTone(row.total_return_pct)}
          />
        )}
      />

      <ServerPagerStrip
        total={q.data?.total ?? 0}
        page={pager.page}
        pageSize={pager.pageSize}
        onPageChange={pager.setPage}
        onPageSizeChange={pager.setPageSize}
        itemLabel="runs"
      />

      <h2 className="font-sans font-semibold text-[20px] text-text mt-8 mb-3">
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
          <RunChartV2 payload={runChartPayloadToV2(latestChart.data)} />
        ) : null}
      </Card>
    </>
  );
}

// ── Existing helpers ───────────────────────────────────────────────────────

function subtitleFor(
  q: { isPending: boolean; isError: boolean; data?: { total?: number } },
  totalRows: number,
  visibleRows: number,
): string {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load runs";
  if (totalRows === 0) return "0 runs";
  if (visibleRows === totalRows) {
    return `${totalRows} ${totalRows === 1 ? "run" : "runs"}`;
  }
  return `${visibleRows} of ${totalRows} runs`;
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

function DesktopRow({
  row,
  allRows,
  visibleKeys,
  isChecked,
  onToggle,
  onGo,
  onDelete,
  onCancel,
  deletePending,
  cancelPending,
  nowMs,
  strategies,
  scenarios,
}: {
  row: RunSummary;
  allRows: RunSummary[];
  visibleKeys: Set<string>;
  isChecked: boolean;
  onToggle: (id: string) => void;
  onGo: (id: string) => void;
  onDelete: (id: string) => void;
  onCancel: (id: string) => void;
  deletePending: boolean;
  cancelPending: boolean;
  nowMs: number;
  strategies: StrategyListItem[];
  scenarios: Scenario[];
}) {
  return (
    <tr
      onClick={() => onGo(row.id)}
      className="xvn-row-in cursor-pointer border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover focus-within:bg-surface-hover"
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
        <Link
          to={`/eval-runs/${row.id}`}
          onClick={(e) => e.stopPropagation()}
          className="text-[11px] text-text-2 hover:underline"
        >
          {evalRunDisambiguator(row, allRows)}
        </Link>
        <div
          className="mt-0.5 font-mono text-[11px] text-text-3 break-all select-all"
          aria-label={`Run id ${row.id}`}
        >
          {row.id}
        </div>
      </td>
      <td className="px-3 py-3" onClick={(e) => e.stopPropagation()}>
        <Link
          to={`/strategies/${row.agent_id}`}
          className="text-[13px] text-text-2 hover:text-text hover:underline"
          onClick={(e) => e.stopPropagation()}
        >
          {displayStrategyName(row.agent_id, strategies)}
        </Link>
      </td>
      <td className="px-3 py-3 text-text-2">
        {displayScenarioName(row.scenario_id, scenarios)}
      </td>
      <td className="px-3 py-3">
        <StatusPill status={row.status} />
      </td>
      <td
        className={`px-3 py-3 text-right font-mono ${signedToneClass(row.total_return_pct)}`}
      >
        {fmtPct(row.total_return_pct)}
      </td>
      <td
        className={`px-3 py-3 text-right font-mono ${signedToneClass(row.sharpe)}`}
      >
        {fmtNumber(row.sharpe)}
      </td>
      {visibleKeys.has("drawdown") ? (
        <td
          className={`px-3 py-3 text-right font-mono ${drawdownToneClass(row.max_drawdown_pct)}`}
        >
          {fmtPct(row.max_drawdown_pct)}
        </td>
      ) : null}
      {visibleKeys.has("mode") ? (
        <td className="px-3 py-3 text-text-2">{row.mode}</td>
      ) : null}
      {visibleKeys.has("tokens") ? (
        <td className="px-3 py-3 text-right font-mono">{fmtTokens(row)}</td>
      ) : null}
      {visibleKeys.has("duration") ? (
        <td className="px-3 py-3 text-right font-mono">
          {fmtDuration(row.started_at, row.completed_at, nowMs, row.status)}
        </td>
      ) : null}
      {visibleKeys.has("started") ? (
        <td className="px-5 py-3 text-[12px] text-text-3">
          {fmtTime(row.started_at)}
        </td>
      ) : null}
      <td
        className="px-5 py-3 text-right"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex justify-end gap-3">
          {isInflight(row) ? (
            <button
              type="button"
              aria-label={`Cancel run ${row.id}`}
              onClick={() => onCancel(row.id)}
              disabled={cancelPending}
              className="text-[12px] text-warn hover:text-text disabled:opacity-50"
            >
              {cancelPending ? "Cancelling..." : "Cancel"}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => onDelete(row.id)}
            disabled={deletePending}
            className="text-[12px] text-text-3 hover:text-danger disabled:opacity-50"
          >
            {deletePending ? "Deleting…" : "Delete"}
          </button>
        </div>
      </td>
    </tr>
  );
}

function StartEvalPanel({
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
  const [liveAsset, setLiveAsset] = useState("BTC/USD");
  const [liveCapital, setLiveCapital] = useState("10000");
  const [liveBarLimit, setLiveBarLimit] = useState("5");
  const [liveWarmupBars, setLiveWarmupBars] = useState("200");
  // Forward-test execution venue. Values are the backend `broker_creds_ref`
  // contract (resolve_live_venue): "alpaca" (paper), "orderly_testnet",
  // "byreal", "degen_arena". All are paper/testnet — real money lives on the
  // /live surface (Degen Arena mainnet additionally gates on DEGEN_ALLOW_MAINNET).
  const [brokerCredsRef, setBrokerCredsRef] = useState<
    "alpaca" | "orderly_testnet" | "byreal" | "degen_arena"
  >("alpaca");
  const [autoFireReview, setAutoFireReview] = useState<boolean>(false);
  const [reviewProvider, setReviewProvider] = useState<string>("");
  const [reviewModel, setReviewModel] = useState<string>("");
  const [preflightError, setPreflightError] = useState<string | null>(null);

  useEffect(() => {
    setAgentId(initialAgentId);
  }, [initialAgentId]);

  const start = useMutation<RunDetail, unknown, StartRunReq>({
    mutationFn: startRun,
    onSuccess: (detail) => {
      qc.invalidateQueries({ queryKey: evalKeys.runs() });
      qc.setQueryData(evalKeys.run(detail.summary.id), detail);
      onClose();
      navigate(`/eval-runs/${encodeURIComponent(detail.summary.id)}`);
    },
  });

  const ready = agentId.length > 0 && !start.isPending;
  const selectedStrategy = (strategies.data ?? []).find(
    (s) => s.agent_id === agentId,
  );
  const selectedStrategyAssets =
    selectedStrategy?.asset_universe?.filter((asset) => asset.trim().length > 0) ?? [];
  const selectedStrategyFirstAsset = selectedStrategyAssets[0];
  const effectiveLiveAsset = selectedStrategyFirstAsset ?? liveAsset;
  const reviewProviderRows = (providers.data?.providers ?? []).filter(
    (row) => row.enabled_models.length > 0 && isProviderConfigured(row),
  );
  const activeReviewProvider =
    reviewProviderRows.find((row) => row.name === reviewProvider) ??
    reviewProviderRows[0];
  const activeReviewModel =
    (reviewModel && activeReviewProvider?.enabled_models.includes(reviewModel))
      ? reviewModel
      : (activeReviewProvider?.enabled_models[0] ?? "");
  const displayedError =
    preflightError ?? (start.isError ? errorDetail(start.error) : null);
  const setupAction = preflightSetupAction(displayedError);

  useEffect(() => {
    if (selectedStrategyFirstAsset && selectedStrategyFirstAsset !== liveAsset) {
      setLiveAsset(selectedStrategyFirstAsset);
    }
  }, [liveAsset, selectedStrategyFirstAsset]);

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!agentId) {
      setPreflightError("Pick a strategy before starting eval.");
      return;
    }
    if (mode === "backtest" && !scenarioId) {
      setPreflightError("Pick a scenario before starting Backtest.");
      return;
    }
    if (mode === "live") {
      const capital = Number(liveCapital);
      const barLimit = Number(liveBarLimit);
      const warmupBars = Number(liveWarmupBars);
      if (!effectiveLiveAsset.trim()) {
        setPreflightError("Select a strategy with an asset before starting a Forward test.");
        return;
      }
      if (!Number.isFinite(capital) || capital <= 0) {
        setPreflightError("Enter a positive live capital amount.");
        return;
      }
      if (!Number.isFinite(barLimit) || barLimit <= 0) {
        setPreflightError("Enter a positive live bar limit.");
        return;
      }
      if (!Number.isFinite(warmupBars) || warmupBars < 0) {
        setPreflightError("Enter a non-negative live warmup bar count.");
        return;
      }
      // Alpaca supplies the live market-data bar stream for most venues, so it
      // is required — EXCEPT Degen Arena, which sources its own Hyperliquid
      // candles (mirrors the engine's `uses_alpaca_data = venue != DegenArena`).
      const usesAlpacaData = brokerCredsRef !== "degen_arena";
      if (usesAlpacaData && brokers.data?.alpaca.configured !== true) {
        setPreflightError(
          "Configure Alpaca paper credentials in Settings -> Brokers before starting a Forward test.",
        );
        return;
      }
      // The execution venue's own credentials must also be configured.
      if (brokerCredsRef === "orderly_testnet" && brokers.data?.orderly.configured !== true) {
        setPreflightError(
          "Configure Orderly testnet credentials (ORDERLY_*) before starting a Forward test on Orderly.",
        );
        return;
      }
      if (brokerCredsRef === "byreal" && brokers.data?.byreal.configured !== true) {
        setPreflightError(
          "Configure Byreal credentials (BYREAL_PRIVATE_KEY) with BYREAL_NETWORK=testnet before starting a Forward test on Byreal.",
        );
        return;
      }
      if (brokerCredsRef === "degen_arena" && brokers.data?.degen_arena.configured !== true) {
        setPreflightError(
          "Configure Degen Arena credentials in Settings -> Brokers before starting a Forward test on Degen Arena.",
        );
        return;
      }
    }
    const blocked = evalPreflightError({
      providers,
      brokers,
      strategy: selectedStrategy,
    });
    if (blocked) {
      setPreflightError(blocked);
      return;
    }
    setPreflightError(null);
    const capitalNum = Number(liveCapital);
    const barLimitNum = Number(liveBarLimit);
    const warmupBarsNum = Number(liveWarmupBars);
    const liveConfig: LiveConfig | null =
      mode === "live"
        ? {
            strategy_id: agentId,
            assets: [
              {
                class: "Crypto",
                symbol: effectiveLiveAsset.split("/")[0] || effectiveLiveAsset,
                venue_symbol: effectiveLiveAsset,
              },
            ],
            capital: { initial: capitalNum, currency: "USD" },
            broker_creds_ref: brokerCredsRef,
            stop_policy: {
              time_limit_secs: null,
              bar_limit: barLimitNum,
              decision_limit: null,
            },
            // Coarse safety label; v1 only accepts "paper". The actual venue is
            // carried by broker_creds_ref / display_name / tags.
            venue_label: "paper",
            warmup_bars: warmupBarsNum,
            safety_limits: null,
            display_name: `Forward test ${
              brokerCredsRef === "alpaca"
                ? "Alpaca paper"
                : brokerCredsRef === "orderly_testnet"
                  ? "Orderly testnet"
                  : brokerCredsRef === "degen_arena"
                    ? "Degen Arena"
                    : "Byreal testnet"
            } ${effectiveLiveAsset}`,
            description: null,
            tags: ["live", brokerCredsRef],
            notes: null,
          }
        : null;
    const request: StartRunReq = {
      agent_id: agentId,
      scenario_id: mode === "live" ? "" : scenarioId,
      mode,
      params_override: null,
      auto_fire_review: autoFireReview,
      review_model:
        autoFireReview && activeReviewProvider && activeReviewModel
          ? { provider: activeReviewProvider.name, model: activeReviewModel }
          : null,
      max_annotations_per_review: 8,
    };
    if (liveConfig) {
      request.live_config = liveConfig;
    }
    start.mutate(request);
  }

  return (
    <div className="w-full bg-surface border border-border rounded-lg shadow-sm mb-3">
        <form onSubmit={onSubmit} className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-sans font-medium text-[20px] tracking-tight">
              Start eval
            </h2>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Picks a strategy + scenario, queues the run, and drops you
              on its detail page so you can watch progress.
            </p>
          </div>

          <div>
            <div className="block text-[12px] text-text-2 mb-1">Strategy</div>
            <StrategyPicker
              strategies={strategies.data ?? []}
              value={agentId}
              onChange={(next) => {
                setAgentId(next);
                setPreflightError(null);
              }}
              loading={strategies.isPending}
              placeholder="— pick a strategy —"
              className="h-9 min-h-9 w-full justify-between"
            />
            {strategies.isError ? (
              <p className="m-0 mt-1 text-[12px] text-rose-300">
                couldn't load strategies — try refreshing
              </p>
            ) : null}
          </div>

          <div>
            <div className="block text-[12px] text-text-2 mb-1">
              Scenario
            </div>
            <SignalSearchableSelectMenu
              ariaLabel="Scenario"
              value={scenarioId}
              options={(scenarios.data ?? []).map((scenario: Scenario) => ({
                value: scenario.id,
                label: `${scenario.display_name} · ${scenarioWindowLabel(scenario)}`,
                meta: scenario.id,
                searchText: `${scenario.display_name} ${scenario.id} ${scenarioWindowLabel(scenario)}`,
              }))}
              onChange={(next) => {
                setScenarioId(next);
                setPreflightError(null);
              }}
              placeholder="— pick a scenario —"
              searchPlaceholder="Search scenarios…"
              emptyHint="No scenarios found"
              loading={scenarios.isPending}
              className="h-9 min-h-9 w-full justify-between"
            />
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
              <label className="inline-flex items-center gap-2 text-[13px] text-text-2">
                <input
                  type="radio"
                  name="mode"
                  value="live"
                  checked={mode === "live"}
                  onChange={() => {
                    setMode("live");
                    setPreflightError(null);
                  }}
                  className="accent-gold"
                />
                forward test
              </label>
            </div>
            <p className="m-0 mt-1.5 text-[11px] text-text-3 leading-snug">
              Backtest replays a scenario. Forward test runs your strategy on
              paper against the live market (no real money) and must be bounded
              by a stop policy.
            </p>
          </fieldset>

          {mode === "live" ? (
            <fieldset className="grid grid-cols-2 gap-3">
              <legend className="col-span-2 block text-[12px] text-text-2 mb-1 px-0">
                Forward-test venue
              </legend>
              <div
                role="group"
                aria-label="Forward-test venue"
                className="col-span-2 flex gap-1 rounded-md border border-border bg-surface-elev p-1"
              >
                {(
                  [
                    ["alpaca", "Alpaca paper"],
                    ["orderly_testnet", "Orderly testnet"],
                    ["byreal", "Byreal testnet"],
                    ["degen_arena", "Degen Arena"],
                  ] as const
                ).map(([value, label]) => (
                  <button
                    key={value}
                    type="button"
                    onClick={() => setBrokerCredsRef(value)}
                    aria-pressed={brokerCredsRef === value}
                    className={`flex-1 rounded px-2 py-1 text-[12px] transition-colors ${
                      brokerCredsRef === value
                        ? "bg-gold/10 text-text-1"
                        : "text-text-2 hover:bg-surface-hover"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <LabeledInput
                label="Asset"
                help="From strategy assets"
                ariaLabel="Forward-test asset"
                value={effectiveLiveAsset}
                readOnly
              />
              <LabeledInput
                label="Capital"
                ariaLabel="Forward-test capital"
                type="number"
                min="1"
                value={liveCapital}
                onChange={setLiveCapital}
              />
              <LabeledInput
                label="Bars to run"
                help="Stop after this many live bars"
                ariaLabel="Forward-test bar limit"
                type="number"
                min="1"
                value={liveBarLimit}
                onChange={setLiveBarLimit}
              />
              <LabeledInput
                label="Warmup bars"
                help="Historical context loaded before the first live bar"
                ariaLabel="Forward-test warmup bars"
                type="number"
                min="0"
                value={liveWarmupBars}
                onChange={setLiveWarmupBars}
              />
              <p className="col-span-2 m-0 text-[11px] leading-snug text-text-3">
                Timeframe comes from the live Alpaca bar stream; this launch is
                bounded by the bar count above, not an open-ended daemon.
              </p>
            </fieldset>
          ) : null}

          <fieldset>
            <legend className="block text-[12px] text-text-2 mb-1.5 px-0">
              Review
            </legend>
            <label className="inline-flex items-center gap-2 text-[13px] text-text-2">
              <input
                type="checkbox"
                checked={autoFireReview}
                onChange={(e) => {
                  setAutoFireReview(e.target.checked);
                  setPreflightError(null);
                }}
                className="accent-gold"
              />
              auto-run review annotations on completion
            </label>
            {autoFireReview ? (
              <div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-2">
                <SignalSearchableSelectMenu
                  ariaLabel="Review provider"
                  value={activeReviewProvider?.name ?? ""}
                  options={reviewProviderRows.map((row) => ({
                    value: row.name,
                    label: row.name,
                    meta: row.enabled_models.join(", "),
                    searchText: `${row.name} ${row.enabled_models.join(" ")}`,
                  }))}
                  onChange={(next) => {
                    setReviewProvider(next);
                    const row = reviewProviderRows.find(
                      (candidate) => candidate.name === next,
                    );
                    setReviewModel(row?.enabled_models[0] ?? "");
                  }}
                  placeholder="— pick provider —"
                  searchPlaceholder="Search providers…"
                  emptyHint="No configured providers"
                  disabled={reviewProviderRows.length === 0}
                  className="w-full justify-between font-mono"
                />
                <SignalSearchableSelectMenu
                  ariaLabel="Review model"
                  value={activeReviewModel}
                  options={(activeReviewProvider?.enabled_models ?? []).map((model) => ({
                    value: model,
                    label: model,
                  }))}
                  onChange={setReviewModel}
                  placeholder="— pick model —"
                  searchPlaceholder="Search models…"
                  emptyHint="No enabled models"
                  disabled={!activeReviewProvider}
                  className="w-full justify-between font-mono"
                />
              </div>
            ) : null}
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
  );
}

function LabeledInput({
  label,
  help,
  ariaLabel,
  value,
  onChange,
  type = "text",
  min,
  readOnly = false,
}: {
  label: string;
  help?: string;
  ariaLabel?: string;
  value: string;
  onChange?: (value: string) => void;
  type?: string;
  min?: string;
  readOnly?: boolean;
}) {
  return (
    <label className="min-w-0">
      <span className="mb-1 block text-[11px] text-text-3">{label}</span>
      <input
        aria-label={ariaLabel ?? label}
        type={type}
        min={min}
        value={value}
        readOnly={readOnly}
        onChange={(e) => onChange?.(e.target.value)}
        className={[
          "w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono focus:outline-none focus:border-text-3",
          readOnly ? "text-text-3" : "",
        ].join(" ")}
      />
      {help ? (
        <span className="mt-1 block text-[10.5px] leading-snug text-text-3">
          {help}
        </span>
      ) : null}
    </label>
  );
}

function evalPreflightError({
  providers,
  brokers,
  strategy,
}: {
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
  const hasCredentialedProvider = rows.some(isProviderConfigured);
  if (!hasCredentialedProvider) {
    return "Add a provider/API key in Settings -> Providers before running eval.";
  }

  const hasEnabledModel = rows.some(
    (row) => isProviderConfigured(row) && row.enabled_models.length > 0,
  );
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
      if (!isProviderConfigured(row)) {
        return `provider '${pair.provider}' has no API key set. Add it in Settings -> Providers before running eval.`;
      }
      if (!row.enabled_models.includes(pair.model)) {
        return `model '${pair.model}' is not enabled for provider '${pair.provider}'. Enable it in Settings -> Providers before running eval.`;
      }
    }
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

function badgeColorFor(
  status: string,
): "gold" | "warn" | "danger" | "info" | "muted" {
  switch (STATUS_TONE[status] ?? "default") {
    case "gold":
      return "gold";
    case "warn":
      return "warn";
    case "danger":
      return "danger";
    case "info":
      return "info";
    default:
      return "muted";
  }
}

function signedTone(
  n: number | null | undefined,
): "default" | "gold" | "danger" {
  if (n == null || n === 0) return "default";
  return n > 0 ? "gold" : "danger";
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

function signedToneClass(n: number | null | undefined): string {
  if (n == null || n === 0) return "text-text";
  return n > 0 ? "text-gold" : "text-danger";
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
  status?: string,
): string {
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return "—";
  // Only tick against `now` while this row is still in-flight. Once the
  // status is terminal (failed/aborted/completed/etc) the timer freezes
  // even if other rows in the page are still running. If the terminal
  // row lacks a `completed_at`, fall back to "—" rather than tracking
  // wall time forever.
  const inflight = status === undefined ? true : isInflightRunStatus(status);
  const endRaw = completedAt
    ? Date.parse(completedAt)
    : inflight
      ? nowMs
      : NaN;
  if (Number.isNaN(endRaw)) return "—";
  const totalSeconds = Math.max(0, Math.floor((endRaw - start) / 1000));
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) return `${totalMinutes}m`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}

function compareIsoDesc(
  a: string | null | undefined,
  b: string | null | undefined,
): number {
  // Treat null as "earliest" so completed runs sort above in-flight.
  const av = a ? Date.parse(a) : -Infinity;
  const bv = b ? Date.parse(b) : -Infinity;
  if (Number.isNaN(av) && Number.isNaN(bv)) return 0;
  if (Number.isNaN(av)) return 1;
  if (Number.isNaN(bv)) return -1;
  return bv - av;
}
