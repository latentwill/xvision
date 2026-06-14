import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import { ApiError } from "@/api/client";
import { listScenariosPaged, scenarioKeys } from "@/api/scenarios";
import {
  ServerPagerStrip,
  useServerPagination,
} from "@/components/primitives/useServerPagination";
import {
  ResponsiveListCard,
  useListColumns,
  useListState,
  useListUrlState,
  type FilterDef,
  type SortOption,
} from "@/components/lists";
import { MListRow } from "@/components/lists/MListRow";
import type {
  Scenario,
  ScenarioSource,
  ListScenariosFilter,
} from "@/api/types.gen";

const SOURCE_TONE: Record<ScenarioSource, "default" | "gold" | "info" | "warn"> = {
  Canonical: "gold",
  User: "info",
  Clone: "default",
  Generated: "warn",
  // Frozen scenarios are materialised from a completed Live run via the
  // "Save as historical scenario" action (Alpaca-Live plan §Phase E).
  // Visually distinguish from `User` (manually authored) — use the info
  // tone for parity with `User` for now; a dedicated tone can land when
  // the Frozen surface picks up its own iconography.
  Frozen: "info",
};

const OPTIMIZER_SCENARIO_TAG = "source:autooptimizer";

// The source filter lives in URL state via `useListUrlState`, so we
// hold it as a lowercase token in the FilterDef options.
const SOURCE_FILTER: FilterDef = {
  id: "source",
  label: "Source",
  options: [
    { value: "any", label: "Any source" },
    { value: "canonical", label: "Canonical" },
    { value: "user", label: "User" },
    { value: "clone", label: "Clone" },
    { value: "generated", label: "Generated" },
    { value: "optimizer", label: "Optimizer" },
    { value: "frozen", label: "Frozen" },
  ],
};

const ARCHIVED_FILTER: FilterDef = {
  id: "archived",
  label: "Archived",
  options: [
    { value: "exclude", label: "Hide archived" },
    { value: "include", label: "Include archived" },
  ],
};

const SORT_OPTIONS: SortOption[] = [
  // Backend already returns rows DESC by creation; the recency default
  // mirrors that ordering. Other options sort client-side over the
  // currently-paged slice (acceptable for v1 — the list-component spec
  // will move sort into the API once we have a multi-field column).
  { value: "added", label: "Recently added" },
  { value: "name", label: "Name A → Z" },
  { value: "name-desc", label: "Name Z → A" },
];

function sourceTokenToFilter(token: string): ScenarioSource | null {
  switch (token) {
    case "canonical":
      return "Canonical";
    case "user":
      return "User";
    case "clone":
      return "Clone";
    case "generated":
      return "Generated";
    case "frozen":
      return "Frozen";
    default:
      return null;
  }
}

function marketOf(row: Scenario): string {
  return `${row.asset_class} / ${row.quote_currency}`;
}

