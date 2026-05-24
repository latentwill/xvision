/**
 * DrawdownCard — composes the existing `UplotDrawdownPane` with a
 * 4-cell footer (Max DD / Avg DD / Duration / Recovery).
 *
 * Per spec §4A.1 (B1) + design handoff §01 ("Drawdown · <strategy>"
 * card). The 4 footer cells use `.caps` eyebrows + JetBrains Mono
 * values. The B4 hero variant tinted-red drawdown lead can be enabled
 * later via a `leadStyle` prop.
 */
import type { ReactElement } from "react";

import { type DrawdownPoint } from "../types";
import { UplotDrawdownPane } from "./UplotDrawdownPane";

export interface DrawdownStats {
  /** Worst drawdown (most-negative). Display already-formatted with sign. */
  maxDrawdownPct: number;
  /** Mean drawdown over the period. */
  avgDrawdownPct: number;
  /** Longest drawdown duration, in days. */
  durationDays: number;
  /** Days from worst trough back to a new high. `null` if not yet recovered. */
  recoveryDays: number | null;
}

export type DrawdownLeadStyle = "default" | "gold-tinted-red";

export interface DrawdownCardProps {
  title?: string;
  /** Time-series drawdown points (≤ 0 values). */
  points: DrawdownPoint[];
  stats: DrawdownStats;
  height?: number;
  /** Cosmetic variant. `gold-tinted-red` (B4 hero) tints the title
   *  border + footer dividers with gold; the pane itself stays
   *  driven by Chart2ThemeDefinition.panes.drawdown. */
  leadStyle?: DrawdownLeadStyle;
}

/**
 * Format a percent value for display. Always shows two decimals with
 * an explicit sign; e.g. `-18.72` → `"-18.72%"`.
 * Exported for tests.
 */
export function formatPct(value: number): string {
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(2)}%`;
}

function formatDays(days: number | null): string {
  if (days == null) return "—";
  return `${Math.round(days)}d`;
}

export function DrawdownCard({
  title = "Drawdown",
  points,
  stats,
  height = 140,
  leadStyle = "default",
}: DrawdownCardProps): ReactElement {
  const isLead = leadStyle === "gold-tinted-red";
  return (
    <div
      className={[
        "border rounded-card overflow-hidden",
        isLead
          ? "bg-surface-card"
          : "border-border bg-surface-card",
      ].join(" ")}
      style={isLead ? { borderColor: "rgba(212,165,71,0.32)" } : undefined}
      data-testid="drawdown-card"
      data-lead-style={leadStyle}
    >
      <header
        className={[
          "px-4 py-3 border-b",
          isLead ? "" : "border-border",
        ].join(" ")}
        style={isLead ? { borderBottomColor: "rgba(212,165,71,0.28)" } : undefined}
      >
        <div className="caps">{title}</div>
      </header>
      <div className="px-4 py-3">
        <UplotDrawdownPane points={points} height={height} />
      </div>
      <footer
        className={[
          "grid grid-cols-2 sm:grid-cols-4 gap-3 px-4 py-3 border-t",
          isLead ? "" : "border-border-soft",
        ].join(" ")}
        style={isLead ? { borderTopColor: "rgba(212,165,71,0.20)" } : undefined}
      >
        <Stat label="Max DD" value={formatPct(stats.maxDrawdownPct)} danger />
        <Stat label="Avg DD" value={formatPct(stats.avgDrawdownPct)} danger />
        <Stat label="Duration" value={formatDays(stats.durationDays)} />
        <Stat label="Recovery" value={formatDays(stats.recoveryDays)} />
      </footer>
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
    <div>
      <div className="caps">{label}</div>
      <div
        className={[
          "mt-1 text-[14px] tabular-nums",
          danger ? "text-danger" : "text-text",
        ].join(" ")}
        style={{ fontFamily: 'Geist Mono, ui-monospace, monospace' }}
      >
        {value}
      </div>
    </div>
  );
}
