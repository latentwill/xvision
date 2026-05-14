import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import { ApiError } from "@/api/client";
import { listStrategies, strategyKeys } from "@/api/strategies";

export function StrategiesRoute() {
  const q = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });

  return (
    <>
      <Topbar
        title="Strategies"
        sub={subtitleFor(q)}
      />

      <FilterBar />

      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : q.data && q.data.length === 0 ? (
          <EmptyState />
        ) : (
          <StrategiesTable items={q.data ?? []} />
        )}
      </Card>
    </>
  );
}

function subtitleFor(q: ReturnType<typeof useQuery>) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load strategies";
  const data = q.data as { length: number } | undefined;
  if (!data) return "";
  const n = data.length;
  return `${n} ${n === 1 ? "bundle" : "bundles"}`;
}

function FilterBar() {
  return (
    <div className="flex flex-col xl:flex-row xl:items-center xl:justify-between mb-4 gap-3">
      <div className="flex flex-col xl:flex-row xl:items-center gap-2.5 min-w-0">
        <div className="flex items-center gap-2 px-3 py-2 bg-surface-elev border border-border rounded w-full xl:w-[280px] max-w-full text-text-3 opacity-50">
          <Icon name="search" size={14} />
          <input
            className="bg-transparent border-0 outline-0 flex-1 text-[13px] text-text placeholder:text-text-3 disabled:cursor-not-allowed"
            placeholder="Filter by name…"
            disabled
            aria-label="Filter strategies (coming soon)"
            title="Filtering ships with Plan 5 (Findings + Polish)"
          />
        </div>
        <select
          className="bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text-2 disabled:opacity-50 disabled:cursor-not-allowed"
          disabled
          title="Status filter ships with Plan 5 (Findings + Polish)"
        >
          <option>All status</option>
        </select>
        <select
          className="bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text-2 disabled:opacity-50 disabled:cursor-not-allowed"
          disabled
          title="Template filter ships with Plan 5 (Findings + Polish)"
        >
          <option>All templates</option>
        </select>
      </div>
      <div className="flex flex-col sm:flex-row sm:items-center gap-2">
        <Link
          to="/strategies/new"
          className="inline-flex items-center justify-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text-2 hover:text-text hover:border-text-3 transition-colors"
        >
          <Icon name="plus" size={13} /> New from template
        </Link>
        <Link
          to="/setup"
          className="inline-flex items-center justify-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
        >
          <Icon name="plus" size={13} /> New strategy
        </Link>
      </div>
    </div>
  );
}

function StrategiesTable({
  items,
}: {
  items: {
    agent_id: string;
    template: string;
    model?: string;
  }[];
}) {
  return (
    <table className="w-full">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 px-5">Agent ID</th>
          <th className="font-normal py-2.5 px-3">Template</th>
          <th className="font-normal py-2.5 px-3">Model</th>
          <th className="font-normal py-2.5 px-3">Status</th>
          <th className="font-normal py-2.5 px-5"></th>
        </tr>
      </thead>
      <tbody>
        {items.map((row) => (
          <tr
            key={row.agent_id}
            className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
          >
            <td className="py-3 px-5 font-mono text-text">
              <Link
                to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                className="text-text hover:underline"
              >
                {row.agent_id}
              </Link>
            </td>
            <td className="py-3 px-3 text-text-2">{row.template}</td>
            <td className="py-3 px-3 font-mono text-text-2 text-[12px]">
              {row.model ?? <span className="text-text-3 italic">—</span>}
            </td>
            <td className="py-3 px-3">
              <Pill tone="gold">
                <span className="w-1.5 h-1.5 rounded-full bg-gold" /> validated
              </Pill>
            </td>
            <td className="py-3 px-5 text-text-3 text-right">
              <Link
                to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                className="text-text-3 hover:text-text"
                aria-label={`Open inspector for ${row.agent_id}`}
              >
                Inspector →
              </Link>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-48 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-32 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-24 rounded bg-surface-elev animate-pulse" />
        </div>
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="px-6 py-16 text-center text-text-2">
      <div className="font-serif italic text-[28px] text-text-3 mb-3">
        no bundles yet
      </div>
      <p className="m-0 max-w-md mx-auto leading-snug">
        Strategies you create with{" "}
        <code className="text-text font-mono">xvn strategy new</code> or the
        wizard will appear here. Until then, the engine is idle.
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
        couldn't reach the engine
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
