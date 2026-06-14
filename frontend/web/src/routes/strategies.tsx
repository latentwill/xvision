import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { StrategiesFolderView } from "./strategies-folder";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import { SignalActionMenu } from "@/components/primitives/SignalMenu";
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
import { ApiError } from "@/api/client";
import { evalKeys, listRuns } from "@/api/eval";
import {
  cloneStrategy,
  listStrategiesPaged,
  strategyKeys,
  type StrategiesPage,
  type Strategy,
  type StrategyListItem,
} from "@/api/strategies";
import { strategyEvalCoverage } from "@/features/strategies/coverage";
import { strategyLeaderboard } from "@/features/home/leaderboard";
import { formatCadence } from "@/lib/format";

const SORT_OPTIONS: SortOption[] = [
  { value: "added", label: "Recently added" },
  { value: "leaderboard", label: "Leaderboard" },
  { value: "name", label: "Name A → Z" },
];

const LEADERBOARD_RUNS_PAGE = { limit: 100 } as const;

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
 * Newer backends expose `agent_count`, which is the only trustworthy
 * number because deterministic filters are not agents. Older backends
 * only expose provider/model summaries, so keep a conservative fallback.
 */
function agentCount(row: StrategyListItem): number {
  if (typeof row.agent_count === "number") {
    return row.agent_count;
  }
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

function filterLabel(row: StrategyListItem): string | null {
  const count = row.filter_count ?? 0;
  if (count <= 0) return null;
  return `${count} ${count === 1 ? "filter" : "filters"}`;
}

function decisionMode(row: StrategyListItem): {
  label: string;
  pillTone: "default" | "info" | "warn" | "danger";
  badgeColor: "info" | "muted" | "warn" | "danger";
} {
  const agents = agentCount(row);
  if (row.activation_mode === "compiled_rules") {
    return { label: "rules-only", pillTone: "warn", badgeColor: "warn" };
  }
  if (row.activation_mode === "filter_gated" || (row.filter_count ?? 0) > 0) {
    return agents > 0
      ? { label: "filter-gated agent", pillTone: "info", badgeColor: "info" }
      : { label: "missing agent", pillTone: "danger", badgeColor: "danger" };
  }
  return agents > 0
    ? { label: "agent-direct", pillTone: "default", badgeColor: "muted" }
    : { label: "missing agent", pillTone: "danger", badgeColor: "danger" };
}

type StrategiesView = "list" | "folder";

export function StrategiesRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const view: StrategiesView =
    searchParams.get("view") === "folder" ? "folder" : "list";

  function setView(next: StrategiesView) {
    setSearchParams(
      (prev) => {
        const next2 = new URLSearchParams(prev);
        if (next === "list") {
          next2.delete("view");
        } else {
          next2.set("view", next);
        }
        return next2;
      },
      { replace: false },
    );
  }

  const topbarSub =
    view === "folder"
      ? "Notes, docs, and reference files the wizard can quote back to you."
      : undefined;

  return (
    <>
      <Topbar title="Strategies" sub={topbarSub} />

      <div className="mb-4 flex items-center gap-3">
        <ViewToggle view={view} onChange={setView} />
      </div>

      {view === "folder" ? (
        <StrategiesFolderView />
      ) : (
        <StrategiesListView />
      )}
    </>
  );
}

