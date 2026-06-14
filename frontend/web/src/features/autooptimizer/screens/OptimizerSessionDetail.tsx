/**
 * OptimizerSessionDetail — /optimizer/run/:sessionId
 *
 * Session-scoped view of a single optimizer session. Single-column layout per
 * the "no right-side boxes" rule; no popups/modals.
 *
 * Composes existing hooks and panels:
 *  - useSessionDetail → session header strip
 *  - useOptimizerStats({ session_id }) → EdgeVsRandomChart
 *  - useCycleRuns({ session_id }) → session cycle list via RecentCyclesTableBody
 *  - EdgeVsRandomChart (existing)
 *  - RecentCyclesTableBody (existing, scoped to session via future prop — rendered inline)
 */

import { Link, Navigate, useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Breadcrumb } from "../ui/Breadcrumb";
import { EdgeVsRandomChart } from "../ui/EdgeVsRandomChart";
import {
  useSessionDetail,
  useOptimizerStats,
  useCycleRuns,
  type CycleRunSummary,
} from "../api";

// ─── Session header strip ──────────────────────────────────────────────────────

function stateBadgeClass(state: string): string {
  switch (state) {
    case "running":
      return "text-accent border-accent/40 bg-accent/[0.06]";
    case "paused":
      return "text-warn border-warn/40 bg-warn/[0.06]";
    case "failed":
      return "text-danger border-danger/40 bg-danger/[0.06]";
    case "finished":
    case "completed":
      return "text-text-3 border-border bg-surface-elev";
    default:
      return "text-text-3 border-border bg-surface-elev";
  }
}

function stateLabel(state: string): string {
  switch (state) {
    case "running":
      return "Running";
    case "paused":
      return "Paused";
    case "cancelling":
      return "Cancelling";
    case "finished":
    case "completed":
      return "Finished";
    case "failed":
      return "Failed";
    default:
      return state;
  }
}

type SessionHeaderProps = {
  sessionId: string;
};

