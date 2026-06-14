// ArenaStandingIndicator — Phase 8 "arena standing" inline strip.
//
// Presentational only (no network calls). Props-driven — the parent supplies
// the arena standing data; this component renders it as a compact chip row.
//
// Layout rules (project CLAUDE.md):
//   - Full-width, inline (NO right-side box; no popups/modals).
//   - Dark-mode: no pure-white borders — use theme tokens (border-border).
//
// Props:
//   tradingViaArena  — whether the strategy is routed through the Arena.
//   aiPotInView      — whether the AI pot is currently in view.
//   rank             — optional current leaderboard rank (1-based).
//   pnlUsd           — optional live P&L in USD (positive = profit, negative = loss).

export interface ArenaStandingIndicatorProps {
  /** Strategy is actively routed through the Degen Arena. */
  tradingViaArena: boolean;
  /** AI pot is visible / being tracked. */
  aiPotInView: boolean;
  /** Current leaderboard rank. Omit or null when not yet ranked. */
  rank?: number | null;
  /** Live P&L in USD. Omit or null when unavailable. */
  pnlUsd?: number | null;
}

function Chip({
  label,
  active,
  testId,
}: {
  label: string;
  active: boolean;
  testId?: string;
}) {
  return (
    <span
      data-testid={testId}
      className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-[11px] font-medium ${
        active
          ? "border-success/40 bg-success/10 text-success"
          : "border-border bg-surface-inset text-text-3"
      }`}
    >
      {label}
      <span aria-hidden="true">{active ? "✓" : "—"}</span>
    </span>
  );
}

function formatPnl(pnl: number): string {
  const sign = pnl >= 0 ? "+" : "-";
  return `${sign}$${Math.abs(pnl).toFixed(2)}`;
}

export function ArenaStandingIndicator({
  tradingViaArena,
  aiPotInView,
  rank,
  pnlUsd,
}: ArenaStandingIndicatorProps) {
  const hasRank = rank != null;
  const hasPnl = pnlUsd != null;

  return (
    <div
      data-testid="arena-standing-indicator"
      className="flex flex-wrap items-center gap-2 rounded border border-border bg-surface-card px-4 py-2.5"
    >
      {/* Section label */}
      <span className="mr-1 text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
        Arena standing
      </span>

      {/* Trading via Arena chip */}
      <Chip
        label="Trading via Arena"
        active={tradingViaArena}
        testId="chip-trading-via-arena"
      />

      {/* AI Pot in view chip */}
      <Chip
        label="AI Pot in view"
        active={aiPotInView}
        testId="chip-ai-pot-in-view"
      />

      {/* Optional rank chip */}
      {hasRank && (
        <span
          data-testid="chip-rank"
          className="inline-flex items-center gap-1 rounded-full border border-border bg-surface-inset px-2.5 py-0.5 text-[11px] font-medium text-text-2"
        >
          Rank&nbsp;
          <span data-testid="rank-value" className="font-mono text-text">
            #{rank}
          </span>
        </span>
      )}

      {/* Optional live PnL chip */}
      {hasPnl && (
        <span
          data-testid="chip-pnl"
          className={`inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] font-mono font-medium ${
            pnlUsd >= 0
              ? "border-success/40 bg-success/10 text-success"
              : "border-danger/40 bg-danger/10 text-danger"
          }`}
        >
          <span data-testid="pnl-value">{formatPnl(pnlUsd)}</span>
        </span>
      )}
    </div>
  );
}
