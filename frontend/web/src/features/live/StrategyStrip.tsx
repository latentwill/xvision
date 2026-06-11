// Live Trading strategy strip (spec §2.4).
//
// Fixed horizontal strip, always visible (does not scroll with the body).
// One horizontal band, left → right:
//
//   [ALL n] [LIVE n] [PAUSED n] [STOPPED n] | pills… | metric picker · deploy
//
// Status filter chips gate which runs render as pills. Default filter is
// LIVE — only runs where `isLiveRun()` is true (real money moving now) may
// appear there; backtests, orphans, and terminal runs land under their real
// bucket (see `stripFilterBucket`). Zero live runs ⇒ a quiet empty state
// with the deploy link instead of stale capsules.

import { useState } from "react";
import { Link } from "react-router-dom";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { LiveStatus } from "@/components/chart/use-run-stream";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";
import type { NamedStrategy } from "@/lib/run-display";
import { StrategyPill } from "./StrategyPill";
import {
  filterRunsForStrip,
  isLiveRun,
  STRIP_FILTERS,
  stripFilterCounts,
  type StripFilter,
} from "./strip-status";
import {
  STRIP_METRIC_OPTIONS,
  type StripMetricId,
} from "./strip-metrics";
import type { RunTransport } from "./useTransport";

export interface StrategyStripProps {
  runs: AgentRunSummary[];
  selectedId: string | null;
  onSelect: (runId: string) => void;
  metric: StripMetricId;
  onMetricChange: (metric: StripMetricId) => void;
  /** Real SSE status of the currently-selected run's chart stream. */
  selectedConnStatus: LiveStatus;
  walletDisabled: boolean;
  strategies?: NamedStrategy[];
  // B-III transport seam — factory so each pill gets its run's handlers +
  // inline-expander UI state. Omitted ⇒ pills render disabled transport
  // placeholders (B-I behavior).
  transportFor?: (run: AgentRunSummary) => RunTransport;
}

export function StrategyStrip({
  runs,
  selectedId,
  onSelect,
  metric,
  onMetricChange,
  selectedConnStatus,
  walletDisabled,
  strategies,
  transportFor,
}: StrategyStripProps) {
  const metricOptions = STRIP_METRIC_OPTIONS.map((o) => ({
    value: o.id,
    label: o.label,
  }));

  // Status filter — LIVE by default so dead/backtest capsules never greet
  // the operator. Local state only; the chart selection is independent.
  const [filter, setFilter] = useState<StripFilter>("LIVE");
  const counts = stripFilterCounts(runs);
  const visible = filterRunsForStrip(runs, filter);

  return (
    <div
      data-testid="strategy-strip"
      className="sticky top-0 z-10 -mx-6 mb-4 border-b border-border bg-bg/95 px-6 py-3 backdrop-blur supports-[backdrop-filter]:bg-bg/80"
    >
      <div className="flex items-center gap-3">
        {/* Status filter chips, with per-bucket counts. */}
        <div
          role="group"
          aria-label="Filter strategies by status"
          data-testid="strip-filter-chips"
          className="flex shrink-0 items-center gap-1"
        >
          {STRIP_FILTERS.map((f) => {
            const active = f === filter;
            return (
              <button
                key={f}
                type="button"
                aria-pressed={active}
                data-testid={`strip-filter-${f.toLowerCase()}`}
                onClick={() => setFilter(f)}
                className={[
                  "rounded-full border px-2 py-0.5 font-mono text-[10.5px] tracking-wide transition-colors",
                  active
                    ? "border-gold/45 bg-gold/10 text-gold"
                    : "border-border text-text-3 hover:text-text-2 hover:border-text-3",
                ].join(" ")}
              >
                {f} {counts[f]}
              </button>
            );
          })}
        </div>

        {/*
          The pill list is the only flexible child (flex-1 + min-w-0) and is
          its own scroll container, so scrolled pills clip at this box's edge
          and can never paint under the right-side controls. `overscroll-x-
          contain` stops a trackpad fling from chaining into page scroll.
        */}
        <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto overscroll-x-contain pb-0.5">
          {visible.length === 0 ? (
            <span
              data-testid="strip-empty-state"
              className="py-1 text-[13px] text-text-3"
            >
              {filter === "LIVE" ? (
                <>
                  No live strategies —{" "}
                  <Link
                    to="/strategies"
                    className="text-text-2 underline-offset-2 hover:text-text hover:underline"
                  >
                    deploy one
                  </Link>
                </>
              ) : filter === "ALL" ? (
                "No strategies."
              ) : (
                `No ${filter.toLowerCase()} strategies.`
              )}
            </span>
          ) : (
            visible.map((run) => {
              const isSelected = run.run_id === selectedId;
              return (
                <StrategyPill
                  key={run.run_id}
                  run={run}
                  selected={isSelected}
                  metric={metric}
                  strategies={strategies}
                  // Selected pill reflects the real chart stream status;
                  // others get a lightweight derived status so we don't
                  // open an EventSource per pill.
                  connStatus={
                    isSelected
                      ? selectedConnStatus
                      : isLiveRun(run)
                        ? "snapshot"
                        : "closed"
                  }
                  onSelect={() => onSelect(run.run_id)}
                  walletDisabled={walletDisabled}
                  transport={transportFor?.(run)}
                />
              );
            })
          )}
        </div>

        {/*
          Right control group: never shrinks, and a left border gives the
          scrolling pill list a hard visual boundary to clip against instead
          of pills appearing to run into the metric picker.
        */}
        <div className="flex shrink-0 items-center gap-3 border-l border-border pl-3">
          <SignalSelectMenu
            label="Metric"
            icon="sliders"
            value={metric}
            options={metricOptions}
            onChange={(v) => onMetricChange(v as StripMetricId)}
            align="right"
          />
          <Link
            to="/strategies"
            className="whitespace-nowrap text-[13px] font-medium text-text-2 hover:text-text"
          >
            Deploy strategy →
          </Link>
        </div>
      </div>
    </div>
  );
}
