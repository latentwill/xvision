import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  archiveScenario,
  cloneScenario,
  deleteScenario,
  getScenario,
  scenarioKeys,
} from "@/api/scenarios";
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

  const cloneMut = useMutation({
    mutationFn: () =>
      cloneScenario(id, {
        display_name: `${q.data?.display_name ?? id} (clone)`,
        description: null,
        time_window: null,
        asset: null,
        granularity: null,
        venue: null,
        tags: null,
        notes: null,
      }),
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
          onClone={() => cloneMut.mutate()}
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
  onClone: () => void;
  onArchive: () => void;
  onDelete: () => void;
  isCloning: boolean;
  isArchiving: boolean;
  isDeleting: boolean;
}) {
  return (
    <div>
      <Breadcrumb scenario={s} />

      <div className="flex items-start justify-between mb-4">
        <div>
          <h1 className="text-text font-serif text-[28px] m-0 leading-tight">
            {s.display_name}
          </h1>
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
              onClick={onClone}
              disabled={isCloning}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 transition-colors disabled:opacity-50"
            >
              {isCloning ? "Cloning…" : "Clone to edit"}
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

      <TabBar value={activeTab} onChange={onTabChange} />

      <Card>
        {activeTab === "definition" && <DefinitionTab s={s} />}
        {activeTab === "runs" && <RunsTab />}
        {activeTab === "bar-cache" && (
          <BarCacheTab cacheKey={s.bar_cache_policy.cache_key} />
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
  const assetLabel =
    s.asset.length > 0
      ? s.asset.map((a) => a.symbol).join(", ") + " / " + s.quote_currency
      : "—";

  const windowLabel = `${fmtDate(s.time_window.start)} → ${fmtDate(s.time_window.end)}`;

  return (
    <dl className="grid grid-cols-[180px_1fr] gap-y-2.5 text-[13px] p-5">
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
  );
}

// ── runs tab ───────────────────────────────────────────────────────────────

function RunsTab() {
  return (
    <div className="px-6 py-8 text-center">
      <p className="text-text-3 text-[13px] m-0">
        Runs against this scenario will appear here.
      </p>
    </div>
  );
}

// ── bar-cache tab ──────────────────────────────────────────────────────────

function BarCacheTab({ cacheKey }: { cacheKey: string }) {
  return (
    <div className="px-6 py-8">
      <p className="text-text-3 text-[13px] m-0 mb-1">Cache key</p>
      <code className="font-mono text-[12px] text-text break-all">{cacheKey}</code>
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
