import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError, apiFetch } from "@/api/client";
import {
  archiveScenario,
  cloneScenario,
  deleteScenario,
  getScenario,
  scenarioKeys,
} from "@/api/scenarios";
import { getScenarioChart, scenarioChartKeys } from "@/api/chart";
import { listRuns } from "@/api/eval";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { useAlpacaAssets } from "@/api/assets";
import { AssetPicker } from "@/components/AssetPicker";
import { toVenuePair } from "@/lib/assets";
import type { RunSummary } from "@/api/types.gen";
import {
  ResponsiveListCard,
  useListColumns,
  useListState,
  useListUrlState,
  type FilterDef,
  type SortOption,
} from "@/components/lists";
import { MListRow } from "@/components/lists/MListRow";
import { ScenarioChartV2 } from "@/components/chart/v2/surfaces/ScenarioChartV2";
import { scenarioChartPayloadToV2 } from "@/components/chart/v2/adapters/scenario-chart-payload";
import { CacheStatusBadge } from "@/components/scenario/CacheStatusBadge";
import { useBarsFetchJob } from "@/components/scenario/useBarsFetchJob";
import type {
  CreateScenarioRequest,
  Scenario,
  ScenarioMutations,
  SlippageModel,
  TimeWindow,
  VenueSettings,
} from "@/api/types.gen";
import { ScenarioForm } from "@/components/scenario/ScenarioForm";

// ── helpers ────────────────────────────────────────────────────────────────

type Tab = "definition" | "runs" | "bar-cache";

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  return d.toISOString().slice(0, 10);
}

function fmtSlippage(model: SlippageModel): string {
  if (model.model === "none") return "none";
  if (model.model === "volume_share")
    return `volume share (impact=${model.price_impact}, limit=${model.volume_limit})`;
  return `linear ${model.bps} bps`;
}

// ── equality helpers (clone-mutations diff) ───────────────────────────────
//
// JSON-equality fallback for unchanged-vs-changed detection. These are
// structurally small (≤ a few hundred bytes) so JSON.stringify is fine
// here — we don't need a deep-equal dep. Used by the inline clone form
// to decide whether each `ScenarioMutations` field should be `null`
// (inherit parent) or the submitted value.

function jsonEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function arraysEqual<T>(a: ReadonlyArray<T>, b: ReadonlyArray<T>): boolean {
  if (a.length !== b.length) return false;
  return jsonEq(a, b);
}

function timeWindowEquals(a: TimeWindow, b: TimeWindow): boolean {
  return a.start === b.start && a.end === b.end;
}

function venueEquals(a: VenueSettings, b: VenueSettings): boolean {
  return jsonEq(a, b);
}

// ── route ──────────────────────────────────────────────────────────────────

