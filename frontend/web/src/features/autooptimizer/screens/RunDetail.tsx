import { Link } from "react-router-dom";
import { Pill } from "@/components/primitives/Pill";
import { Topbar } from "@/components/shell/Topbar";
import { ActivityFeed } from "../ui/ActivityFeed";
import { useOptimizerStatus } from "../api";

interface RunDetailProps {
  sessionId: string;
}

function statePill(state: string) {
  const lower = state.toLowerCase();
  if (lower === "running")
    return (
      <Pill tone="gold" animated>
        Running
      </Pill>
    );
  if (lower === "paused") return <Pill tone="warn">Paused</Pill>;
  if (lower === "finished") return <Pill tone="default">Finished</Pill>;
  if (lower === "failed") return <Pill tone="danger">Failed</Pill>;
  if (lower === "cancelling") return <Pill tone="warn">Cancelling</Pill>;
  return <Pill tone="default">{state}</Pill>;
}

function modeLabel(mode: string): string {
  if (mode === "explore") return "Explore";
  if (mode === "exploit") return "Exploit";
  return mode;
}

/**
 * Run detail screen — P1 scope: header + ActivityFeed.
 * P4 will add charts and experiments table below the feed.
 *
 * Layout: single full-width column (no right-side box; chat rail is always
 * present on this route per the three-pane shell rule).
 */
export function RunDetail({ sessionId }: RunDetailProps) {
  const status = useOptimizerStatus();
  const session = status?.active_session?.session_id === sessionId
    ? status.active_session
    : null;

  const state = session?.state ?? "finished";
  const strategyId = session?.strategy_id ?? "";
  const mode = session?.mode ?? "";

  return (
    <>
      <Topbar
        title="Optimizer"
        sub={`Run ${sessionId.slice(0, 8)}`}
      />
      <div className="space-y-5">
        {/* Header strip */}
        <div className="flex flex-wrap items-center gap-3">
          <Link
            to="/optimizer"
            className="text-[13px] text-text-3 hover:text-text transition-colors"
            aria-label="Back to Optimizer"
          >
            ← Optimizer
          </Link>
          {statePill(state)}
          {strategyId && (
            <span className="rounded border border-border px-2 py-0.5 font-mono text-[11px] text-text-2">
              {strategyId}
            </span>
          )}
          {mode && (
            <span className="rounded border border-border px-2 py-0.5 text-[11px] text-text-3">
              {modeLabel(mode)}
            </span>
          )}
          <span className="font-mono text-[11px] text-text-3 ml-auto">
            {sessionId.slice(0, 8)}
          </span>
        </div>

        {/* Activity feed */}
        <ActivityFeed sessionId={sessionId} />

        {/* P4 placeholders */}
        <div data-placeholder="p4-charts" />
        <div data-placeholder="p4-experiments-table" />
      </div>
    </>
  );
}
