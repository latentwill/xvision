// Live Trading account stat strip (Task B-II / spec §2.6 — B3).
//
// Four stats in a single full-width horizontal row, rendered INLINE below the
// chart inside the existing seam (NO right-side box — the chat rail owns the
// right edge). Derivation lives in `live-account.ts`; this file is the thin
// display layer. PnL stats are color-coded with theme tokens (gold positive /
// danger negative). Any value not cleanly derivable renders "—".

import { useMemo } from "react";

import type { RunChartPayload } from "@/api/types.gen";

import {
  currentEquity,
  dailyPnl,
  drawdownFromPeak,
  latestCloseByAsset,
  unrealizedPnl,
} from "./live-account";
import {
  DASH,
  barsByAsset,
  fmtPctSigned,
  fmtUsdPlain,
  fmtUsdSigned,
  pnlTone,
} from "./live-format";
import { derivePositionsByDecision } from "@/features/decisions/positions";
import type { DecisionRowDto } from "@/api/types.gen";

export interface LiveAccountStripProps {
  /** Lifted live stream payload (shared with the chart + positions table). */
  data: RunChartPayload | undefined;
  /** Fetched decision rows for the selected run (for unrealized PnL). */
  decisions: DecisionRowDto[];
}

export function LiveAccountStrip({ data, decisions }: LiveAccountStripProps) {
  const stats = useMemo(() => {
    if (!data) {
      return {
        equity: null as number | null,
        daily: { usd: null as number | null, pct: null as number | null, basis: "none" as const },
        drawdown: null as number | null,
        unrealized: null as number | null,
      };
    }
    const prices = latestCloseByAsset(barsByAsset(data));
    const byDecision = derivePositionsByDecision(decisions);
    const maxIndex =
      byDecision.size > 0 ? Math.max(...byDecision.keys()) : null;
    const open = maxIndex != null ? byDecision.get(maxIndex) ?? [] : [];
    return {
      equity: currentEquity(data.equity),
      daily: dailyPnl(data.equity),
      drawdown: drawdownFromPeak(data.drawdown, data.equity),
      unrealized: unrealizedPnl(open, prices),
    };
  }, [data, decisions]);

  const dailyLabel =
    stats.daily.basis === "series-start" ? "PnL (since start)" : "Daily PnL";

  return (
    <div
      data-testid="live-account-strip"
      className="grid grid-cols-2 gap-px overflow-hidden rounded-card border border-border bg-border sm:grid-cols-4"
    >
      <Stat label="Current equity" value={fmtUsdPlain(stats.equity)} />
      <Stat
        label={dailyLabel}
        value={fmtUsdSigned(stats.daily.usd)}
        sub={fmtPctSigned(stats.daily.pct)}
        tone={pnlTone(stats.daily.usd)}
      />
      <Stat
        label="Drawdown from peak"
        value={stats.drawdown == null ? DASH : `${stats.drawdown.toFixed(2)}%`}
        // Drawdown is a loss metric: any non-zero magnitude is bad news.
        tone={
          stats.drawdown == null || stats.drawdown === 0
            ? "text-text"
            : "text-danger"
        }
      />
      <Stat
        label="Unrealized PnL"
        value={fmtUsdSigned(stats.unrealized)}
        tone={pnlTone(stats.unrealized)}
      />
    </div>
  );
}

function Stat({
  label,
  value,
  sub,
  tone = "text-text",
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: string;
}) {
  return (
    <div className="bg-surface-card px-4 py-3">
      <div className="text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
        {label}
      </div>
      <div className={`mt-1 text-[18px] font-semibold tabular-nums ${tone}`}>
        {value}
      </div>
      {sub != null && (
        <div className={`text-[12px] tabular-nums ${tone}`}>{sub}</div>
      )}
    </div>
  );
}
