import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import {
  ListPagination,
  useServerPagination,
} from "@/components/primitives/ListPagination";
import { ApiError } from "@/api/client";
import {
  listStrategiesPaged,
  strategyKeys,
  type StrategiesPage,
  type StrategyListItem,
} from "@/api/strategies";
import { formatCadence } from "@/lib/format";

export function StrategiesRoute() {
  // QA-round-7 backend-pagination follow-up (#386 gap): page-size +
  // page-nav drive `limit`/`offset` in the TanStack query key so page
  // changes refetch the next slice instead of slicing one big
  // client-side response. Recency-first ULID DESC sort is enforced
  // upstream in `engine::api::strategy::list_paged`.
  //
  // The pager state is seeded with `total = 0`; once the first
  // response lands, `useServerPagination` clamps the page if needed
  // (the hook reads `total` from props on every render). Passing a
  // stable initial value here lets the very first request go out at
  // `(limit=50, offset=0)` without flicker.
  const [totalFromServer, setTotalFromServer] = useState(0);
  const pager = useServerPagination(totalFromServer);
  const params = { limit: pager.limit, offset: pager.offset };
  const q = useQuery({
    queryKey: strategyKeys.list(params),
    queryFn: () => listStrategiesPaged(params),
    placeholderData: (prev) => prev,
  });
  // Keep the pager's notion of `total` in sync with the server.
  useEffect(() => {
    if (q.data?.total !== undefined && q.data.total !== totalFromServer) {
      setTotalFromServer(q.data.total);
    }
  }, [q.data?.total, totalFromServer]);

  const items = q.data?.items ?? [];
  const total = q.data?.total ?? 0;

  return (
    <>
      <Topbar title="Strategies" sub={subtitleFor(q, total)} />

      <FilterBar />

      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : total === 0 ? (
          <EmptyState />
        ) : (
          <StrategiesTable items={items} />
        )}
      </Card>

      <ListPagination
        total={total}
        page={pager.page}
        pageSize={pager.pageSize}
        onPageChange={pager.setPage}
        onPageSizeChange={pager.setPageSize}
        itemLabel="strategies"
      />
    </>
  );
}

function subtitleFor(
  q: { isPending: boolean; isError: boolean; data?: StrategiesPage },
  total: number,
) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load strategies";
  return `${total} ${total === 1 ? "strategy" : "strategies"}`;
}

function FilterBar() {
  return (
    <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="text-[13px] text-text-3">Latest strategy drafts</div>
      <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto">
        <Link
          to="/strategies/new"
          className="inline-flex flex-1 items-center justify-center gap-2 rounded border border-border px-3.5 py-2 text-[13px] font-medium text-text-2 transition-colors hover:border-text-3 hover:text-text sm:flex-none"
        >
          <Icon name="plus" size={13} /> New strategy
        </Link>
        <Link
          to="/strategies/new"
          className="inline-flex flex-1 items-center justify-center gap-2 rounded bg-gold px-3.5 py-2 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:flex-none"
        >
          <Icon name="plus" size={13} /> Open form
        </Link>
      </div>
    </div>
  );
}

function StrategiesTable({
  items,
}: {
  items: StrategyListItem[];
}) {
  return (
    <>
      <div className="divide-y divide-border-soft md:hidden">
        {items.map((row) => (
          <article key={row.agent_id} className="px-4 py-3">
            <div className="mb-1.5 flex items-start justify-between gap-2">
              <div>
                <Link
                  to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                  className="text-[15px] text-text hover:underline"
                >
                  {row.display_name || "Untitled strategy"}
                </Link>
              </div>
              <Pill>
                <span className="h-1.5 w-1.5 rounded-full bg-text-3" /> draft
              </Pill>
            </div>

            <div className="mt-1 text-[12px] text-text-2">
              {row.template} · {formatCadence(row.decision_cadence_minutes)}
            </div>
            <div className="mt-1 break-all font-mono text-[12px] text-text-2">
              {row.model ?? <span className="italic text-text-3">—</span>}
            </div>
            <div className="mt-2">
              <TagList tags={row.tags ?? []} />
            </div>

            <div className="mt-2.5">
              <Link
                to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                className="text-[13px] text-text-3 hover:text-text"
                aria-label={`Open inspector for ${displayName(row)}`}
              >
                Inspector →
              </Link>
            </div>
          </article>
        ))}
      </div>

      <div className="hidden overflow-x-auto md:block">
        <table className="w-full">
          <thead>
            <tr className="border-b border-border-soft text-left text-[12px] text-text-2">
              <th className="px-3 py-2.5 font-normal">Name</th>
              <th className="px-3 py-2.5 font-normal">Template</th>
              <th className="px-3 py-2.5 font-normal">Tags</th>
              <th className="px-3 py-2.5 font-normal">Cadence</th>
              <th className="px-3 py-2.5 font-normal">Model</th>
              <th className="px-3 py-2.5 font-normal">Status</th>
              <th className="px-5 py-2.5 font-normal"></th>
            </tr>
          </thead>
          <tbody>
            {items.map((row) => (
              <tr
                key={row.agent_id}
                className="border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover"
              >
                <td className="px-3 py-3 text-text">
                  <Link
                    to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                    className="break-all text-text hover:underline"
                  >
                    {row.display_name || "Untitled strategy"}
                  </Link>
                </td>
                <td className="px-3 py-3 text-text-2">{row.template}</td>
                <td className="px-3 py-3">
                  <TagList tags={row.tags ?? []} />
                </td>
                <td className="px-3 py-3 font-mono text-[12px] text-text-2">
                  {formatCadence(row.decision_cadence_minutes)}
                </td>
                <td className="max-w-[180px] px-3 py-3 break-all font-mono text-[12px] text-text-2">
                  {row.model ?? <span className="italic text-text-3">—</span>}
                </td>
                <td className="px-3 py-3">
                  <Pill>
                    <span className="h-1.5 w-1.5 rounded-full bg-text-3" /> draft
                  </Pill>
                </td>
                <td className="px-5 py-3 text-right text-text-3">
                  <Link
                    to={`/authoring/${encodeURIComponent(row.agent_id)}`}
                    className="text-text-3 hover:text-text"
                    aria-label={`Open inspector for ${displayName(row)}`}
                  >
                    Inspector →
                  </Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

function displayName(row: StrategyListItem) {
  return row.display_name || "Untitled strategy";
}

function TagList({ tags }: { tags: string[] }) {
  if (tags.length === 0) {
    return <span className="text-[12px] italic text-text-3">—</span>;
  }
  const visible = tags.slice(0, 3);
  const extra = tags.length - visible.length;
  return (
    <div className="flex max-w-[280px] flex-wrap gap-1.5">
      {visible.map((tag) => (
        <span
          key={tag}
          className="max-w-[150px] truncate rounded border border-border-soft bg-surface-elev px-1.5 py-0.5 font-mono text-[11px] leading-tight text-text-2"
          title={tag}
        >
          {tag}
        </span>
      ))}
      {extra > 0 ? <Pill tone="default">+{extra}</Pill> : null}
    </div>
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
        no strategies yet
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
