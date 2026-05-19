import { useEffect, useState } from "react";
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
import { ScenarioChart } from "@/components/chart/ScenarioChart";
import {
  scenarioGranularityToCli,
  useBarsFetchJob,
} from "@/components/scenario/useBarsFetchJob";
import type { Scenario, SlippageModel } from "@/api/types.gen";

// ── helpers ────────────────────────────────────────────────────────────────

type Tab = "definition" | "runs" | "bar-cache";

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  return d.toISOString().slice(0, 10);
}

function fmtSlippage(model: SlippageModel): string {
  if (model.model === "none") return "none";
  return `linear ${model.bps} bps`;
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
        title={q.data?.display_name ?? "Scenario"}
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
  const [cloneDisplayName, setCloneDisplayName] = useState(
    `${s.display_name} (clone)`,
  );
  const [cloneDescription, setCloneDescription] = useState(s.description);
  const [cloneNotes, setCloneNotes] = useState(s.notes ?? "");
  const [cloneTags, setCloneTags] = useState(s.tags.join(", "));

  function submitClone() {
    const parsedTags = cloneTags
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    const tagsChanged =
      parsedTags.length !== s.tags.length ||
      parsedTags.some((t, i) => t !== s.tags[i]);
    // `notes` is special-cased in `api/scenario.rs::clone`:
    //   notes: mutations.notes,    // passthrough, NOT unwrap_or(parent)
    // …whereas `description` / `tags` etc. use
    // `mutations.X.unwrap_or(parent_s.X)`. Sending null for notes
    // therefore writes empty notes, not "inherit parent". Always send
    // the form's current notes value to preserve the prefilled parent
    // text when the operator leaves it untouched. PR #341 review.
    onClone({
      display_name: cloneDisplayName.trim() || null,
      description: cloneDescription !== s.description ? cloneDescription : null,
      notes: cloneNotes,
      tags: tagsChanged ? parsedTags : null,
      time_window: null,
      asset: null,
      granularity: null,
      venue: null,
      warmup_bars: null,
    });
  }

  return (
    <div>
      <Breadcrumb scenario={s} />

      <div className="flex items-start justify-between mb-4">
        <div>
          <h1 className="text-text font-serif text-[28px] m-0 leading-tight">
            {s.display_name}
          </h1>
          <div
            data-testid="scenario-detail-id"
            className="font-mono text-[12px] text-text-3 mt-1 break-all select-all"
            aria-label={`Scenario id ${s.id}`}
          >
            {s.id}
          </div>
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
          <div className="grid grid-cols-1 gap-3">
            <label className="block">
              <span className="block text-[12px] text-text-2 mb-1">
                Display name
              </span>
              <input
                value={cloneDisplayName}
                onChange={(e) => setCloneDisplayName(e.target.value)}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
              />
            </label>
            <label className="block">
              <span className="block text-[12px] text-text-2 mb-1">
                Description
              </span>
              <textarea
                value={cloneDescription}
                onChange={(e) => setCloneDescription(e.target.value)}
                rows={2}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40 resize-vertical"
              />
            </label>
            <label className="block">
              <span className="block text-[12px] text-text-2 mb-1">Notes</span>
              <textarea
                value={cloneNotes}
                onChange={(e) => setCloneNotes(e.target.value)}
                rows={2}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40 resize-vertical"
              />
            </label>
            <label className="block">
              <span className="block text-[12px] text-text-2 mb-1">
                Tags (comma-separated)
              </span>
              <input
                value={cloneTags}
                onChange={(e) => setCloneTags(e.target.value)}
                className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
              />
            </label>
          </div>
          <p className="m-0 mt-3 text-[12px] text-text-3 leading-snug">
            Other fields (asset / window / venue / granularity) are inherited
            from the parent. Use the wizard to clone with structural changes.
          </p>
          <div className="mt-3 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => setCloneOpen(false)}
              className="px-3 py-1.5 rounded text-[12.5px] text-text-3 hover:text-text"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={submitClone}
              disabled={isCloning || cloneDisplayName.trim().length === 0}
              data-testid="scenario-clone-submit"
              className="px-3.5 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-50"
            >
              {isCloning ? "Cloning…" : "Create clone"}
            </button>
          </div>
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

// ── breadcrumb ─────────────────────────────────────────────────────────────

function Breadcrumb({ scenario }: { scenario: Scenario }) {
  return (
    <nav className="text-[12px] text-text-3 mb-3">
      <Link to="/scenarios" className="hover:text-text transition-colors">
        Scenarios
      </Link>
      {scenario.parent_scenario_id && (
        <>
          {" · forked from "}
          <Link
            to={`/scenarios/${scenario.parent_scenario_id}`}
            className="hover:text-text transition-colors"
          >
            {scenario.parent_scenario_id}
          </Link>
        </>
      )}
    </nav>
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
              ? "border-text text-text"
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
  const scenarioGranularity = scenarioGranularityToCli(s.granularity);
  const [chartGranularity, setChartGranularity] = useState(scenarioGranularity);
  useEffect(() => {
    setChartGranularity(scenarioGranularity);
  }, [s.id, scenarioGranularity]);

  const assetLabel =
    s.asset.length > 0
      ? s.asset.map((a) => a.symbol).join(", ") + " / " + s.quote_currency
      : "—";

  const windowLabel = `${fmtDate(s.time_window.start)} → ${fmtDate(s.time_window.end)}`;

  const chart = useQuery({
    queryKey: scenarioChartKeys.scenario(s.id, chartGranularity),
    queryFn: () => getScenarioChart(s.id, chartGranularity),
  });
  const barsFetch = useBarsFetchJob(buildBarsFetchSpec(s, chartGranularity));

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
              <label className="text-text-3 text-[12px]" htmlFor="scenario-chart-granularity">
                Indicator timeframe
              </label>
              <select
                id="scenario-chart-granularity"
                value={chartGranularity}
                onChange={(event) => setChartGranularity(event.target.value)}
                className="bg-surface border border-border rounded px-2 py-1 text-[12px] text-text"
              >
                {CHART_GRANULARITY_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
            <ScenarioChart
              payload={chart.data}
              onFetch={barsFetch.start}
              fetchStatus={barsFetch.statusText}
              fetchDisabled={!barsFetch.canStart}
            />
            <BarsFetchJobStatus fetch={barsFetch} />
          </div>
        )}
      </div>

      <dl className="grid grid-cols-[180px_1fr] gap-y-2.5 text-[13px] px-5 pb-5">
        <dt className="text-text-3 self-center">Asset</dt>
        <dd className="font-mono m-0">{assetLabel}</dd>

        <dt className="text-text-3 self-center">Window</dt>
        <dd className="font-mono m-0">{windowLabel}</dd>

        <dt className="text-text-3 self-center">Granularity</dt>
        <dd className="font-mono m-0">{s.granularity}</dd>

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

function RunsTab({ scenarioId }: { scenarioId: string }) {
  const { data, isPending, error } = useQuery({
    queryKey: ["runs", "by-scenario", scenarioId],
    queryFn: () => listRuns(),
  });

  if (isPending) {
    return (
      <div className="px-6 py-8 text-center text-text-3 text-[13px]">
        Loading runs…
      </div>
    );
  }
  if (error) {
    return (
      <div className="px-6 py-8 text-center text-danger text-[13px]">
        {error instanceof Error ? error.message : String(error)}
      </div>
    );
  }

  const filtered = (data ?? []).filter((r) => r.scenario_id === scenarioId);

  if (filtered.length === 0) {
    return (
      <div className="px-6 py-8 text-center">
        <p className="text-text-3 text-[13px] m-0">
          No runs against this scenario yet.
        </p>
      </div>
    );
  }

  return (
    <div className="px-5 py-4 overflow-x-auto">
      <table className="w-full text-[13px]">
        <thead>
          <tr className="text-text-3 text-left">
            <th className="pb-2 pr-4 font-medium">Run</th>
            <th className="pb-2 pr-4 font-medium">Strategy</th>
            <th className="pb-2 pr-4 font-medium">Mode</th>
            <th className="pb-2 pr-4 font-medium">Status</th>
            <th className="pb-2 font-medium">Completed</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((r) => (
            <tr key={r.id} className="border-t border-border">
              <td className="py-2 pr-4">
                <Link
                  to={`/eval-runs/${r.id}`}
                  className="font-mono text-[12px] text-text hover:underline"
                >
                  {r.id}
                </Link>
              </td>
              <td className="py-2 pr-4 font-mono text-[12px] text-text-2">
                {r.agent_id}
              </td>
              <td className="py-2 pr-4 text-text-2">{r.mode}</td>
              <td className="py-2 pr-4 text-text-2">{r.status}</td>
              <td className="py-2 text-text-3">
                {r.completed_at ? fmtDate(r.completed_at) : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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

        <dt className="text-text-3">Asset</dt>
        <dd className="font-mono m-0">{data.asset}</dd>

        <dt className="text-text-3">Granularity</dt>
        <dd className="font-mono m-0">{data.granularity}</dd>

        <dt className="text-text-3">Window</dt>
        <dd className="font-mono text-[12px] m-0">
          {data.window_start} → {data.window_end}
        </dd>

        <dt className="text-text-3">Bars</dt>
        <dd className="font-mono m-0">{data.bar_count.toLocaleString()}</dd>

        <dt className="text-text-3">Fetched at</dt>
        <dd className="font-mono text-[12px] m-0">{data.fetched_at}</dd>

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

function buildBarsFetchSpec(s: Scenario, granularity?: string) {
  const asset = s.asset[0]?.symbol;
  if (!asset) return null;
  const selectedGranularity = granularity ?? scenarioGranularityToCli(s.granularity);
  const scenarioGranularity = scenarioGranularityToCli(s.granularity);
  const invalidateQueryKeys: Array<readonly unknown[]> = [
    scenarioChartKeys.scenario(s.id, selectedGranularity),
  ];
  if (selectedGranularity === scenarioGranularity) {
    invalidateQueryKeys.push(["bars-cache", s.bar_cache_policy.cache_key] as const);
  }
  return {
    asset,
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
        <div className="font-serif italic text-[24px] text-danger mb-3">
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
