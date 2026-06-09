// A single strategy pill in the Live cockpit strip (spec §2.4).
//
// Shows: strategy name · status pill (ACTIVE/PAUSED/STOPPED) · one
// configurable metric (color-coded) · SSE connection dot. Clicking the
// pill selects it. Transport controls (⏸/⏹/▶) reveal on hover/focus;
// in B-I they are disabled placeholders (see TransportControls).

import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { LiveStatus } from "@/components/chart/use-run-stream";
import { ConnectionDot } from "./ConnectionDot";
import { TransportControls } from "./TransportControls";
import { computeStripMetric, type StripMetricId } from "./strip-metrics";
import { deriveStripStatus, type StripStatus } from "./strip-status";

const STATUS_STYLE: Record<StripStatus, string> = {
  ACTIVE: "bg-info/15 text-info",
  PAUSED: "bg-warn/15 text-warn",
  STOPPED: "bg-surface-elev text-text-3",
};

const METRIC_TONE: Record<"pos" | "neg" | "neutral", string> = {
  pos: "text-info",
  neg: "text-danger",
  neutral: "text-text-2",
};

export interface StrategyPillProps {
  run: AgentRunSummary;
  selected: boolean;
  metric: StripMetricId;
  /** Real SSE status for the SELECTED pill; lightweight derived value otherwise. */
  connStatus: LiveStatus;
  onSelect: () => void;
  walletDisabled: boolean;
  // B-III transport seam — omitted in B-I so buttons render disabled.
  onPause?: () => void;
  onResume?: () => void;
  onStop?: () => void;
}

export function StrategyPill({
  run,
  selected,
  metric,
  connStatus,
  onSelect,
  walletDisabled,
  onPause,
  onResume,
  onStop,
}: StrategyPillProps) {
  const status = deriveStripStatus(run);
  const name = run.objective || run.strategy_id || run.run_id.slice(0, 8);
  const m = computeStripMetric(metric, run);

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      aria-label={`Strategy ${name}`}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      data-selected={selected || undefined}
      className={[
        "group flex shrink-0 items-center gap-2.5 rounded-lg border px-3 py-2",
        "cursor-pointer text-[13px] transition-colors focus:outline-none",
        selected
          ? "border-gold/45 bg-gold/10"
          : "border-border bg-surface hover:bg-surface-hover focus:bg-surface-hover",
      ].join(" ")}
    >
      <ConnectionDot status={connStatus} />
      <span className="max-w-[160px] truncate font-medium text-text">{name}</span>
      <span
        className={`shrink-0 rounded px-1.5 py-0.5 text-[10.5px] font-semibold tracking-wide ${STATUS_STYLE[status]}`}
      >
        {status}
      </span>
      <span
        className={`shrink-0 font-mono text-[12.5px] tabular-nums ${METRIC_TONE[m.tone]}`}
        title={m.derived ? undefined : "Not available yet"}
      >
        {m.text}
      </span>
      {/* Transport controls — reveal on hover/focus, disabled in B-I. */}
      <span className="ml-0.5 hidden group-hover:flex group-focus-within:flex">
        <TransportControls
          status={status}
          walletDisabled={walletDisabled}
          onPause={onPause}
          onResume={onResume}
          onStop={onStop}
        />
      </span>
    </div>
  );
}
