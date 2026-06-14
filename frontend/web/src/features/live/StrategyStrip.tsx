// Live Trading run list (spec §2.4).
//
// Fixed horizontal strip, always visible (does not scroll with the body).
// One horizontal band, left → right:
//
//   [ALL n] [LIVE n] [PAUSED n] [STOPPED n] | run list… | deploy
//
// Status filter chips gate which runs render as rows. Default filter is
// LIVE — only runs where `isLiveRun()` is true (real money moving now) may
// appear there; backtests, orphans, and terminal runs land under their real
// bucket (see `stripFilterBucket`). Zero live runs ⇒ a quiet empty state
// with the deploy link instead of stale rows.

import { useState } from "react";
import { Link } from "react-router-dom";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { LiveStatus } from "@/components/chart/use-run-stream";
import { displayStrategyName, type NamedStrategy } from "@/lib/run-display";
import { ConnectionDot } from "./ConnectionDot";
import { TransportControls } from "./TransportControls";
import {
  filterRunsForStrip,
  isLiveRun,
  STRIP_FILTERS,
  stripFilterCounts,
  deriveStripStatus,
  type StripStatus,
  type StripFilter,
} from "./strip-status";
import {
  computeStripMetric,
} from "./strip-metrics";
import type { RunTransport } from "./useTransport";

const STATUS_STYLE: Record<StripStatus, string> = {
  ACTIVE: "bg-info/15 text-info",
  PAUSED: "bg-warn/15 text-warn",
  STOPPED: "bg-surface-elev text-text-3",
  STALE: "bg-surface-elev text-text-3",
};

const METRIC_TONE: Record<"pos" | "neg" | "neutral", string> = {
  pos: "text-info",
  neg: "text-danger",
  neutral: "text-text-2",
};

export interface StrategyStripProps {
  runs: AgentRunSummary[];
  selectedId: string | null;
  onSelect: (runId: string) => void;
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
  selectedConnStatus,
  walletDisabled,
  strategies,
  transportFor,
}: StrategyStripProps) {
  // Status filter — LIVE by default so dead/backtest rows never greet
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
          The run list is the only flexible child (flex-1 + min-w-0). It owns
          overflow, so long strategy names or many launches never paint under
          the right-side controls.
        */}
        <div
          data-testid="live-run-list"
          className="flex min-w-0 flex-1 flex-col gap-1 overflow-y-auto overscroll-y-contain"
        >
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
                <LiveRunRow
                  key={run.run_id}
                  run={run}
                  selected={isSelected}
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
          of rows appearing to run into the deploy link.
        */}
        <div className="flex shrink-0 items-center gap-3 border-l border-border pl-3">
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

interface LiveRunRowProps {
  run: AgentRunSummary;
  selected: boolean;
  connStatus: LiveStatus;
  onSelect: () => void;
  walletDisabled: boolean;
  strategies?: NamedStrategy[];
  transport?: RunTransport;
}

function LiveRunRow({
  run,
  selected,
  connStatus,
  onSelect,
  walletDisabled,
  strategies,
  transport,
}: LiveRunRowProps) {
  const status = deriveStripStatus(run);
  const name = run.agent_id
    ? displayStrategyName(run.agent_id, strategies)
    : run.strategy_id
      ? displayStrategyName(run.strategy_id, strategies)
      : (() => {
          const obj = run.objective ?? "";
          if (!obj) return run.run_id.slice(0, 8);
          // Strip a leading "eval:<kind>:" prefix (e.g. "eval:Backtest:scenario")
          const stripped = obj.replace(/^eval:[^:]+:/i, "");
          // De-slug: replace hyphens/underscores with spaces and title-case
          const deslug = stripped
            .replace(/[-_]+/g, " ")
            .trim()
            .replace(/\b\w/g, (ch) => ch.toUpperCase());
          return deslug || run.run_id.slice(0, 8);
        })();
  const pnl = computeStripMetric("daily_pnl_usd", run);
  const sharpe = computeStripMetric("sharpe", run);

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      aria-label={`Live run ${name}`}
      data-selected={selected || undefined}
      data-testid={`live-run-row-${run.run_id}`}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      className={[
        "grid min-w-0 grid-cols-[minmax(200px,1fr)_74px_96px_76px_82px_80px_92px] items-center gap-3",
        "border px-3 py-2 text-[13px] transition-colors focus:outline-none",
        selected
          ? "border-gold/45 bg-gold/10"
          : "border-border bg-surface hover:bg-surface-hover focus:bg-surface-hover",
      ].join(" ")}
    >
      <div className="flex min-w-0 items-center gap-2.5">
        <ConnectionDot status={connStatus} />
        <span className="min-w-0 truncate font-medium text-text">{name}</span>
        <span
          className={`w-fit shrink-0 rounded px-1.5 py-0.5 text-[10.5px] font-semibold tracking-wide ${STATUS_STYLE[status]}`}
        >
          {status}
        </span>
      </div>
      <MetricCell label="PnL" value={pnl.text} tone={pnl.tone} />
      <MetricCell label="Decisions" value={String(run.model_call_count ?? "—")} />
      <MetricCell label="Trades" value={String(run.tool_call_count ?? "—")} />
      <MetricCell label="Sharpe" value={sharpe.text} tone={sharpe.tone} />
      {run.span_count > 0 ? (
        <Link
          to={`/live/runs/${encodeURIComponent(run.run_id)}`}
          onClick={(e) => e.stopPropagation()}
          className="w-fit font-mono text-[12px] text-text-2 hover:text-text"
        >
          Trace {run.span_count}
        </Link>
      ) : (
        <span className="font-mono text-[12px] text-text-3">Trace —</span>
      )}
      <div className="justify-self-end">
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

function MetricCell({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "pos" | "neg" | "neutral";
}) {
  return (
    <span className="min-w-0 font-mono text-[12px] text-text-3">
      {label}{" "}
      <span className={`tabular-nums ${METRIC_TONE[tone]}`}>{value}</span>
    </span>
  );
}