export function ScenariosRoute() {
  const [sourceToken, setSourceToken] = useState<string>("any");
  const [archivedToken, setArchivedToken] = useState<string>("exclude");

  // QA-round-7 backend-pagination follow-up (#386 gap): page-size +
  // page-nav drive `limit`/`offset` in the query key.
  const [totalFromServer, setTotalFromServer] = useState(0);
  const pager = useServerPagination(totalFromServer);

  const filter: ListScenariosFilter = useMemo(
    () => ({
      source:
        sourceToken === "optimizer"
          ? null
          : sourceTokenToFilter(sourceToken),
      tags: sourceToken === "optimizer" ? [OPTIMIZER_SCENARIO_TAG] : [],
      exclude_tags:
        sourceToken === "optimizer" ? [] : [OPTIMIZER_SCENARIO_TAG],
      include_archived: archivedToken === "include",
      parent_scenario_id: null,
      limit: pager.limit,
      offset: pager.offset,
    }),
    [sourceToken, archivedToken, pager.limit, pager.offset],
  );

  const q = useQuery({
    queryKey: scenarioKeys.list(filter),
    queryFn: () => listScenariosPaged(filter),
    placeholderData: (prev) => prev,
  });
  useEffect(() => {
    if (q.data?.total !== undefined && q.data.total !== totalFromServer) {
      setTotalFromServer(q.data.total);
    }
  }, [q.data?.total, totalFromServer]);

  const items = q.data?.items ?? [];
  const total = q.data?.total ?? 0;

  // Backend already filtered by source + include_archived; this list
  // state drives the toolbar (search + sort + active chips) over the
  // currently-paged slice. The Source / Archived filter values are
  // mirrored into local state so the URL → backend filter loop stays
  // intact.
  const list = useListState<Scenario>({
    rows: items,
    filters: [SOURCE_FILTER, ARCHIVED_FILTER],
    sortOptions: SORT_OPTIONS,
    filterFn: (row, query) => {
      const q = query.trim().toLowerCase();
      if (q.length === 0) return true;
      if (row.display_name.toLowerCase().includes(q)) return true;
      if (marketOf(row).toLowerCase().includes(q)) return true;
      // Granularity no longer surfaced on operator scenario rows
      // (scenarios are 1h-fixed today — filtering by it would be
      // meaningless to the operator). If granularity becomes
      // configurable per-scenario again, restore this branch.
      return false;
    },
    sortFn: (rows, key) => {
      switch (key) {
        case "name":
          return rows.sort((a, b) =>
            a.display_name.localeCompare(b.display_name),
          );
        case "name-desc":
          return rows.sort((a, b) =>
            b.display_name.localeCompare(a.display_name),
          );
        case "added":
        default:
          // Backend recency order; preserve as-returned.
          return rows;
      }
    },
  });
  useListUrlState("scenarios", list);

  // Bridge filter values ↔ local state. `useListUrlState` writes the
  // URL; this back-edge keeps the backend query in sync when the user
  // flips the filter.
  const sourceValue =
    list.filters.find((f) => f.def.id === "source")?.value ?? "any";
  const archivedValue =
    list.filters.find((f) => f.def.id === "archived")?.value ?? "exclude";
  useEffect(() => {
    if (sourceValue !== sourceToken) setSourceToken(sourceValue);
  }, [sourceValue, sourceToken]);
  useEffect(() => {
    if (archivedValue !== archivedToken) setArchivedToken(archivedValue);
  }, [archivedValue, archivedToken]);

  const navigate = useNavigate();
  function go(id: string) {
    navigate(`/scenarios/${id}`);
  }

  // Granularity column removed per QA: scenarios are 1h-fixed
  // today and the value was a permanent "1h" across every row.
  // Showing it suggested it was configurable; it isn't. The new
  // `Created` column surfaces the existing `scenarios.created_at`
  // column (migration 011) so operators can sort/scan recency at a
  // glance — also a QA item from the same session.
  const desktopColumns = [
    { key: "name",    label: "Name",    essential: true, estWidth: 200 },
    { key: "market",  label: "Market",  priority: 4,     estWidth: 130 },
    { key: "window",  label: "Window",  priority: 3,     estWidth: 110 },
    { key: "created", label: "Created", priority: 1,     estWidth: 100 },
    { key: "source",  label: "Source",  priority: 2,     estWidth: 100 },
    { key: "tags",    label: "Tags",    priority: 0,     estWidth: 150 },
    { key: "actions", label: "",        essential: true, estWidth: 60  },
  ];
  const columnState = useListColumns("scenarios", desktopColumns);

  return (
    <>
      <Topbar title="Scenarios" sub={subtitleFor(q, total, list.rows.length, archivedToken)} />

      <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
        <Link
          to="/scenarios/new"
          className="inline-flex w-full items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:w-auto motion-safe:active:scale-[0.96]"
        >
          <Icon name="plus" size={13} /> New scenario
        </Link>
      </div>

      <ResponsiveListCard<Scenario>
        listId="scenarios"
        toolbar={{
          search: { ...list.search, placeholder: "Search scenarios…" },
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
        empty={
          total === 0
            ? "No scenarios yet. Create one to start running evals."
            : "No scenarios match these filters."
        }
        emptyAction={
          total === 0 ? (
            <Link
              to="/scenarios/new"
              className="inline-flex items-center gap-1.5 rounded border border-gold px-3 py-1.5 text-[12px] font-medium text-gold hover:bg-gold/10"
            >
              <Icon name="plus" size={11} /> New scenario
            </Link>
          ) : null
        }
        renderRow={(row, _i, visibleKeys) => (
          <DesktopRow key={row.id} row={row} onGo={go} visibleKeys={visibleKeys} />
        )}
        renderMobileRow={(row) => (
          <MListRow
            key={row.id}
            onClick={() => go(row.id)}
            title={row.display_name}
            badge={row.source}
            badgeColor={badgeColorFor(row.source)}
            subtitle={marketOf(row)}
            // Granularity dropped from the mobile row meta line per
            // QA — it was redundant ("1h" on every row).
            meta={fmtWindow(row.time_window.start, row.time_window.end)}
          />
        )}
      />

      <ServerPagerStrip
        total={total}
        page={pager.page}
        pageSize={pager.pageSize}
        onPageChange={pager.setPage}
        onPageSizeChange={pager.setPageSize}
        itemLabel="scenarios"
      />
    </>
  );
}

function subtitleFor(
  q: { isPending: boolean; isError: boolean },
  total: number,
  visibleRows: number,
  archivedToken: string,
): string {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load scenarios";
  const base =
    total === 0
      ? "0 scenarios"
      : visibleRows === total
        ? `${total} ${total === 1 ? "scenario" : "scenarios"}`
        : `${visibleRows} of ${total} scenarios`;
  return archivedToken === "exclude" ? `${base} · archived hidden` : base;
}

function DesktopRow({
  row,
  onGo,
  visibleKeys,
}: {
  row: Scenario;
  onGo: (id: string) => void;
  visibleKeys: Set<string>;
}) {
  return (
    <tr
      onClick={() => onGo(row.id)}
      className="xvn-row-in cursor-pointer border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover focus-within:bg-surface-hover"
    >
      <td className="py-3 pl-5 pr-3">
        <Link
          to={`/scenarios/${row.id}`}
          onClick={(e) => e.stopPropagation()}
          className="block text-[13px] leading-tight text-text hover:underline"
        >
          {row.display_name}
        </Link>
        {row.description ? (
          <div className="mt-0.5 max-w-[260px] truncate text-[11px] leading-tight text-text-3">
            {row.description}
          </div>
        ) : null}
      </td>
      {visibleKeys.has("market") ? (
        <td className="px-3 py-3 font-mono text-[12px] text-text-2">
          {marketOf(row)}
        </td>
      ) : null}
      {visibleKeys.has("window") ? (
        <td className="whitespace-nowrap px-3 py-3 text-[12px] text-text-2">
          {fmtWindow(row.time_window.start, row.time_window.end)}
        </td>
      ) : null}
      {/* Granularity <td> removed — see column-list comment above. */}
      {visibleKeys.has("created") ? (
        <td
          className="whitespace-nowrap px-3 py-3 text-[12px] text-text-2"
          title={row.created_at}
        >
          {fmtCreated(row.created_at)}
        </td>
      ) : null}
      {visibleKeys.has("source") ? (
        <td className="px-3 py-3">
          <SourcePill source={row.source} />
        </td>
      ) : null}
      {visibleKeys.has("tags") ? (
        <td className="px-3 py-3">
          <div className="flex flex-wrap gap-1">
            {row.tags.length > 0
              ? row.tags.slice(0, 3).map((tag) => (
                  <Pill key={tag} tone="default">
                    {tag}
                  </Pill>
                ))
              : <span className="text-[11px] text-text-3">—</span>}
            {row.tags.length > 3 ? (
              <Pill tone="default">+{row.tags.length - 3}</Pill>
            ) : null}
          </div>
        </td>
      ) : null}
      <td
        className="px-5 py-3 text-right"
        onClick={(e) => e.stopPropagation()}
      >
        <Link
          to={`/scenarios/${row.id}`}
          className="text-[12px] text-text-3 transition-colors hover:text-text"
          tabIndex={-1}
        >
          View →
        </Link>
      </td>
    </tr>
  );
}

function SourcePill({ source }: { source: ScenarioSource }) {
  const tone = SOURCE_TONE[source] ?? "default";
  return <Pill tone={tone}>{source}</Pill>;
}

function badgeColorFor(
  source: ScenarioSource,
): "gold" | "warn" | "danger" | "info" | "muted" {
  switch (SOURCE_TONE[source] ?? "default") {
    case "gold":
      return "gold";
    case "warn":
      return "warn";
    case "info":
      return "info";
    default:
      return "muted";
  }
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

/// Format `scenarios.created_at` (ISO 8601 from the API) as a short
/// month-day-year so the new Created column fits next to the time
/// window column without wrapping. Hover shows the full ISO via the
/// `title` attribute on the cell.
function fmtCreated(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
