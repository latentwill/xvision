// Live cockpit strategy strip (spec §2.4).
//
// Fixed horizontal strip, always visible (does not scroll with the body).
// Renders one pill per live-run, a column picker for the configurable
// metric, and a "Deploy strategy →" link to /strategies at the right end.

import { Link } from "react-router-dom";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { LiveStatus } from "@/components/chart/use-run-stream";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";
import type { NamedStrategy } from "@/lib/run-display";
import { StrategyPill } from "./StrategyPill";
import { isLiveRun } from "./strip-status";
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

  return (
    <div
      data-testid="strategy-strip"
      className="sticky top-0 z-10 -mx-6 mb-4 border-b border-border bg-bg/95 px-6 py-3 backdrop-blur supports-[backdrop-filter]:bg-bg/80"
    >
      <div className="flex items-center gap-3">
        <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto pb-0.5">
          {runs.length === 0 ? (
            <span className="py-1 text-[13px] text-text-3">
              No live strategies.
            </span>
          ) : (
            runs.map((run) => {
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

        <div className="flex shrink-0 items-center gap-3">
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