/** Inline segmented control for List / Folder views. */
function ViewToggle({
  view,
  onChange,
}: {
  view: StrategiesView;
  onChange: (v: StrategiesView) => void;
}) {
  const views: [StrategiesView, string][] = [
    ["list", "List"],
    ["folder", "Folder"],
  ];
  return (
    <div
      role="tablist"
      aria-label="Strategies view"
      className="flex rounded border border-border overflow-hidden"
    >
      {views.map(([v, label]) => (
        <button
          key={v}
          type="button"
          role="tab"
          aria-selected={view === v}
          onClick={() => onChange(v)}
          className={[
            "px-3 py-1 text-[12.5px] font-medium transition-colors",
            view === v
              ? "bg-surface-elev text-text"
              : "text-text-3 hover:text-text-2",
          ].join(" ")}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** List view body — the original StrategiesRoute content. */
function StrategiesListView() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const clone = useMutation<Strategy, unknown, StrategyListItem>({
    mutationFn: (row) =>
      cloneStrategy(row.agent_id, {
        display_name: `${displayName(row)} (clone)`,
      }),
    onSuccess: (strategy) => {
      navigate(`/strategies/${encodeURIComponent(strategy.manifest.id)}`);
    },
  });
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

  const items = (q.data?.items ?? []).filter(
    (row) => !isLegacyAgentlessExample(row),
  );
  const total = q.data?.total ?? 0;
  const leaderboardRuns = useQuery({
    queryKey: evalKeys.runs(LEADERBOARD_RUNS_PAGE),
    queryFn: () => listRuns(LEADERBOARD_RUNS_PAGE),
    enabled: searchParams.get("sort") === "leaderboard",
  });

  const list = useListState<StrategyListItem>({
    rows: items,
    filters: [SHAPE_FILTER],
    sortOptions: SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const shape = values.shape ?? "all";
      if (shape !== "all" && shapeOf(row) !== shape) return false;
      const needle = query.trim().toLowerCase();
      if (needle.length === 0) return true;
      return strategySearchTerms(row).some((term) =>
        term.toLowerCase().includes(needle),
      );
    },
    sortFn: (rows, key) => {
      switch (key) {
        case "leaderboard":
          return sortByLeaderboard(rows, leaderboardRuns.data ?? []);
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

  // Created column — strategies are filesystem artifacts, not DB
  // rows, so there's no `created_at` column to plumb. Instead, we
  // parse the millisecond timestamp embedded in the leading 10
  // chars of the strategy's ULID `agent_id`. That's exactly the
  // moment the strategy was minted; far more reliable than reading
  // file mtime (mtime changes on every edit). `ulidToCreatedAt`
  // falls back to `null` on malformed legacy ids so the column
  // gracefully shows `—` rather than crashing the row.
  const desktopColumns = [
    { key: "name",    label: "Name",       essential: true, estWidth: 200 },
    { key: "shape",   label: "Shape",      priority: 3,     estWidth: 110 },
    { key: "tags",    label: "Tags",       priority: 2,     estWidth: 150 },
    { key: "cadence", label: "Time frame", priority: 1,     estWidth: 100 },
    { key: "model",   label: "Model",      priority: 2,     estWidth: 140 },
    { key: "created", label: "Created",    priority: 0,     estWidth: 100 },
    { key: "actions", label: "",           essential: true, estWidth: 60  },
  ];
  const columnState = useListColumns("strategies", desktopColumns);

  return (
    <>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <span className="text-[12.5px] text-text-3">{subtitle}</span>
        <div className="flex flex-wrap items-center gap-2">
          <NewStrategyButton
            pending={false}
            onClick={() => navigate("/strategies/new")}
          />
        </div>
      </div>

      <ResponsiveListCard<StrategyListItem>
        listId="strategies"
        title="Strategies"
        count={list.totalRows}
        toolbar={{
          search: { ...list.search, placeholder: "Search name, model, capability, author…" },
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
        empty="No strategies match these filters."
        emptyAction={
          <NewStrategyButton
            compact
            pending={false}
            onClick={() => navigate("/strategies/new")}
          />
        }
        renderRow={(row, _i, visibleKeys) => (
          <DesktopRow
            key={row.agent_id}
            row={row}
            visibleKeys={visibleKeys}
            clonePending={
              clone.isPending && clone.variables?.agent_id === row.agent_id
            }
            onClone={() => clone.mutate(row)}
          />
        )}
        renderMobileRow={(row) => (
          <MListRow
            key={row.agent_id}
            onClick={() => {
              window.location.href = `/strategies/${encodeURIComponent(row.agent_id)}`;
            }}
            title={row.display_name || "Untitled strategy"}
            badge={decisionMode(row).label}
            badgeColor={decisionMode(row).badgeColor}
            subtitle={formatCadence(row.decision_cadence_minutes)}
            meta={modelSummary(row)}
            rightTop={
              rowMeta(row)
            }
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

function sortByLeaderboard(
  rows: StrategyListItem[],
  runs: Awaited<ReturnType<typeof listRuns>>,
): StrategyListItem[] {
  const ranked = strategyLeaderboard(strategyEvalCoverage(rows, runs), rows.length);
  const rankById = new Map(
    ranked.map((entry, index) => [entry.strategy.agent_id, index]),
  );
  return rows.sort((a, b) => {
    const aRank = rankById.get(a.agent_id);
    const bRank = rankById.get(b.agent_id);
    if (aRank !== undefined && bRank !== undefined) return aRank - bRank;
    if (aRank !== undefined) return -1;
    if (bRank !== undefined) return 1;
    return 0;
  });
}

function strategySearchTerms(row: StrategyListItem): string[] {
  const terms = [
    row.display_name,
    row.agent_id,
    row.agent_id.slice(0, 12),
    row.template,
    row.creator ?? "",
    row.model ?? "",
    row.activation_mode ?? "",
    row.execution_mode ?? "",
    row.origin ?? "",
    decisionMode(row).label,
    shapeOf(row),
    modelSummary(row),
  ];
  terms.push(...(row.tags ?? []));
  terms.push(...(row.providers ?? []));
  terms.push(...(row.models ?? []));
  terms.push(...(row.capabilities ?? []));
  terms.push(...(row.asset_universe ?? []));
  for (const pair of row.provider_models ?? []) {
    terms.push(pair.provider, pair.model, `${pair.provider}/${pair.model}`);
  }
  return terms.filter((term) => term.trim().length > 0);
}

function NewStrategyButton({
  pending,
  onClick,
  compact = false,
}: {
  pending: boolean;
  onClick: () => void;
  compact?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={pending}
      onClick={onClick}
      className={[
        "inline-flex w-full items-center justify-center gap-2 rounded bg-gold font-medium text-bg transition-colors hover:bg-gold-soft disabled:cursor-not-allowed disabled:opacity-60 sm:w-auto motion-safe:active:scale-[0.96]",
        compact ? "px-3 py-1.5 text-[12px]" : "px-3.5 py-1.5 text-[13px]",
      ].join(" ")}
    >
      <Icon name="plus" size={compact ? 11 : 13} />
      {pending ? "Creating..." : "New Strategy"}
    </button>
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

function DesktopRow({
  row,
  onClone,
  clonePending,
  visibleKeys,
}: {
  row: StrategyListItem;
  onClone: () => void;
  clonePending: boolean;
  visibleKeys: Set<string>;
}) {
  const shape = shapeOf(row);
  const mode = decisionMode(row);
  const navigate = useNavigate();
  return (
    <tr
      key={row.agent_id}
      className="xvn-row-in border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover"
    >
      {visibleKeys.has("name") && (
        <td className="px-3 py-3 text-text">
          <Link
            to={`/strategies/${encodeURIComponent(row.agent_id)}`}
            className="break-all text-text hover:underline"
          >
            {row.display_name || "Untitled strategy"}
          </Link>
        </td>
      )}
      {visibleKeys.has("shape") && (
        <td className="px-3 py-3">
          <Pill tone={mode.pillTone}>{mode.label}</Pill>
          {filterLabel(row) ? (
            <span className="ml-1.5">
              <Pill tone="default">{filterLabel(row)}</Pill>
            </span>
          ) : null}
          {shape === "multi" ? (
            <span className="ml-1.5">
              <Pill tone="default">{agentCount(row)} agents</Pill>
            </span>
          ) : null}
        </td>
      )}
      {visibleKeys.has("tags") && (
        <td className="px-3 py-3">
          <TagList tags={row.tags ?? []} />
        </td>
      )}
      {visibleKeys.has("cadence") && (
        <td className="px-3 py-3 font-mono text-[12px] text-text-2">
          {formatCadence(row.decision_cadence_minutes)}
        </td>
      )}
      {visibleKeys.has("model") && (
        <td className="max-w-[180px] px-3 py-3 break-all font-mono text-[12px] text-text-2">
          {row.model ?? <span className="font-medium text-text-3">—</span>}
        </td>
      )}
      {visibleKeys.has("created") && (
        <td
          className="whitespace-nowrap px-3 py-3 text-[12px] text-text-2"
          title={ulidToCreatedAt(row.agent_id)?.toISOString() ?? row.agent_id}
        >
          {fmtCreatedFromUlid(row.agent_id)}
        </td>
      )}
      {visibleKeys.has("actions") && (
        <td className="px-3 py-3 text-right">
          <SignalActionMenu
            align="right"
            triggerAriaLabel={`Actions for ${displayName(row)}`}
            triggerClassName="inline-flex h-7 w-7 items-center justify-center rounded text-text-3 transition-colors hover:bg-surface-hover hover:text-text"
            triggerLabel={<Icon name="moreH" size={15} />}
            groups={[
              {
                items: [
                  {
                    icon: "openExternal",
                    label: "Open",
                    shortcut: "⌘O",
                    onClick: () => navigate(`/strategies/${encodeURIComponent(row.agent_id)}`),
                  },
                  {
                    icon: "copy",
                    label: "Duplicate",
                    shortcut: "⌘D",
                    disabled: clonePending,
                    onClick: onClone,
                  },
                  {
                    icon: "compare",
                    label: "Compare…",
                    onClick: () => navigate(`/eval-runs/compare?ids=${encodeURIComponent(row.agent_id)}`),
                  },
                ],
              },
              {
                items: [
                  {
                    icon: "fileCode",
                    label: "View raw JSON",
                    onClick: () => navigate(`/strategies/${encodeURIComponent(row.agent_id)}?tab=json`),
                  },
                ],
              },
            ]}
          />
        </td>
      )}
    </tr>
  );
}

function isLegacyAgentlessExample(row: StrategyListItem): boolean {
  return row.agent_id.startsWith("example-") && agentCount(row) === 0;
}

/// Decode a ULID into its embedded millisecond Unix timestamp.
/// Returns `null` for ids that don't parse as Crockford base32 — the
/// strategies library is filesystem-backed and the row's `agent_id`
/// could theoretically be a non-ULID name from a hand-imported
/// fixture; we don't want a single bad id to crash the whole list.
function ulidToCreatedAt(id: string | null | undefined): Date | null {
  if (!id || id.length < 10) return null;
  // Crockford's base32: 0-9 then A-Z minus I, L, O, U.
  // (case-insensitive; we uppercase before lookup.)
  const table = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let ms = 0;
  for (let i = 0; i < 10; i += 1) {
    const ch = id[i].toUpperCase();
    const v = table.indexOf(ch);
    if (v === -1) return null;
    ms = ms * 32 + v;
  }
  // Sanity: ULID timestamps are within (1970, 10889] in ms. Reject
  // anything outside the plausible range so a future change to id
  // format can't render absurd dates.
  if (ms < 0 || ms > 281_474_976_710_655) return null;
  return new Date(ms);
}

function fmtCreatedFromUlid(id: string | null | undefined): JSX.Element | string {
  const d = ulidToCreatedAt(id);
  if (!d) return <span className="font-medium text-text-3">—</span>;
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function rowMeta(row: StrategyListItem): string | undefined {
  const agents = agentCount(row);
  const filters = row.filter_count ?? 0;
  const parts: string[] = [];
  if (agents > 0) parts.push(`${agents} ${agents === 1 ? "agent" : "agents"}`);
  if (filters > 0) parts.push(`${filters} ${filters === 1 ? "filter" : "filters"}`);
  return parts.length > 0 ? parts.join(" · ") : undefined;
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
    return <span className="text-[12px] font-medium text-text-3">—</span>;
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