export function ScenariosDetailRoute() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [activeTab, setActiveTab] = useState<Tab>("definition");
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const q = useQuery({
    queryKey: scenarioKeys.detail(id),
    queryFn: () => getScenario(id),
    enabled: !!id,
  });

  // QA22 / `strategy-clone-editable-frontend` (scenario flavor):
  // operator: "Clone to edit (can't edit clone)". The engine's clone
  // API already accepts a `ScenarioMutations` payload that overrides
  // display_name / description / notes / tags / time_window etc. at
  // clone time — the SPA was passing all-nulls, so every clone was
  // identical to its parent with no way to amend. The detail view
  // now exposes an inline form (expanded by the "Clone to edit"
  // button) that lets the operator override the simple text fields
  // before submitting. Per the workspace no-popups rule the form is
  // an inline accordion, not a modal.
  const cloneMut = useMutation({
    mutationFn: (mutations: Parameters<typeof cloneScenario>[1]) =>
      cloneScenario(id, mutations),
    onSuccess: (newScenario) => {
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      navigate(`/scenarios/${newScenario.id}`);
    },
  });

  const archiveMut = useMutation({
    mutationFn: () => archiveScenario(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      qc.invalidateQueries({ queryKey: scenarioKeys.detail(id) });
    },
  });

  const deleteMut = useMutation({
    mutationFn: () => deleteScenario(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      navigate("/scenarios");
    },
    onError: (err) => {
      const msg =
        err instanceof ApiError
          ? `${err.code}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err);
      setDeleteError(msg);
    },
  });

  return (
    <>
      <Topbar
        title="Scenario"
        sub={q.isPending ? "Loading…" : q.isError ? "Error" : id}
        back={{ to: "/scenarios", label: "Back to scenarios" }}
      />

      {q.isPending ? (
        <LoadingSkeleton />
      ) : q.isError ? (
        <ErrorState err={q.error} onRetry={() => q.refetch()} />
      ) : q.data ? (
        <DetailView
          scenario={q.data}
          activeTab={activeTab}
          onTabChange={setActiveTab}
          deleteError={deleteError}
          onClearDeleteError={() => setDeleteError(null)}
          onClone={(mutations) => cloneMut.mutate(mutations)}
          onArchive={() => archiveMut.mutate()}
          onDelete={() => { setDeleteError(null); deleteMut.mutate(); }}
          isCloning={cloneMut.isPending}
          isArchiving={archiveMut.isPending}
          isDeleting={deleteMut.isPending}
        />
      ) : null}
    </>
  );
}

// ── detail view ────────────────────────────────────────────────────────────

type CloneMutations = Parameters<typeof cloneScenario>[1];

function DetailView({
  scenario: s,
  activeTab,
  onTabChange,
  deleteError,
  onClearDeleteError,
  onClone,
  onArchive,
  onDelete,
  isCloning,
  isArchiving,
  isDeleting,
}: {
  scenario: Scenario;
  activeTab: Tab;
  onTabChange: (t: Tab) => void;
  deleteError: string | null;
  onClearDeleteError: () => void;
  onClone: (mutations: CloneMutations) => void;
  onArchive: () => void;
  onDelete: () => void;
  isCloning: boolean;
  isArchiving: boolean;
  isDeleting: boolean;
}) {
  const [cloneOpen, setCloneOpen] = useState(false);

  // Pre-fill the form from the parent scenario so the operator only
  // needs to touch fields they actually want to override. ScenarioForm
  // accepts `Partial<CreateScenarioRequest>` — Scenario carries all of
  // CreateScenarioRequest's fields plus some extras (id, parent_…,
  // created_at), which we just don't read.
  //
  // Note: we hand `display_name = "<parent> (clone)"` so the operator
  // sees a sensible default. The diff-against-parent logic below treats
  // any change from the parent name (including the "(clone)" default)
  // as an explicit override.
  const formInitial: Partial<CreateScenarioRequest> = {
    display_name: `${s.display_name} (clone)`,
    description: s.description,
    asset_class: s.asset_class,
    quote_currency: s.quote_currency,
    time_window: s.time_window,
    capital: s.capital,
    timezone: s.timezone,
    calendar: s.calendar,
    venue: s.venue,
    data_source: s.data_source,
    replay_mode: s.replay_mode,
    tags: s.tags,
    notes: s.notes,
    parent_scenario_id: null,
    source: s.source,
    warmup_bars: s.warmup_bars,
  };

  // Diff the submitted request against the parent and emit a
  // `ScenarioMutations` payload with `null` for any field the operator
  // left unchanged. The engine's `api/scenario.rs::clone` handler maps
  // `null` to "inherit parent" for most fields; `notes` is special-
  // cased there (passthrough, NOT unwrap_or(parent)) — we always send
  // the form's current `notes` value to preserve PR #341's behaviour.
  function submitCloneFromForm(req: CreateScenarioRequest) {
    onClone({
      display_name: req.display_name !== s.display_name ? req.display_name : null,
      description: req.description !== s.description ? req.description : null,
      notes: req.notes,
      tags: arraysEqual(req.tags, s.tags) ? null : req.tags,
      time_window: timeWindowEquals(req.time_window, s.time_window)
        ? null
        : req.time_window,
      venue: venueEquals(req.venue, s.venue) ? null : req.venue,
      warmup_bars: req.warmup_bars !== s.warmup_bars ? req.warmup_bars : null,
    } satisfies ScenarioMutations);
  }

  return (
    <div>
      <div className="flex items-start justify-between mb-4">
        <div>
          <h1 className="text-text font-sans text-[28px] m-0 leading-tight">
            {s.display_name}
          </h1>
          <div
            data-testid="scenario-detail-id"
            className="font-mono text-[12px] text-text-3 mt-1 break-all select-all"
            aria-label={`Scenario id ${s.id}`}
          >
            {s.id}
          </div>
          {s.parent_scenario_id ? (
            <div className="mt-1 text-[12px] text-text-3">
              forked from{" "}
              <Link
                to={`/scenarios/${s.parent_scenario_id}`}
                className="font-mono hover:text-text transition-colors"
              >
                {s.parent_scenario_id}
              </Link>
            </div>
          ) : null}
          {s.description && (
            <p className="text-text-2 text-[13px] mt-1 mb-0">{s.description}</p>
          )}
          {s.archived_at && (
            <span className="inline-block mt-1 text-text-3 text-[12px]">
              archived {fmtDate(s.archived_at)}
            </span>
          )}
        </div>

        <div className="flex flex-col items-end gap-2 shrink-0 ml-6">
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setCloneOpen((open) => !open)}
              disabled={isCloning}
              aria-expanded={cloneOpen}
              aria-controls="scenario-clone-form"
              data-testid="clone-to-edit"
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 transition-colors disabled:opacity-50"
            >
              {isCloning ? "Cloning…" : cloneOpen ? "Cancel clone" : "Clone to edit"}
            </button>

            {!s.archived_at && (
              <button
                type="button"
                onClick={onArchive}
                disabled={isArchiving}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 transition-colors disabled:opacity-50"
              >
                {isArchiving ? "Archiving…" : "Archive"}
              </button>
            )}

            <button
              type="button"
              onClick={onDelete}
              disabled={isDeleting}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border border-danger text-danger hover:bg-danger/10 transition-colors disabled:opacity-50"
            >
              {isDeleting ? "Deleting…" : "Delete"}
            </button>
          </div>

          {deleteError && (
            <div className="flex items-center gap-2 px-3 py-1.5 rounded border border-danger/40 bg-danger/5 text-danger text-[12px] max-w-xs">
              <span className="flex-1">{deleteError}</span>
              <button
                type="button"
                onClick={onClearDeleteError}
                className="text-danger/60 hover:text-danger transition-colors leading-none"
                aria-label="Dismiss error"
              >
                ×
              </button>
            </div>
          )}
        </div>
      </div>

      {cloneOpen && (
        <div
          id="scenario-clone-form"
          data-testid="scenario-clone-form"
          className="mb-4 rounded border border-border-soft bg-surface-elev p-4"
        >
          <div className="mb-3 text-[12px] uppercase tracking-wide text-text-3">
            Edit before cloning
          </div>
          {/*
            ScenarioForm here reuses the wizard's widgets (asset picker,
            date range, venue fees/slippage/latency, warmup-bars input)
            so the operator can override any structural field inline.
            The granularity selector was removed from operator-facing
            scenario authoring per QA (scenarios are 1h-fixed today);
            the chart preview below carries its own "Indicator
            timeframe" picker, which is a chart rendering control, not
            scenario metadata. `submitCloneFromForm` diffs the
            submitted CreateScenarioRequest against the parent
            scenario and emits a `ScenarioMutations` payload with
            `null` for unchanged fields, preserving the engine's
            "inherit parent" semantics on the clone handler.
          */}
          <ScenarioForm
            initial={formInitial}
            submitting={isCloning}
            layout="inline"
            onSubmit={submitCloneFromForm}
            onCancel={() => setCloneOpen(false)}
          />
        </div>
      )}

      <TabBar value={activeTab} onChange={onTabChange} />

      <Card>
        {activeTab === "definition" && <DefinitionTab s={s} />}
        {activeTab === "runs" && <RunsTab scenarioId={s.id} />}
        {activeTab === "bar-cache" && (
          <BarCacheTab scenario={s} />
        )}
      </Card>
    </div>
  );
}

// ── tabs ───────────────────────────────────────────────────────────────────

function TabBar({
  value,
  onChange,
}: {
  value: Tab;
  onChange: (t: Tab) => void;
}) {
  const tabs: [Tab, string][] = [
    ["definition", "Definition"],
    ["runs", "Runs"],
    ["bar-cache", "Bar cache"],
  ];
  return (
    <div className="flex gap-4 border-b border-border mb-4">
      {tabs.map(([t, label]) => (
        <button
          key={t}
          type="button"
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

// ── definition tab ─────────────────────────────────────────────────────────

function DefinitionTab({ s }: { s: Scenario }) {
  const [chartGranularity, setChartGranularity] = useState("1h");
  useEffect(() => {
    setChartGranularity("1h");
  }, [s.id]);

  // Scenarios are asset-free; the operator chooses which market backs the
  // standalone preview. `chartAsset` is a bare symbol (e.g. "BTC"); it is
  // converted to a venue pair (e.g. "BTC/USD") for the chart and bars-fetch
  // API calls via toVenuePair(). Defaults to "BTC" matching the backend default.
  const [chartAsset, setChartAsset] = useState("BTC");
  const alpacaAssets = useAlpacaAssets();

  // Venue pair form used for chart/bars API calls.
  const chartAssetPair = toVenuePair(chartAsset);

  const marketLabel = `${s.asset_class} / ${s.quote_currency}`;

  const windowLabel = `${fmtDate(s.time_window.start)} → ${fmtDate(s.time_window.end)}`;

  const chart = useQuery({
    queryKey: scenarioChartKeys.scenario(s.id, chartGranularity, chartAssetPair),
    queryFn: () => getScenarioChart(s.id, chartGranularity, chartAssetPair),
  });
  const barsFetch = useBarsFetchJob(
    buildBarsFetchSpec(s, chartGranularity, chartAssetPair),
  );

  return (
    <div>
      <div className="p-5 pb-0">
        {chart.isPending && (
          <div className="text-text-3 text-[13px] mb-4">Loading chart…</div>
        )}
        {chart.isError && (
          <div className="text-danger text-[13px] mb-4">Chart unavailable.</div>
        )}
        {chart.data && (
          <div className="mb-5">
            <div className="mb-3 flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <span className="text-text-3 text-[12px]">Preview asset</span>
                {alpacaAssets.isPending ? (
                  <span className="text-text-3 text-[12px]">Loading…</span>
                ) : (
                  <AssetPicker
                    assets={alpacaAssets.data}
                    value={chartAsset}
                    onChange={setChartAsset}
                    showOrderlyOnlyBadge={false}
                    placeholder="Search assets…"
                    className="w-36"
                  />
                )}
              </div>
              <div className="flex items-center gap-2">
                <label className="text-text-3 text-[12px]" htmlFor="scenario-chart-granularity">
                  Indicator timeframe
                </label>
                <select
                  id="scenario-chart-granularity"
                  value={chartGranularity}
                  onChange={(event) => setChartGranularity(event.target.value)}
                  className="bg-surface-elev border border-border rounded px-2 py-1 text-[12px] text-text focus:outline-none focus:border-gold/40"
                >
                  {CHART_GRANULARITY_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>
            <div className="flex items-center justify-between mb-2">
              <span className="text-text-3 text-[12px]">
                {chartAssetPair} · {chartGranularity}
              </span>
              <CacheStatusBadge
                status={chart.data.cache_status}
                onFetch={barsFetch.start}
                fetchStatus={barsFetch.statusText}
                disabled={!barsFetch.canStart}
              />
            </div>
            {chart.data.bars.length === 0 ? (
              <div className="flex items-center justify-center h-[360px] text-text-3 text-[13px] border border-border rounded">
                No bars cached yet. Use Fetch bars to populate this chart.
              </div>
            ) : (
              <ScenarioChartV2
                payload={scenarioChartPayloadToV2(
                  chart.data,
                  chartAssetPair,
                  chartGranularity,
                )}
              />
            )}
            <BarsFetchJobStatus fetch={barsFetch} />
          </div>
        )}
      </div>

      <dl className="grid grid-cols-[180px_1fr] gap-y-2.5 text-[13px] px-5 pb-5">
        <dt className="text-text-3 self-center">Market</dt>
        <dd className="font-mono m-0">{marketLabel}</dd>

        <dt className="text-text-3 self-center">Window</dt>
        <dd className="font-mono m-0">{windowLabel}</dd>

        {/*
          Granularity removed from the operator-facing scenario
          metadata panel per QA: scenarios are 1h-fixed today, so
          showing it as a metadata row read as a configurable
          attribute when it isn't. The chart preview's
          "Indicator timeframe" picker above is a chart rendering
          control and stays. If granularity ever becomes
          per-scenario configurable again, re-introduce this row.
        */}

        <dt className="text-text-3 self-center">Venue</dt>
        <dd className="font-mono m-0">{String(s.venue.venue)}</dd>

        <dt className="text-text-3 self-center">Fees (m/t bps)</dt>
        <dd className="font-mono m-0">
          {s.venue.fees.maker_bps} / {s.venue.fees.taker_bps}
        </dd>

        <dt className="text-text-3 self-center">Slippage</dt>
        <dd className="font-mono m-0">{fmtSlippage(s.venue.slippage)}</dd>

        <dt className="text-text-3 self-center">Latency (ms)</dt>
        <dd className="font-mono m-0">{s.venue.latency.decision_to_fill_ms}</dd>

        <dt className="text-text-3 self-center">Cache key</dt>
        <dd className="font-mono text-[11px] break-all m-0">
          {s.bar_cache_policy.cache_key}
        </dd>

        <dt className="text-text-3 self-center">Source</dt>
        <dd className="m-0">
          <Pill tone="default">{s.source}</Pill>
        </dd>

        <dt className="text-text-3 self-start mt-0.5">Tags</dt>
        <dd className="m-0">
          {s.tags.length > 0 ? (
            <div className="flex flex-wrap gap-1">
              {s.tags.map((tag) => (
                <Pill key={tag} tone="default">
                  {tag}
                </Pill>
              ))}
            </div>
          ) : (
            <span className="text-text-3">—</span>
          )}
        </dd>
      </dl>
    </div>
  );
}

// ── runs tab ───────────────────────────────────────────────────────────────

// Migrated to ResponsiveListCard 2026-05-21 per audit row #8
// (`docs/superpowers/audits/2026-05-21-list-surfaces-audit.md`). Adds
// search by run id + strategy name, filters by mode + status, and
// sorts by completed-desc (default), started-desc, or strategy A→Z.
// URL state lives at `useListUrlState("scenario-runs", …)`. The
// underlying server query is unchanged — `listRuns()` still returns
// every run; client-side filtering narrows to `scenario_id`.

const RUNS_SORT_OPTIONS: SortOption[] = [
  { value: "completed-desc", label: "Recently completed" },
  { value: "started-desc", label: "Recently started" },
  { value: "strategy", label: "Strategy A → Z" },
];

const RUNS_MODE_FILTER: FilterDef = {
  id: "mode",
  label: "Mode",
  options: [
    { value: "all", label: "All modes" },
    { value: "backtest", label: "Backtest" },
    { value: "live", label: "Live" },
  ],
};

const RUNS_STATUS_FILTER: FilterDef = {
  id: "status",
  label: "Status",
  options: [
    { value: "all", label: "All statuses" },
    { value: "completed", label: "Completed" },
    { value: "running", label: "Running" },
    { value: "cancelled", label: "Cancelled" },
    { value: "failed", label: "Failed" },
    { value: "queued", label: "Pending" },
  ],
};

const RUNS_DESKTOP_COLUMNS = [
  { key: "run",       label: "Run",       essential: true, estWidth: 200 },
  { key: "strategy",  label: "Strategy",  priority: 3,     estWidth: 160 },
  { key: "mode",      label: "Mode",      priority: 2,     estWidth: 90  },
  { key: "status",    label: "Status",    essential: true, estWidth: 100 },
  { key: "completed", label: "Completed", priority: 1,     estWidth: 120 },
];

function RunsTab({ scenarioId }: { scenarioId: string }) {
  const navigate = useNavigate();
  const runs = useQuery({
    queryKey: ["runs", "by-scenario", scenarioId],
    queryFn: () => listRuns(),
  });

  // Fetch all strategies once to build a display_name lookup map. The query is
  // cached under the global strategy list key so other parts of the UI share
  // the same cache entry. When still loading we fall back to showing the raw
  // agent_id ULID — no flicker, no empty placeholder.
  const { data: strategies } = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });

  const strategyNameMap = useMemo(
    () =>
      new Map<string, string>(
        (strategies ?? []).map((s) => [s.agent_id, s.display_name]),
      ),
    [strategies],
  );

  const scopedRows = useMemo(
    () => (runs.data ?? []).filter((r) => r.scenario_id === scenarioId),
    [runs.data, scenarioId],
  );

  const list = useListState<RunSummary>({
    rows: scopedRows,
    filters: [RUNS_MODE_FILTER, RUNS_STATUS_FILTER],
    sortOptions: RUNS_SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const mode = values.mode ?? "all";
      if (mode !== "all" && row.mode !== mode) return false;
      const status = values.status ?? "all";
      if (status !== "all" && row.status !== status) return false;
      const needle = query.trim().toLowerCase();
      if (needle.length === 0) return true;
      if (row.id.toLowerCase().includes(needle)) return true;
      const strategyName =
        strategyNameMap.get(row.agent_id) ?? row.agent_id;
      return strategyName.toLowerCase().includes(needle);
    },
    sortFn: (rs, key) => {
      switch (key) {
        case "strategy":
          return [...rs].sort((a, b) => {
            const an = strategyNameMap.get(a.agent_id) ?? a.agent_id;
            const bn = strategyNameMap.get(b.agent_id) ?? b.agent_id;
            return an.localeCompare(bn);
          });
        case "started-desc":
          return [...rs].sort((a, b) =>
            (b.started_at || "").localeCompare(a.started_at || ""),
          );
        case "completed-desc":
        default:
          return [...rs].sort((a, b) => {
            // Runs without a completed_at sort to the bottom of "Recently
            // completed" so in-flight runs don't masquerade as the freshest
            // result. Tie-break on started_at descending.
            const ac = a.completed_at || "";
            const bc = b.completed_at || "";
            if (!ac && !bc) {
              return (b.started_at || "").localeCompare(a.started_at || "");
            }
            if (!ac) return 1;
            if (!bc) return -1;
            return bc.localeCompare(ac);
          });
      }
    },
  });
  useListUrlState("scenario-runs", list);
  const columnState = useListColumns("scenario-runs", RUNS_DESKTOP_COLUMNS);

  return (
    <div className="px-3 py-2">
      <ResponsiveListCard<RunSummary>
        listId="scenario-runs"
        title="Runs"
        count={list.totalRows}
        toolbar={{
          search: {
            ...list.search,
            placeholder: "Search run id or strategy…",
          },
          filters: list.filters,
          sort: list.sort,
          clearAll: list.clearAll,
        }}
        columns={RUNS_DESKTOP_COLUMNS}
        columnState={columnState}
        rows={list.rows}
        loading={runs.isPending}
        error={
          runs.isError
            ? {
                message:
                  runs.error instanceof Error
                    ? runs.error.message
                    : String(runs.error),
                retry: () => runs.refetch(),
              }
            : null
        }
        empty={
          scopedRows.length === 0
            ? "No runs against this scenario yet."
            : "No runs match these filters."
        }
        renderRow={(r) => {
          const strategyName = strategyNameMap.get(r.agent_id);
          return (
            <tr key={r.id} className="border-t border-border align-middle">
              <td className="py-2 px-3">
                <Link
                  to={`/eval-runs/${r.id}`}
                  className="font-mono text-[12px] text-text hover:underline"
                >
                  {r.id}
                </Link>
              </td>
              <td className="py-2 pr-3">
                {strategyName != null ? (
                  <>
                    <div className="text-text-2">{strategyName}</div>
                    <div className="font-mono text-[11px] text-text-3">
                      {r.agent_id}
                    </div>
                  </>
                ) : (
                  <div className="font-mono text-[12px] text-text-2">
                    {r.agent_id}
                  </div>
                )}
              </td>
              <td className="py-2 pr-3 text-text-2">{r.mode}</td>
              <td className="py-2 pr-3 text-text-2">{r.status}</td>
              <td className="py-2 px-3 text-text-3">
                {r.completed_at ? fmtDate(r.completed_at) : "—"}
              </td>
            </tr>
          );
        }}
        renderMobileRow={(r) => {
          const strategyName = strategyNameMap.get(r.agent_id) ?? r.agent_id;
          const completed = r.completed_at
            ? `Completed ${fmtDate(r.completed_at)}`
            : `Started ${fmtDate(r.started_at)}`;
          return (
            <MListRow
              key={r.id}
              onClick={() => navigate(`/eval-runs/${r.id}`)}
              title={strategyName}
              badge={r.status}
              subtitle={`${r.mode} · ${completed}`}
            />
          );
        }}
      />
    </div>
  );
}

// ── bar-cache tab ──────────────────────────────────────────────────────────

type BarsCacheRowResponse = {
  cache_key: string;
  asset: string;
  granularity: string;
  window_start: string;
  window_end: string;
  bar_count: number;
  fetched_at: string;
};

function BarCacheTab({ scenario }: { scenario: Scenario }) {
  const cacheKey = scenario.bar_cache_policy.cache_key;
  const barsFetch = useBarsFetchJob(buildBarsFetchSpec(scenario));
  const { data, isPending } = useQuery({
    queryKey: ["bars-cache", cacheKey],
    queryFn: () =>
      apiFetch<BarsCacheRowResponse>(
        `/api/bars/${encodeURIComponent(cacheKey)}`,
      ),
    retry: false,
  });

  if (isPending) {
    return (
      <div className="px-6 py-8 text-center text-text-3 text-[13px]">
        Loading cache row…
      </div>
    );
  }

  if (!data) {
    return (
      <div className="px-6 py-8">
        <p className="text-text-3 text-[13px] m-0 mb-1">Cache key</p>
        <code className="font-mono text-[12px] text-text break-all">
          {cacheKey}
        </code>
        <p className="text-text-3 text-[13px] mt-4">
          No cache row yet.
        </p>
        <button
          type="button"
          onClick={barsFetch.start}
          disabled={!barsFetch.canStart}
          className="mt-3 border border-border px-2 py-1 rounded text-[12px] text-text hover:border-text-3 disabled:opacity-60 disabled:cursor-not-allowed"
        >
          {barsFetch.statusText ?? "Fetch bars"}
        </button>
        <BarsFetchJobStatus fetch={barsFetch} />
      </div>
    );
  }

  return (
    <div className="px-5 py-4">
      <dl className="grid grid-cols-[180px_1fr] gap-y-2 text-[13px]">
        <dt className="text-text-3">Cache key</dt>
        <dd className="font-mono text-[11px] break-all m-0">{cacheKey}</dd>

        <dt className="text-text-3">Preview asset</dt>
        <dd className="font-mono m-0">{data.asset}</dd>

        {/*
          Renamed "Granularity" → "Bar timeframe" here so this read-
          out is unambiguously about the cached bar data (the chart
          rendering surface), not about scenario configuration. See
          the matching note in the main scenario metadata <dl>
          above.
        */}
        <dt className="text-text-3">Bar timeframe</dt>
        <dd className="font-mono m-0">{data.granularity}</dd>

        <dt className="text-text-3">Window</dt>
        <dd className="font-mono text-[12px] m-0">
          {new Date(data.window_start).toLocaleDateString()} → {new Date(data.window_end).toLocaleDateString()}
        </dd>

        <dt className="text-text-3">Bars</dt>
        <dd className="font-mono m-0">{data.bar_count.toLocaleString()}</dd>

        <dt className="text-text-3">Fetched at</dt>
        <dd className="font-mono text-[12px] m-0">{new Date(data.fetched_at).toLocaleString()}</dd>

        <dt className="text-text-3"></dt>
        <dd className="m-0 mt-2">
          <button
            type="button"
            onClick={barsFetch.start}
            disabled={!barsFetch.canStart}
            className="border border-border px-2 py-1 rounded text-[12px] text-text hover:border-text-3 disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {barsFetch.statusText ?? "Fetch bars"}
          </button>
          <BarsFetchJobStatus fetch={barsFetch} />
        </dd>
      </dl>
    </div>
  );
}

const CHART_GRANULARITY_OPTIONS = [
  { value: "1m", label: "1 minute" },
  { value: "5m", label: "5 minutes" },
  { value: "15m", label: "15 minutes" },
  { value: "1h", label: "1 hour" },
  { value: "4h", label: "4 hours" },
  { value: "6h", label: "6 hours" },
  { value: "1d", label: "1 day" },
  { value: "1w", label: "1 week" },
];


function buildBarsFetchSpec(s: Scenario, granularity?: string, asset?: string) {
  // Scenarios are asset-free; the operator picks which market backs the
  // standalone preview. Default to BTC/USD when none is selected.
  const selectedAsset = asset ?? "BTC/USD";
  const selectedGranularity = granularity ?? "1h";
  const invalidateQueryKeys: Array<readonly unknown[]> = [
    scenarioChartKeys.scenario(s.id, selectedGranularity, selectedAsset),
    ["bars-cache", s.bar_cache_policy.cache_key] as const,
  ];
  return {
    asset: selectedAsset,
    granularity: selectedGranularity,
    from: fmtDate(s.time_window.start),
    to: fmtDate(s.time_window.end),
    invalidateQueryKeys,
  };
}

function BarsFetchJobStatus({
  fetch,
}: {
  fetch: ReturnType<typeof useBarsFetchJob>;
}) {
  if (!fetch.statusText && !fetch.outputText && !fetch.errorText) return null;
  return (
    <div className="mt-2 rounded border border-border bg-surface px-3 py-2 text-[12px] text-text-2">
      {fetch.statusText && <div>{fetch.statusText}</div>}
      {fetch.errorText && <div className="text-danger">{fetch.errorText}</div>}
      {fetch.outputText && (
        <pre className="mt-2 whitespace-pre-wrap font-mono text-[11px] text-text-3">
          {fetch.outputText}
        </pre>
      )}
    </div>
  );
}

// ── loading / error states ─────────────────────────────────────────────────

function LoadingSkeleton() {
  return (
    <Card>
      <div className="px-5 py-6 space-y-4" aria-busy>
        <div className="h-5 w-64 rounded bg-surface-elev animate-pulse" />
        <div className="h-4 w-96 rounded bg-surface-elev animate-pulse" />
        <div className="h-4 w-80 rounded bg-surface-elev animate-pulse" />
        <div className="h-4 w-72 rounded bg-surface-elev animate-pulse" />
      </div>
    </Card>
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
    <Card>
      <div className="px-6 py-12 text-center">
        <div className="font-sans font-semibold text-[24px] text-danger mb-3">
          couldn't load scenario
        </div>
        <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
          <code className="text-danger font-mono text-[12px]">{detail}</code>
        </p>
        <button
          type="button"
          onClick={onRetry}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Retry
        </button>
      </div>
    </Card>
  );
}
