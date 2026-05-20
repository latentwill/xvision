import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import {
  ServerPagerStrip,
  useServerPagination,
} from "@/components/primitives/useServerPagination";
import {
  ResponsiveListCard,
  useListState,
  useListUrlState,
  type FilterDef,
  type SortOption,
} from "@/components/lists";
import { MListRow } from "@/components/lists/MListRow";
import { ApiError } from "@/api/client";
import {
  listStrategiesPaged,
  strategyKeys,
  type StrategiesPage,
  type StrategyListItem,
} from "@/api/strategies";
import { formatCadence } from "@/lib/format";

const SORT_OPTIONS: SortOption[] = [
  { value: "added", label: "Recently added" },
  { value: "name", label: "Name A → Z" },
];

const SHAPE_FILTER: FilterDef = {
  id: "shape",
  label: "Pipeline shape",
  options: [
    { value: "all", label: "All shapes" },
    { value: "single", label: "Trader-only (single agent)" },
    { value: "multi", label: "Multi-agent" },
  ],
};

/** True if a `StrategyListItem` looks like a multi-agent strategy.
 *
 * `StrategyListItem` doesn't expose `agents[]` directly — the runtime
 * shape is summarised via the parallel `providers[]` / `models[]`
 * arrays (legacy) and `provider_models[]` (preferred). We treat a
 * strategy with more than one (provider, model) pair as multi-agent
 * and otherwise as trader-only. Returns the agent count as a number so
 * the row meta can show "1 agent" / "3 agents" without recomputing.
 */
function agentCount(row: StrategyListItem): number {
  if (row.provider_models && row.provider_models.length > 0) {
    return row.provider_models.length;
  }
  // Legacy parallel arrays. Use the longer of the two so we count an
  // agent even when one side is partially populated.
  const providers = row.providers?.length ?? 0;
  const models = row.models?.length ?? 0;
  return Math.max(providers, models);
}

function shapeOf(row: StrategyListItem): "single" | "multi" {
  return agentCount(row) > 1 ? "multi" : "single";
}

