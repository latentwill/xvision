// /agents — library + escape-valve list view.
//
// Most agents reach this page via inline authoring in the Inspector
// (the canonical path under View C). This page is the cross-strategy
// view: every agent in the workspace, regardless of how it was created.
// Standalone-create lives at /agents/new (Task 5).

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { useEffect, useMemo, useState } from "react";

import { Topbar } from "@/components/shell/Topbar";
import { Icon } from "@/components/primitives/Icon";
import { Pill } from "@/components/primitives/Pill";
import {
  agentKeys,
  listAgentsPaged,
  updateAgent,
  type Agent,
  type AgentStatus,
} from "@/api/agents";
import { listTools, toolKeys } from "@/api/tools";
import { ApiError } from "@/api/client";
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

// Toolbar filter on agent shape — single-slot agents vs. multi-slot
// (the role label per AgentRef is free text inside Strategy, so we
// can't filter by role from the agents list alone). "Archived" sits
// alongside as a second filter so the URL stays declarative.
const SHAPE_FILTER: FilterDef = {
  id: "shape",
  label: "Shape",
  options: [
    { value: "all", label: "All shapes" },
    { value: "single", label: "Single-slot" },
    { value: "multi", label: "Multi-slot" },
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
  // Backend `list_paged` returns rows DESC by `updated_at`; the recency
  // default mirrors that ordering. Other options sort client-side over
  // the currently-paged slice.
  { value: "updated", label: "Recently updated" },
  { value: "name", label: "Name A → Z" },
  { value: "name-desc", label: "Name Z → A" },
];

export function AgentsRoute() {
  const [archivedToken, setArchivedToken] = useState<string>("exclude");

  // QA-round-7 backend-pagination follow-up (#386 gap): page-size +
  // page-nav drive `limit`/`offset` in the query key so page changes
  // refetch instead of slicing one big response.
  const [totalFromServer, setTotalFromServer] = useState(0);
  const pager = useServerPagination(totalFromServer);

  // Server query — pure server-side params. The `q`/sort/shape filters
  // run client-side over the current page (see `useListState` below).
  const params = useMemo(
    () => ({
      include_archived: archivedToken === "include",
      limit: pager.limit,
      offset: pager.offset,
    }),
    [archivedToken, pager.limit, pager.offset],
  );
  const q = useQuery({
    queryKey: agentKeys.list(params),
    queryFn: () => listAgentsPaged(params),
    placeholderData: (prev) => prev,
  });
  useEffect(() => {
    if (q.data?.total !== undefined && q.data.total !== totalFromServer) {
      setTotalFromServer(q.data.total);
    }
  }, [q.data?.total, totalFromServer]);

  const items = q.data?.items ?? [];
  const total = q.data?.total ?? 0;

  const list = useListState<Agent>({
    rows: items,
    filters: [SHAPE_FILTER, ARCHIVED_FILTER],
    sortOptions: SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const shape = values.shape ?? "all";
      if (shape === "single" && row.slots.length !== 1) return false;
      if (shape === "multi" && row.slots.length <= 1) return false;
      const qq = query.trim().toLowerCase();
      if (qq.length === 0) return true;
      if (row.name.toLowerCase().includes(qq)) return true;
      if (row.description.toLowerCase().includes(qq)) return true;
      if (row.tags.some((t) => t.toLowerCase().includes(qq))) return true;
      return false;
    },
    sortFn: (rows, key) => {
      switch (key) {
        case "name":
          return rows.sort((a, b) => a.name.localeCompare(b.name));
        case "name-desc":
          return rows.sort((a, b) => b.name.localeCompare(a.name));
        case "updated":
        default:
          // Backend already DESC by updated_at; preserve order.
          return rows;
      }
    },
  });
  useListUrlState("agents", list);

  // Bridge URL-driven archived filter back to local state so the
  // backend query refetches.
  const archivedValue =
    list.filters.find((f) => f.def.id === "archived")?.value ?? "exclude";
  useEffect(() => {
    if (archivedValue !== archivedToken) setArchivedToken(archivedValue);
  }, [archivedValue, archivedToken]);

  const navigate = useNavigate();
  function go(id: string) {
    navigate(`/agents/${encodeURIComponent(id)}`);
  }

  // QA asked for a `Created` column on agents alongside the
  // existing `Updated`. `agents.created_at` already exists
  // (migration 005) and is on the Agent type; just plumb it
  // through. Placed BEFORE `Updated` so the natural reading order
  // is "created → last touched".
  const desktopColumns = [
    { key: "name",    label: "Name",    essential: true, estWidth: 200 },
    { key: "status",  label: "Status",  priority: 4,     estWidth: 80  },
    { key: "tools",   label: "Tools",   priority: 2,     estWidth: 120 },
    { key: "slots",   label: "Slots",   priority: 3,     estWidth: 80  },
    { key: "skills",  label: "Skills",  priority: 1,     estWidth: 120 },
    { key: "created", label: "Created", priority: 0,     estWidth: 100 },
    { key: "updated", label: "Updated", priority: 0,     estWidth: 100 },
  ];
  const columnState = useListColumns("agents", desktopColumns);
  const queryClient = useQueryClient();
  const toolsQ = useQuery({ queryKey: toolKeys.all, queryFn: listTools });
  const updateTools = useMutation({
    mutationFn: ({ row, tools }: { row: Agent; tools: string[] }) =>
      updateAgent(row.agent_id, {
        slots: row.slots.map((slot) => ({
          ...slot,
          allowed_tools: tools,
        })),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: agentKeys.all });
    },
  });

  return (
    <>
      <Topbar title="Agents" sub={subtitleFor(q, total, list.rows.length, archivedToken)} />

      <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
        <Link
          to="/agents/memory"
          className="inline-flex items-center gap-1.5 rounded border border-border px-3 py-1.5 text-[13px] font-medium text-text-2 transition-colors hover:border-border-strong hover:text-text"
        >
          Memory
        </Link>
        <Link
          to="/agents/skills"
          className="inline-flex items-center gap-1.5 rounded border border-border px-3 py-1.5 text-[13px] font-medium text-text-2 transition-colors hover:border-border-strong hover:text-text"
        >
          Skills
        </Link>
        <Link
          to="/agents/new"
          className="inline-flex w-full items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft sm:w-auto motion-safe:active:scale-[0.96]"
        >
          <Icon name="plus" size={13} /> New agent
        </Link>
      </div>

      <ResponsiveListCard<Agent>
        listId="agents"
        title="Agents"
        count={total}
        toolbar={{
          search: { ...list.search, placeholder: "Search agents by name…" },
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
            ? "No agents yet. Start with a single-slot agent — name it, give it a system prompt, pick a model."
            : "No agents match these filters."
        }
        emptyAction={
          total === 0 ? (
            <Link
              to="/agents/new"
              className="inline-flex items-center gap-1.5 rounded border border-gold px-3 py-1.5 text-[12px] font-medium text-gold hover:bg-gold/10"
            >
              <Icon name="plus" size={11} /> New agent
            </Link>
          ) : null
        }
        renderRow={(row, _i, visibleKeys) => (
          <DesktopRow
            key={row.agent_id}
            row={row}
            onGo={go}
            visibleKeys={visibleKeys}
            tools={toolsQ.data?.items ?? []}
            toolsLoading={toolsQ.isPending}
            updatingTools={updateTools.isPending}
            onToolsChange={(nextTools) =>
              updateTools.mutate({ row, tools: nextTools })
            }
          />
        )}
        renderMobileRow={(row) => (
          <MListRow
            key={row.agent_id}
            onClick={() => go(row.agent_id)}
            title={row.name}
            badge={defaultStatus(row)}
            badgeColor={badgeColorFor(defaultStatus(row))}
            subtitle={row.description || undefined}
            meta={`${row.slots.length} ${row.slots.length === 1 ? "slot" : "slots"} · ${formatRelative(row.updated_at)}`}
          />
        )}
      />

      <ServerPagerStrip
        total={total}
        page={pager.page}
        pageSize={pager.pageSize}
        onPageChange={pager.setPage}
        onPageSizeChange={pager.setPageSize}
        itemLabel="agents"
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
  if (q.isError) return "Couldn't load agents";
  const base =
    total === 0
      ? "0 agents"
      : visibleRows === total
        ? `${total} ${total === 1 ? "agent" : "agents"}`
        : `${visibleRows} of ${total} agents`;
  return archivedToken === "exclude" ? `${base} · archived hidden` : base;
}

function DesktopRow({
  row,
  onGo,
  visibleKeys,
  tools,
  toolsLoading,
  updatingTools,
  onToolsChange,
}: {
  row: Agent;
  onGo: (id: string) => void;
  visibleKeys: Set<string>;
  tools: { name: string; description: string }[];
  toolsLoading: boolean;
  updatingTools: boolean;
  onToolsChange: (tools: string[]) => void;
}) {
  const status = defaultStatus(row);
  const skillsCount = row.slots.reduce(
    (acc, s) => acc + s.skill_ids.length,
    0,
  );
  return (
    <tr
      onClick={() => onGo(row.agent_id)}
      className="xvn-row-in cursor-pointer border-b border-border-soft transition-colors last:border-b-0 hover:bg-surface-hover focus-within:bg-surface-hover"
    >
      <td className="px-5 py-3">
        <Link
          to={`/agents/${encodeURIComponent(row.agent_id)}`}
          onClick={(e) => e.stopPropagation()}
          className="block font-medium text-text hover:underline"
        >
          {row.name}
        </Link>
        {row.description ? (
          <div className="text-text-3 text-[12px] mt-0.5 line-clamp-1">
            {row.description}
          </div>
        ) : null}
      </td>
      {visibleKeys.has("status") ? (
        <td className="px-5 py-3">
          <StatusPill status={status} />
        </td>
      ) : null}
      {visibleKeys.has("tools") ? (
        <td className="px-5 py-3 overflow-hidden">
          <AgentToolsSelect
            row={row}
            tools={tools}
            loading={toolsLoading}
            disabled={updatingTools}
            onChange={onToolsChange}
          />
        </td>
      ) : null}
      {visibleKeys.has("slots") ? (
        <td className="px-5 py-3 text-text-2 font-mono text-[12px]">
          {row.slots.length === 1
            ? `1 (${row.slots[0]?.name ?? "main"})`
            : `${row.slots.length}`}
        </td>
      ) : null}
      {visibleKeys.has("skills") ? (
        <td className="px-5 py-3 text-text-2 font-mono text-[12px]">
          {skillsCount}
        </td>
      ) : null}
      {visibleKeys.has("created") ? (
        <td
          className="px-5 py-3 text-text-3 text-[12px]"
          title={row.created_at}
        >
          {formatRelative(row.created_at)}
        </td>
      ) : null}
      {visibleKeys.has("updated") ? (
        <td className="px-5 py-3 text-text-3 text-[12px]">
          {formatRelative(row.updated_at)}
        </td>
      ) : null}
    </tr>
  );
}

function AgentToolsSelect({
  row,
  tools,
  loading,
  disabled,
  onChange,
}: {
  row: Agent;
  tools: { name: string; description: string }[];
  loading: boolean;
  disabled: boolean;
  onChange: (tools: string[]) => void;
}) {
  const selectedTools = agentTools(row);
  const value =
    selectedTools.length === 0
      ? "__none__"
      : selectedTools.length === 1
        ? selectedTools[0]
        : "__custom__";

  return (
    <select
      aria-label={`Tools for ${row.name}`}
      value={value}
      disabled={loading || disabled}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        e.stopPropagation();
        const next = e.target.value;
        if (next === "__custom__") return;
        onChange(next === "__none__" ? [] : [next]);
      }}
      className="h-7 w-[150px] max-w-[150px] rounded-sm border border-border bg-surface-elev px-2 font-mono text-[11px] text-text-2 outline-none transition-colors hover:border-text-3 focus:border-gold/50 disabled:cursor-not-allowed disabled:opacity-60 overflow-hidden text-ellipsis"
    >
      <option value="__none__">No tools</option>
      {selectedTools.length > 1 ? (
        <option value="__custom__">{selectedTools.length} tools</option>
      ) : null}
      {tools.map((tool) => (
        <option key={tool.name} value={tool.name} title={tool.description}>
          {tool.name}
        </option>
      ))}
    </select>
  );
}

