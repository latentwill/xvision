/**
 * StrategyCard — one card in the B2 ComparisonABDashboard grid.
 *
 * Layout: head (color dot + Cormorant name + `.caps` kind line + LEAD
 * badge if first + `×` button when removable) → MiniSparkline (price
 * line area-filled in strategy color) → 2×2 metrics → indicator chip
 * strip.
 *
 * The card is wrapped by `LeadCardChrome` from the surface; this
 * component renders the inner content only so the chrome can switch
 * background/border without re-mounting the card body.
 */
import type { ReactElement } from "react";

import { MiniSparkline } from "./MiniSparkline";

export interface StrategyCardMetrics {
  return: number;
  sharpe: number;
  mdd: number;
  win: number;
}

export interface StrategyCardProps {
  id: string;
  name: string;
  /** Display caption under the name; e.g. "Trend · 50/200". */
  caption: string;
  color: string;
  metrics: StrategyCardMetrics;
  /** Parallel arrays for the mini equity sparkline. */
  time: number[];
  equity: number[];
  /** `LEAD` badge + gradient chrome upstream. */
  lead: boolean;
  /** Indicator chip strip — render-as-given. Defaults to no chips. */
  chips?: string[];
  /** Hide the `×` button when this strategy can't be removed. */
  removable: boolean;
  onRemove: (id: string) => void;
}

function fmtPct(n: number, d = 2): string {
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(d)}%`;
}

function fmtRatio(n: number): string {
  return n.toFixed(2);
}

export function StrategyCard({
  id,
  name,
  caption,
  color,
  metrics,
  time,
  equity,
  lead,
  chips = [],
  removable,
  onRemove,
}: StrategyCardProps): ReactElement {
  return (
    <div className="flex flex-col p-3" data-testid={`strategy-card-${id}`}>
      {/* HEAD */}
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 min-w-0">
            <span
              aria-hidden="true"
              className="inline-block w-2 h-2 rounded-full shrink-0"
              style={{
                backgroundColor: color,
                boxShadow: `0 0 0 3px ${color}1a`,
              }}
            />
            <div
              className="text-[15.5px] leading-tight text-text truncate"
              style={{ fontFamily: '"Cormorant Garamond", serif' }}
            >
              {name}
            </div>
            {lead && (
              <span className="caps text-gold ml-1 shrink-0">LEAD</span>
            )}
          </div>
          <div className="caps mt-1.5">{caption}</div>
        </div>
        {removable && (
          <button
            type="button"
            className="inline-flex items-center justify-center w-5 h-5 rounded text-text-3 hover:text-text"
            onClick={() => onRemove(id)}
            aria-label={`Remove ${name}`}
          >
            ×
          </button>
        )}
      </div>

      {/* SPARKLINE */}
      <div className="mt-2">
        <MiniSparkline time={time} values={equity} color={color} />
      </div>

      {/* METRICS 2×2 */}
      <div className="grid grid-cols-2 gap-2 mt-2 text-[12px]">
        <Stat label="Return" value={fmtPct(metrics.return)} />
        <Stat label="Sharpe" value={fmtRatio(metrics.sharpe)} />
        <Stat label="Max DD" value={fmtPct(metrics.mdd)} danger />
        <Stat label="Win" value={`${metrics.win.toFixed(1)}%`} />
      </div>

      {/* INDICATOR CHIPS */}
      {chips.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {chips.map((c) => (
            <span
              key={c}
              className="inline-flex items-center px-1.5 py-[1px] text-[10.5px] rounded border border-border-soft text-text-3"
              style={{ fontFamily: '"JetBrains Mono", monospace' }}
            >
              {c}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  danger = false,
}: {
  label: string;
  value: string;
  danger?: boolean;
}): ReactElement {
  return (
    <div className="flex items-baseline justify-between">
      <span className="caps">{label}</span>
      <span
        className={[
          "tabular-nums",
          danger ? "text-danger" : "text-text",
        ].join(" ")}
        style={{ fontFamily: '"JetBrains Mono", monospace' }}
      >
        {value}
      </span>
    </div>
  );
}
