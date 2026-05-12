// Agent list table. Renders a row per agent with name, description,
// status pill, slot count, skills count, updated_at.
//
// Status pill in v1: Draft / Validated / In use / Archived.
// "In use" depends on deployed_in (empty in v1), so most agents resolve
// to Draft until validation passes — keep the surface ready for when the
// strategies refactor lights up deployment links.

import { Link } from "react-router-dom";
import type { Agent, AgentStatus } from "@/api/agents";
import { Pill } from "@/components/primitives/Pill";

type StatusLookup = Record<string, AgentStatus>;

export function AgentList({
  items,
  statuses,
}: {
  items: Agent[];
  statuses?: StatusLookup;
}) {
  return (
    <table className="w-full text-[13.5px]">
      <thead>
        <tr className="text-text-3 text-left text-[11px] uppercase tracking-wide">
          <th className="px-5 py-3 font-medium">Name</th>
          <th className="px-5 py-3 font-medium">Status</th>
          <th className="px-5 py-3 font-medium">Slots</th>
          <th className="px-5 py-3 font-medium">Skills</th>
          <th className="px-5 py-3 font-medium">Updated</th>
        </tr>
      </thead>
      <tbody>
        {items.map((a) => (
          <AgentRow
            key={a.agent_id}
            agent={a}
            status={statuses?.[a.agent_id] ?? defaultStatus(a)}
          />
        ))}
      </tbody>
    </table>
  );
}

function AgentRow({ agent, status }: { agent: Agent; status: AgentStatus }) {
  const skillsCount = agent.slots.reduce(
    (acc, s) => acc + s.skill_ids.length,
    0,
  );

  return (
    <tr className="border-t border-border-soft hover:bg-surface-hover transition-colors">
      <td className="px-5 py-3">
        <Link
          to={`/agents/${encodeURIComponent(agent.agent_id)}`}
          className="text-text hover:text-gold transition-colors"
        >
          <div className="font-medium">{agent.name}</div>
          {agent.description ? (
            <div className="text-text-3 text-[12px] mt-0.5 line-clamp-1">
              {agent.description}
            </div>
          ) : null}
        </Link>
      </td>
      <td className="px-5 py-3">
        <StatusPill status={status} />
      </td>
      <td className="px-5 py-3 text-text-2 font-mono text-[12px]">
        {agent.slots.length === 1
          ? `1 (${agent.slots[0]?.name ?? "main"})`
          : `${agent.slots.length}`}
      </td>
      <td className="px-5 py-3 text-text-2 font-mono text-[12px]">
        {skillsCount}
      </td>
      <td className="px-5 py-3 text-text-3 text-[12px]">
        {formatRelative(agent.updated_at)}
      </td>
    </tr>
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

// Default status when no separately-computed status is provided. In v1 the
// list endpoint doesn't run validators per row (would be N+1), so we treat
// every non-archived agent as Draft until the detail view validates.
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