function StatusPill({ status }: { status: AgentStatus }) {
  switch (status) {
    case "Draft":
      return <Pill tone="default">Draft</Pill>;
    case "Validated":
      return <Pill tone="gold">Validated</Pill>;
    case "In use":
      return <Pill tone="info">In use</Pill>;
    case "Archived":
      return <Pill tone="default">Archived</Pill>;
  }
}

function badgeColorFor(
  status: AgentStatus,
): "gold" | "warn" | "danger" | "info" | "muted" {
  switch (status) {
    case "Validated":
      return "gold";
    case "In use":
      return "info";
    case "Archived":
    case "Draft":
    default:
      return "muted";
  }
}

function agentTools(agent: Agent): string[] {
  const seen = new Set<string>();
  for (const slot of agent.slots) {
    for (const tool of slot.allowed_tools ?? []) seen.add(tool);
  }
  return [...seen].sort();
}

// Default status when no separately-computed status is provided. In v1
// the list endpoint doesn't run validators per row (would be N+1), so
// we treat every non-archived agent as Draft until the detail view
// validates.
function defaultStatus(agent: Agent): AgentStatus {
  if (agent.archived) return "Archived";
  return "Draft";
}

function formatRelative(iso: string): string {
  try {
    const then = new Date(iso).getTime();
    const now = Date.now();
    const seconds = Math.max(0, Math.round((now - then) / 1000));
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.round(seconds / 3600)}h ago`;
    const days = Math.round(seconds / 86400);
    if (days < 30) return `${days}d ago`;
    return new Date(iso).toLocaleDateString();
  } catch {
    return iso;
  }
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
