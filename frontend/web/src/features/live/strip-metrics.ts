// Strategy-strip configurable metric definitions.
//
// The strip pill shows ONE operator-selected metric (spec §2.4). The
// operator picks it from a column picker; the choice persists to
// localStorage under `STRIP_METRIC_STORAGE_KEY`.
//
// Data source today is `AgentRunSummary` (from `listAgentRuns`), which
// carries run-level aggregates but NOT the financial series (equity,
// PnL, sharpe, drawdown) — those live on the eval `RunSummary` / equity
// stream and are wired by B-II. Metrics we cannot yet derive return
// `null`; the renderer shows "—" rather than faking a value.

import type { AgentRunSummary } from "@/api/types-agent-runs";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";

export const STRIP_METRIC_STORAGE_KEY = "live_trading_strip_metric";

export type StripMetricId =
  | "daily_pnl_usd"
  | "daily_pnl_pct"
  | "unrealized_pnl"
  | "current_equity"
  | "trades_today"
  | "decisions_today"
  | "run_time"
  | "sharpe"
  | "max_drawdown";

export const DEFAULT_STRIP_METRIC: StripMetricId = "daily_pnl_usd";

export interface StripMetricOption {
  id: StripMetricId;
  label: string;
}

// Order mirrors the spec's enumeration. Default (daily PnL $) first.
export const STRIP_METRIC_OPTIONS: StripMetricOption[] = [
  { id: "daily_pnl_usd", label: "Daily PnL ($)" },
  { id: "daily_pnl_pct", label: "Daily PnL (%)" },
  { id: "unrealized_pnl", label: "Unrealized PnL" },
  { id: "current_equity", label: "Current equity" },
  { id: "trades_today", label: "Trades today" },
  { id: "decisions_today", label: "Decisions today" },
  { id: "run_time", label: "Run time" },
  { id: "sharpe", label: "Sharpe" },
  { id: "max_drawdown", label: "Max drawdown" },
];

/** A rendered metric value plus its sign tone for color-coding. */
export interface MetricValue {
  /** Display text, or "—" when the metric isn't derivable yet. */
  text: string;
  /** Sign-driven tone for color: positive / negative / neutral. */
  tone: "pos" | "neg" | "neutral";
  /** True when the value is a real (non-placeholder) figure. */
  derived: boolean;
}

const DASH: MetricValue = { text: "—", tone: "neutral", derived: false };

function fmtDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

/**
 * Compute the display value for a strip metric from the run summary.
 *
 * Financial metrics (PnL, equity, sharpe, drawdown) are not present on
 * `AgentRunSummary` yet — they return the dash placeholder. B-II wires
 * the equity-stream / eval-RunSummary source and replaces these arms.
 */
export function computeStripMetric(
  id: StripMetricId,
  run: AgentRunSummary,
  now: number = Date.now(),
): MetricValue {
  switch (id) {
    case "trades_today":
      // TODO(B-II): scope to "today" once the equity/trade stream is wired;
      // tool_call_count is the closest run-level proxy available now.
      return {
        text: String(run.tool_call_count),
        tone: "neutral",
        derived: true,
      };
    case "decisions_today":
      // TODO(B-II): scope to "today"; model_call_count is the run-level proxy.
      return {
        text: String(run.model_call_count),
        tone: "neutral",
        derived: true,
      };
    case "run_time": {
      const startMs = new Date(run.started_at).getTime();
      const elapsed = run.duration_ms ?? (Number.isFinite(startMs) ? now - startMs : NaN);
      if (!Number.isFinite(elapsed) || elapsed < 0) return DASH;
      return { text: fmtDuration(elapsed), tone: "neutral", derived: true };
    }
    // Financial metrics — not derivable from AgentRunSummary. B-II fills these.
    case "daily_pnl_usd": // TODO(B-II): equity-stream delta since midnight UTC, $
    case "daily_pnl_pct": // TODO(B-II): equity-stream delta since midnight UTC, %
    case "unrealized_pnl": // TODO(B-II): sum of open-position unrealized PnL
    case "current_equity": // TODO(B-II): latest equity_usd point
    case "sharpe": // TODO(B-II): RunSummary.sharpe (eval runs source)
    case "max_drawdown": // TODO(B-II): RunSummary.max_drawdown_pct
      return DASH;
    default:
      return DASH;
  }
}

export function isStripMetricId(v: string | null): v is StripMetricId {
  return STRIP_METRIC_OPTIONS.some((o) => o.id === v);
}

export function loadStripMetric(): StripMetricId {
  const raw = safeStorageGet(STRIP_METRIC_STORAGE_KEY);
  return isStripMetricId(raw) ? raw : DEFAULT_STRIP_METRIC;
}

export function saveStripMetric(id: StripMetricId): void {
  safeStorageSet(STRIP_METRIC_STORAGE_KEY, id);
}
