// /agents — library + escape-valve list view.
//
// Most agents reach this page via inline authoring in the Inspector
// (the canonical path under View C). This page is the cross-strategy
// view: every agent in the workspace, regardless of how it was created.
// Standalone-create lives at /agents/new (Task 5).

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { useState } from "react";

import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Icon } from "@/components/primitives/Icon";
import { AgentList } from "@/components/agent/AgentList";
import { agentKeys, listAgents, type Agent } from "@/api/agents";
import { ApiError } from "@/api/client";

export function AgentsRoute() {
  const [includeArchived, setIncludeArchived] = useState(false);
  const [search, setSearch] = useState("");

  const q = useQuery({
    queryKey: agentKeys.list({
      include_archived: includeArchived,
      q: search || undefined,
    }),
    queryFn: () =>
      listAgents({
        include_archived: includeArchived,
        q: search || undefined,
      }),
  });

  return (
    <>
      <Topbar title="Agents" sub={subtitleFor(q)} />

      <FilterBar
        search={search}
        onSearch={setSearch}
        includeArchived={includeArchived}
        onToggleArchived={setIncludeArchived}
      />

      <Card>
        {q.isPending ? (
          <LoadingSkeleton />
        ) : q.isError ? (
          <ErrorState err={q.error} onRetry={() => q.refetch()} />
        ) : (q.data ?? []).length === 0 ? (
          <EmptyState />
        ) : (
          <AgentList items={q.data ?? []} />
        )}
      </Card>
    </>
  );
}

function subtitleFor(q: { isPending: boolean; isError: boolean; data?: Agent[] }) {
  if (q.isPending) return "Loading…";
  if (q.isError) return "Couldn't load agents";
  const n = q.data?.length ?? 0;
  return `${n} ${n === 1 ? "agent" : "agents"}`;
}

function FilterBar({
  search,
  onSearch,
  includeArchived,
  onToggleArchived,
}: {
  search: string;
  onSearch: (s: string) => void;
  includeArchived: boolean;
  onToggleArchived: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-3 mb-4">
      <div className="flex-1 relative">
        <span className="absolute left-3 top-1/2 -translate-y-1/2 text-text-3">
          <Icon name="search" size={14} />
        </span>
        <input
          type="text"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="Search agents by name…"
          className="w-full pl-9 pr-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold/40"
        />
      </div>

      <label className="flex items-center gap-2 text-[13px] text-text-2 cursor-pointer">
        <input
          type="checkbox"
          checked={includeArchived}
          onChange={(e) => onToggleArchived(e.target.checked)}
          className="accent-gold"
        />
        Show archived
      </label>

      <Link
        to="/agents/skills"
        className="inline-flex items-center gap-1.5 px-3 py-2 rounded text-[13px] font-medium border border-border text-text-2 hover:text-text hover:border-border-strong transition-colors"
      >
        Skills
      </Link>

      <Link
        to="/agents/new"
        className="inline-flex items-center gap-1.5 px-3 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
      >
        <Icon name="plus" size={14} />
        New agent
      </Link>
    </div>
  );
}

function LoadingSkeleton() {
  return (
    <div className="p-8 text-center text-text-3 text-[13px]">Loading…</div>
  );
}

function ErrorState({
  err,
  onRetry,
}: {
  err: unknown;
  onRetry: () => void;
}) {
  const msg =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);
  return (
    <div className="p-8 text-center">
      <div className="text-danger text-[13px] mb-3">{msg}</div>
      <button
        type="button"
        onClick={onRetry}
        className="px-3 py-1.5 rounded border border-border text-[13px] text-text-2 hover:text-text hover:border-border-strong transition-colors"
      >
        Retry
      </button>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="p-12 text-center">
      <div className="mb-3 inline-flex items-center justify-center w-12 h-12 rounded-full bg-gold/10 text-gold">
        <Icon name="user" size={22} />
      </div>
      <h3 className="m-0 mb-1 text-[16px] font-medium text-text">
        No agents yet
      </h3>
      <p className="m-0 mb-5 text-text-3 text-[13px] max-w-md mx-auto leading-snug">
        Agents are reusable templates that compose into strategies. Start
        with a single-slot agent — name it, give it a system prompt, pick a
        model.
      </p>
      <Link
        to="/agents/new"
        className="inline-flex items-center gap-1.5 px-4 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
      >
        <Icon name="plus" size={14} />
        New agent
      </Link>
    </div>
  );
}
