// A single strategy pill in the Live Trading strip (spec §2.4).
//
// Shows: strategy name · status pill (ACTIVE/PAUSED/STOPPED) · one
// configurable metric (color-coded) · SSE connection dot. Clicking the
// pill selects it. Transport controls (⏸/⏹/▶) reveal on hover/focus;
// in B-I they are disabled placeholders (see TransportControls).

import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { LiveStatus } from "@/components/chart/use-run-stream";
import { displayStrategyName, type NamedStrategy } from "@/lib/run-display";
import { ConnectionDot } from "./ConnectionDot";
import { TransportControls } from "./TransportControls";
import { computeStripMetric, type StripMetricId } from "./strip-metrics";
import { deriveStripStatus, type StripStatus } from "./strip-status";
import type { RunTransport } from "./useTransport";

const STATUS_STYLE: Record<StripStatus, string> = {
  ACTIVE: "bg-info/15 text-info",
  PAUSED: "bg-warn/15 text-warn",
  STOPPED: "bg-surface-elev text-text-3",
  // Orphaned recorder row (parent eval run terminal) — render muted like a
  // dead run, never with the live/info treatment.
  STALE: "bg-surface-elev text-text-3",
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
  strategies?: NamedStrategy[];
  // B-III transport seam — handlers + inline-expander UI state. Omitted ⇒
  // buttons render as disabled placeholders (B-I behavior).
  transport?: RunTransport;
}

export function StrategyPill({
  run,
  selected,
  metric,
  connStatus,
  onSelect,
  walletDisabled,
  strategies,
  transport,
}: StrategyPillProps) {
  const status = deriveStripStatus(run);
  const name = run.agent_id
    ? displayStrategyName(run.agent_id, strategies)
    : run.objective || run.strategy_id || run.run_id.slice(0, 8);
  const m = computeStripMetric(metric, run);

  // Keep the transport area visible (not hover-gated) whenever an inline
  // expander or error is showing, so the confirm/flatten flow doesn't vanish
  // when the pointer leaves the pill.
  const expanderActive =
    !!transport &&
    (transport.pausedExpanderOpen ||
      transport.stopConfirmOpen ||
      transport.flattenPending ||
      !!transport.error);

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
        "group flex shrink-0 flex-col gap-1.5 rounded-lg border px-3 py-2",
        "cursor-pointer text-[13px] transition-colors focus:outline-none",
        selected
          ? "border-gold/45 bg-gold/10"
          : "border-border bg-surface hover:bg-surface-hover focus:bg-surface-hover",
      ].join(" ")}
    >
      <div className="flex items-center gap-2.5">
        <ConnectionDot status={connStatus} />
        <span className="max-w-[160px] truncate font-medium text-text">
          {name}
        </span>
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
      </div>
      {/*
        Transport controls. Buttons reveal on hover/focus; the inline
        confirm/flatten expanders force the area visible (`expanderActive`)
        so the flow doesn't vanish when the pointer leaves the pill. No
        popups — everything renders within the pill's own box.
      */}
      <div
        className={[
          expanderActive ? "flex" : "hidden group-hover:flex group-focus-within:flex",
        ].join(" ")}
      >
        <TransportControls
          status={status}
          walletDisabled={walletDisabled}
          onPause={transport?.onPause}
          onResume={transport?.onResume}
          onStop={transport?.onStop}
          onStopConfirm={transport?.onStopConfirm}
          onStopCancel={transport?.onStopCancel}
          onFlatten={transport?.onFlatten}
          onKeepOpen={transport?.onKeepOpen}
          pausedExpanderOpen={transport?.pausedExpanderOpen}
          flattenPending={transport?.flattenPending}
          stopConfirmOpen={transport?.stopConfirmOpen}
          error={transport?.error}
          busy={transport?.busy}
          confirmWord={name}
        />
      </div>
    </div>
  );
}
