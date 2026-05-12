import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { listScenarios, scenarioKeys } from "@/api/scenarios";
import type { Scenario, ScenarioSource, ListScenariosFilter } from "@/api/types.gen";

const SOURCE_TONE: Record<ScenarioSource, "default" | "gold" | "info" | "warn"> = {
  Canonical: "gold",
  User: "info",
  Clone: "default",
  Generated: "warn",
};

// Lowercase values used in the <select> option values
type SourceOption = "any" | "canonical" | "user" | "clone" | "generated";

function sourceOptionToFilter(v: SourceOption): ScenarioSource | null {
  switch (v) {
    case "canonical": return "Canonical";
    case "user": return "User";
    case "clone": return "Clone";
    case "generated": return "Generated";
    default: return null;
  }
}

function buildFilter(source: SourceOption, includeArchived: boolean): ListScenariosFilter {
  return {
    source: sourceOptionToFilter(source),
    tags: [],
    include_archived: includeArchived,
    parent_scenario_id: null,
  };
}

export function ScenariosRoute() {
  const [source, setSource] = useState<SourceOption>("any");
  const [includeArchived, setIncludeArchived] = useState(false);

  const filter = buildFilter(source, includeArchived);

  const q = useQuery({
    queryKey: scenarioKeys.list(filter),
    queryFn: () => listScenarios(filter),
  });

  return (
    <>
      <Topbar title="Scenarios" sub={subtitleFor(q)} />
      <div className="mb-3 flex items-center justify-between">
        <Filters
          source={source}
          onSource={setSource}
          includeArchived={includeArchived}
          onIncludeArchived={setIncludeArchived}
        />
        <Link
          to="/scenarios/new"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 transition-colors"
        >
          + New scenario
        </Link>
      </div>
      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : q.data && q.data.length === 0 ? (
          <EmptyState />
        ) : (
          <ScenariosTable items={q.data ?? []} />
        )}
      </Card>
    </>
  );
}

function subtitleFor(q: ReturnType<typeof useQuery>) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load scenarios";
  const data = q.data as { length: number } | undefined;
  if (!data) return "";
  const n = data.length;
  return `${n} ${n === 1 ? "scenario" : "scenarios"}`;
}

function Filters({
  source,
  onSource,
  includeArchived,
  onIncludeArchived,
}: {
  source: SourceOption;
  onSource: (v: SourceOption) => void;
  includeArchived: boolean;
  onIncludeArchived: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-4">
      <div className="flex items-center gap-2">
        <label htmlFor="scenario-source" className="text-[12px] text-text-2">
          Source
        </label>
        <select
          id="scenario-source"
          value={source}
          onChange={(e) => onSource(e.target.value as SourceOption)}
          className="text-[12px] bg-surface-card border border-border text-text rounded px-2 py-1 outline-none focus:border-text-3"
        >
          <option value="any">(any)</option>
          <option value="canonical">Canonical</option>
          <option value="user">User</option>
          <option value="clone">Clone</option>
          <option value="generated">Generated</option>
        </select>
      </div>
      <label className="flex items-center gap-2 text-[12px] text-text-2 cursor-pointer">
        <input
          type="checkbox"
          checked={includeArchived}
          onChange={(e) => onIncludeArchived(e.target.checked)}
          className="accent-gold cursor-pointer"
        />
        Include archived
      </label>
    </div>
  );
}

function ScenariosTable({ items }: { items: Scenario[] }) {
  const navigate = useNavigate();

  function go(id: string) {
    navigate(`/scenarios/${id}`);
  }

  return (
    <table className="w-full">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 pl-5 pr-3">Name</th>
          <th className="font-normal py-2.5 px-3">Asset</th>
          <th className="font-normal py-2.5 px-3">Window</th>
          <th className="font-normal py-2.5 px-3">Granularity</th>
          <th className="font-normal py-2.5 px-3">Source</th>
          <th className="font-normal py-2.5 px-3">Tags</th>
          <th className="font-normal py-2.5 px-5 text-right"></th>
        </tr>
      </thead>
      <tbody>
        {items.map((row) => (
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
            <td className="py-3 pl-5 pr-3">
              <div className="text-text text-[13px] leading-tight">{row.display_name}</div>
              {row.description ? (
                <div className="text-text-3 text-[11px] leading-tight mt-0.5 max-w-[260px] truncate">
                  {row.description}
                </div>
              ) : null}
            </td>
            <td className="py-3 px-3 font-mono text-text-2 text-[12px]">
              {row.asset.length > 0 ? row.asset.map((a) => a.symbol).join(", ") : "—"}
            </td>
            <td className="py-3 px-3 text-text-2 text-[12px] whitespace-nowrap">
              {fmtWindow(row.time_window.start, row.time_window.end)}
            </td>
            <td className="py-3 px-3 font-mono text-text-2 text-[12px]">
              {row.granularity}
            </td>
            <td className="py-3 px-3">
              <SourcePill source={row.source} />
            </td>
            <td className="py-3 px-3">
              <div className="flex flex-wrap gap-1">
                {row.tags.length > 0
                  ? row.tags.slice(0, 3).map((tag) => (
                      <Pill key={tag} tone="default">
                        {tag}
                      </Pill>
                    ))
                  : <span className="text-text-3 text-[11px]">—</span>}
                {row.tags.length > 3 ? (
                  <Pill tone="default">+{row.tags.length - 3}</Pill>
                ) : null}
              </div>
            </td>
            <td
              className="py-3 px-5 text-right"
              onClick={(e) => e.stopPropagation()}
            >
              <Link
                to={`/scenarios/${row.id}`}
                className="text-[12px] text-text-3 hover:text-text transition-colors"
                tabIndex={-1}
              >
                View →
              </Link>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function SourcePill({ source }: { source: ScenarioSource }) {
  const tone = SOURCE_TONE[source] ?? "default";
  return <Pill tone={tone}>{source}</Pill>;
}

function fmtWindow(start: string, end: string): string {
  function fmtDate(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
    return d.toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }
  return `${fmtDate(start)} – ${fmtDate(end)}`;
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-40 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-20 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-28 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-16 rounded bg-surface-elev animate-pulse" />
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
        no scenarios yet
      </div>
      <p className="m-0 max-w-md mx-auto leading-snug">
        Scenarios define the market environment for eval runs. Use{" "}
        <code className="text-text font-mono">xvn scenario</code> to create
        one, or click{" "}
        <Link
          to="/scenarios/new"
          className="text-gold hover:underline underline-offset-2"
        >
          + New scenario
        </Link>{" "}
        above.
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
        couldn't load scenarios
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