function SessionHeader({ sessionId }: SessionHeaderProps) {
  const { data: session, isLoading, isError } = useSessionDetail(sessionId);

  if (isLoading) {
    return (
      <div
        aria-label="Session header"
        className="rounded-md border border-border bg-surface-card p-5"
      >
        <p className="text-[12px] text-text-3">Loading session…</p>
      </div>
    );
  }

  if (isError || !session) {
    return (
      <div
        aria-label="Session header"
        className="rounded-md border border-border bg-surface-card p-5"
      >
        <p className="text-[12px] text-danger">Couldn't load session details.</p>
      </div>
    );
  }

  const totalAttempts =
    session.kept_count + session.suspect_count + session.dropped_count;

  return (
    <div
      aria-label="Session header"
      className="rounded-md border border-border bg-surface-card p-5 space-y-3"
    >
      {/* Title row */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="m-0 font-mono text-[11px] text-text-4 uppercase tracking-widest">
            Session
          </p>
          <h2 className="m-0 mt-0.5 font-mono text-[14px] text-text select-all break-all">
            {session.session_id}
          </h2>
        </div>
        <span
          className={[
            "shrink-0 rounded border px-2 py-0.5 font-mono text-[11px] uppercase tracking-wide",
            stateBadgeClass(session.state),
          ].join(" ")}
        >
          {stateLabel(session.state)}
        </span>
      </div>

      {/* Stats chip row */}
      <div className="flex flex-wrap gap-x-6 gap-y-1 font-mono text-[12px] text-text-3">
        <span>
          <span className="text-text-2">{session.cycles_completed}</span> cycle
          {session.cycles_completed === 1 ? "" : "s"}
          {session.cycles_planned != null ? ` / ${session.cycles_planned}` : ""}
        </span>
        <span>
          <span className="text-accent">{session.kept_count}</span> kept
        </span>
        <span>
          <span className="text-warn">{session.suspect_count}</span> suspect
        </span>
        <span>
          <span className="text-danger">{session.dropped_count}</span> dropped
        </span>
        <span>
          <span className="text-text-2">{totalAttempts}</span> total experiments
        </span>
        {session.errored_count > 0 && (
          <span>
            <span className="text-danger">{session.errored_count}</span> errored
          </span>
        )}
        <span className="text-text-4">
          Mode:{" "}
          <span className="text-text-3">{session.mode}</span>
        </span>
        <span className="text-text-4">
          Strategy:{" "}
          <span className="text-text-3 select-all">{session.strategy_id}</span>
        </span>
      </div>
    </div>
  );
}

// ─── Session cycle list ────────────────────────────────────────────────────────

function SessionCycleList({ sessionId }: { sessionId: string }) {
  const { data: cycles, isLoading, isError } = useCycleRuns({ session_id: sessionId, limit: 50 });

  if (isLoading) {
    return <p className="text-[12px] text-text-3">Loading cycles…</p>;
  }
  if (isError) {
    return <p className="text-[12px] text-danger">Couldn't load cycles for this session.</p>;
  }
  if (!cycles || cycles.length === 0) {
    return (
      <p className="text-[12px] text-text-3">
        No cycles recorded for this session yet.
      </p>
    );
  }

  return (
    <table className="w-full text-[12px] font-mono border-collapse">
      <thead>
        <tr className="text-left text-text-4 uppercase tracking-widest text-[10px]">
          <th className="pb-2 pr-4 font-normal">Cycle</th>
          <th className="pb-2 pr-4 font-normal text-right">Experiments</th>
          <th className="pb-2 pr-4 font-normal text-right">Kept</th>
          <th className="pb-2 pr-4 font-normal text-right">Suspect</th>
          <th className="pb-2 font-normal text-right">$</th>
        </tr>
      </thead>
      <tbody>
        {cycles.map((c: CycleRunSummary) => (
          <tr
            key={c.cycle_id}
            className="border-t border-border hover:bg-surface-elev/40 transition-colors"
          >
            <td className="py-1.5 pr-4">
              <Link
                to={`/optimizer/cycle/${encodeURIComponent(c.cycle_id)}`}
                className="text-text-2 hover:text-text hover:underline"
              >
                {c.cycle_id}
              </Link>
            </td>
            <td className="py-1.5 pr-4 text-right text-text-3">{c.node_count}</td>
            <td className="py-1.5 pr-4 text-right text-accent">{c.active_count}</td>
            <td className="py-1.5 pr-4 text-right text-warn">
              {c.suspect_count ?? 0}
            </td>
            <td className="py-1.5 text-right text-text-3">
              {c.cost_usd != null ? `$${c.cost_usd.toFixed(2)}` : "—"}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ─── Page root ────────────────────────────────────────────────────────────────

export function OptimizerSessionDetail() {
  const { sessionId } = useParams<{ sessionId: string }>();

  // Guard: missing session id → back to /optimizer
  if (!sessionId) {
    return <Navigate to="/optimizer" replace />;
  }

  // Session-scoped stats for the trajectory chart
  const { data: statsRows = [] } = useOptimizerStats({ session_id: sessionId });

  return (
    <>
      <Topbar
        title="Optimizer"
        sub="Session"
        back={{ to: "/optimizer", label: "Back to Optimizer" }}
      />
      <div className="space-y-5">
        <Breadcrumb
          items={[
            { label: "OPTIMIZER", to: "/optimizer" },
            { label: "session" },
            { label: sessionId },
          ]}
        />

        {/* Session summary header */}
        <SessionHeader sessionId={sessionId} />

        {/* Session-scoped trajectory chart */}
        <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
          <div className="text-[11px] uppercase tracking-widest text-text-4">
            Edge vs random — this session
          </div>
          <EdgeVsRandomChart rows={statsRows} />
        </section>

        {/* Session cycle list */}
        <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
          <h2 className="m-0 text-[15px] font-semibold tracking-tight">
            Cycles this session
          </h2>
          <SessionCycleList sessionId={sessionId} />
        </section>

        {/* Cross-link to eval runs (best-effort; no backend FK) */}
        <div className="text-[12px] text-text-3">
          <Link
            to="/eval-runs"
            className="text-text-2 hover:text-text hover:underline"
          >
            View eval runs →
          </Link>
        </div>
      </div>
    </>
  );
}
