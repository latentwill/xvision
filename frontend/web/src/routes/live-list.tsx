// frontend/web/src/routes/live-list.tsx
//
// /live list page — shows all active live-strategy deployments.
// Polls every 10 s. Single-column layout (QA30: no right sidebar).
// No modals, no popups (per UI rule).

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import type { AgentRunSummary } from "@/api/types-agent-runs";

// Status → display tone
const STATUS_COLORS: Record<
  string,
  { bg: string; text: string }
> = {
  running: { bg: "bg-info/15", text: "text-info" },
  queued: { bg: "bg-surface-elev", text: "text-text-3" },
  completed: { bg: "bg-gold/15", text: "text-gold" },
  failed: { bg: "bg-danger/15", text: "text-danger" },
  cancelled: { bg: "bg-warn/15", text: "text-warn" },
  interrupted: { bg: "bg-warn/15", text: "text-warn" },
  agent_failure: { bg: "bg-danger/15", text: "text-danger" },
};

function statusStyle(status: string) {
  return STATUS_COLORS[status] ?? { bg: "bg-surface-elev", text: "text-text-3" };
}

function relativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(ms)) return iso;
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function RunRow({ run }: { run: AgentRunSummary }) {
  const shortId = run.run_id.slice(0, 8);
  const { bg, text } = statusStyle(run.status);
  return (
    <Link
      to={`/live/${run.run_id}`}
      className="flex items-center gap-4 rounded-lg border border-border bg-surface px-5 py-4 text-[13px] transition-colors hover:bg-surface-hover focus:bg-surface-hover focus:outline-none"
      aria-label={shortId}
    >
      <span className="w-24 shrink-0 font-mono text-text-2">{shortId}</span>
      <span
        className={`shrink-0 rounded px-2 py-0.5 text-[11px] font-medium ${bg} ${text}`}
      >
        {run.status}
      </span>
      <span className="min-w-0 flex-1 truncate text-text-3">
        {run.objective || "—"}
      </span>
      <span className="shrink-0 text-[12px] text-text-3">
        {relativeTime(run.started_at)}
      </span>
      <span className="shrink-0 text-text-3" aria-hidden="true">
        →
      </span>
    </Link>
  );
}

export function LiveListRoute() {
  const q = useQuery({
    queryKey: agentRunKeys.list(),
    queryFn: listAgentRuns,
    refetchInterval: 10_000,
  });

  const runs = q.data ?? [];

  return (
    <>
      <Topbar title="Live strategies" sub="Real money · active deployments" />

      <div className="space-y-3">
        {q.isPending ? (
          <p className="py-10 text-center text-[13px] text-text-3">Loading…</p>
        ) : q.isError ? (
          <p className="py-10 text-center text-[13px] text-danger">
            Failed to load live deployments.
          </p>
        ) : runs.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-16 text-center">
            <p className="text-[15px] font-medium text-text-2">
              No active live deployments
            </p>
            <p className="text-[13px] text-text-3">
              Configure a broker to start live trading.{" "}
              <Link
                to="/settings/brokers"
                className="text-text-2 underline underline-offset-2 hover:text-text"
              >
                Settings → Brokers
              </Link>
            </p>
          </div>
        ) : (
          runs.map((run) => <RunRow key={run.run_id} run={run} />)
        )}
      </div>
    </>
  );
}