export function StrategiesRoute() {
  // QA-round-7 backend-pagination follow-up (#386 gap): page-size +
  // page-nav drive `limit`/`offset` in the TanStack query key so page
  // changes refetch the next slice instead of slicing one big
  // client-side response. Recency-first ULID DESC sort is enforced
  // upstream in `engine::api::strategy::list_paged`.
  const [totalFromServer, setTotalFromServer] = useState(0);
  const pager = useServerPagination(totalFromServer);
  const params = { limit: pager.limit, offset: pager.offset };
  const q = useQuery({
    queryKey: strategyKeys.list(params),
    queryFn: () => listStrategiesPaged(params),
    placeholderData: (prev) => prev,
  });
  useEffect(() => {
    if (q.data?.total !== undefined && q.data.total !== totalFromServer) {
      setTotalFromServer(q.data.total);
    }
  }, [q.data?.total, totalFromServer]);

  const items = q.data?.items ?? [];
  const total = q.data?.total ?? 0;

  // Template filter is derived from the observed templates on the
  // current page. Stable order: template names sorted alphabetically.
  const templateFilter: FilterDef = useMemo(() => {
    const seen = new Set<string>();
    const options: { value: string; label: string }[] = [
      { value: "all", label: "All templates" },
    ];
    items.forEach((row) => {
      const t = row.template?.trim();
      if (t && !seen.has(t)) seen.add(t);
    });
    Array.from(seen)
      .sort((a, b) => a.localeCompare(b))
      .forEach((t) => options.push({ value: t, label: t }));
    return { id: "template", label: "Template", options };
  }, [items]);

  const list = useListState<StrategyListItem>({
    rows: items,
    filters: [SHAPE_FILTER, templateFilter],
    sortOptions: SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const shape = values.shape ?? "all";
      if (shape !== "all" && shapeOf(row) !== shape) return false;
      const template = values.template ?? "all";
      if (template !== "all" && row.template !== template) return false;
      const needle = query.trim().toLowerCase();
      if (needle.length === 0) return true;
      const name = (row.display_name || "").toLowerCase();
      const idPrefix = row.agent_id.slice(0, 12).toLowerCase();
      const fullId = row.agent_id.toLowerCase();
      return (
        name.includes(needle) ||
        idPrefix.includes(needle) ||
        fullId.includes(needle)
      );
    },
    sortFn: (rows, key) => {
      switch (key) {
        case "name":
          return rows.sort((a, b) =>
            (a.display_name || "").localeCompare(b.display_name || ""),
          );
        case "added":
        default:
          // Server already returns ULID DESC (recently added first).
          // No-op so the default order matches what came down.
          return rows;
      }
    },
  });
  useListUrlState("strategies", list);

  const subtitle = subtitleFor(q, total, list.totalRows, list.rows.length);

  const desktopColumns = [
    { key: "name", label: "Name" },
    { key: "template", label: "Template" },
    { key: "shape", label: "Shape" },
    { key: "tags", label: "Tags" },
    { key: "cadence", label: "Cadence" },
    { key: "model", label: "Model" },
    { key: "status", label: "Status" },
    { key: "actions", label: "" },
  ];

  return (
    <>
      <Topbar title="Strategies" sub={subtitle} />

      <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
        <Link
          to="/strategies/new"
          className="inline-flex w-full items-center justify-center gap-2 rounded border border-border px-3.5 py-1.5 text-[13px] font-medium text-text-2 transition-colors hover:border-text-3 hover:text-text sm:w-auto"
        >
          <Icon name="plus" size={13} /> New strategy
        </Link>
        <Link
          to="/strategies/new"
          className="inline-flex w-full items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:w-auto"
        >
          <Icon name="plus" size={13} /> Open form
        </Link>
      </div>

      <ResponsiveListCard<StrategyListItem>
        listId="strategies"
        title="Strategies"
        count={list.totalRows}
        toolbar={{
          search: { ...list.search, placeholder: "Search name or id…" },
          filters: list.filters,
          sort: list.sort,
          clearAll: list.clearAll,
        }}
        columns={desktopColumns}
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
        empty="No strategies match these filters."
        emptyAction={
          <Link
            to="/strategies/new"
            className="inline-flex items-center gap-1.5 rounded border border-gold px-3 py-1.5 text-[12px] font-medium text-gold hover:bg-gold/10"
          >
            <Icon name="plus" size={11} /> New strategy
          </Link>
        }
        renderRow={(row) => <DesktopRow key={row.agent_id} row={row} />}
        renderMobileRow={(row) => (
          <MListRow
            key={row.agent_id}
            onClick={() => {
              window.location.href = `/authoring/${encodeURIComponent(row.agent_id)}`;
            }}
            title={row.display_name || "Untitled strategy"}
            badge={shapeOf(row) === "multi" ? "multi-agent" : "trader-only"}
            badgeColor={shapeOf(row) === "multi" ? "info" : "muted"}
            subtitle={`${row.template} · ${formatCadence(row.decision_cadence_minutes)}`}
            meta={modelSummary(row)}
            rightTop={
              agentCount(row) > 0
                ? `${agentCount(row)} ${agentCount(row) === 1 ? "agent" : "agents"}`
                : undefined
            }
            rightSub="draft"
          />
        )}
      />

      <ServerPagerStrip
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
  totalRows: number,
  visibleRows: number,
) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load strategies";
  if (total === 0) return "0 strategies";
  if (visibleRows === totalRows) {
    return `${total} ${total === 1 ? "strategy" : "strategies"}`;
  }
  return `${visibleRows} of ${totalRows} strategies`;
}

function DesktopRow({ row }: { row: StrategyListItem }) {
  const shape = shapeOf(row);
  return (
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
        <Pill tone={shape === "multi" ? "info" : "default"}>
          {shape === "multi"
            ? `multi · ${agentCount(row)} agents`
            : "trader-only"}
        </Pill>
      </td>
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
  );
}

function displayName(row: StrategyListItem) {
  return row.display_name || "Untitled strategy";
}

function modelSummary(row: StrategyListItem): string {
  if (row.model && row.model.trim().length > 0) return row.model;
  const pairs = row.provider_models ?? [];
  if (pairs.length > 0) {
    return pairs.map((p) => p.model).join(" · ");
  }
  const models = row.models ?? [];
  if (models.length > 0) return models.join(" · ");
  return "—";
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

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
