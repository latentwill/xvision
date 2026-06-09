// Live Trading active positions table (Task B-II / spec §2.6 — B4).
//
// Columns: Symbol, Entry price, Qty, Entry time, Current value, Unrealized
// PnL, % gain/loss. Rows = current open positions (from
// `derivePositionsByDecision` at the max decision_index, enriched in
// `live-account.ts::buildPositionRows`). NO pagination. Full-width inline
// below the stat strip (NO right-side box). Clean themed empty state.

import { useMemo } from "react";

import type { DecisionRowDto, RunChartPayload } from "@/api/types.gen";

import { buildPositionRows, latestCloseByAsset, type PositionRow } from "./live-account";
import {
  DASH,
  barsByAsset,
  fmtPctSigned,
  fmtUsdPlain,
  fmtUsdSigned,
  pnlTone,
} from "./live-format";

function fmtQty(n: number): string {
  // Crypto sizes can be fractional; show up to 6 sig places without trailing
  // noise.
  return n.toLocaleString("en-US", { maximumFractionDigits: 6 });
}

function fmtEntryTime(iso: string | null): string {
  if (!iso) return DASH;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return DASH;
  // Local time, compact — matches the operator's wall clock.
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export interface LivePositionsTableProps {
  data: RunChartPayload | undefined;
  decisions: DecisionRowDto[];
}

export function LivePositionsTable({ data, decisions }: LivePositionsTableProps) {
  const rows = useMemo<PositionRow[]>(() => {
    if (!data) return [];
    const prices = latestCloseByAsset(barsByAsset(data));
    return buildPositionRows(decisions, prices);
  }, [data, decisions]);

  return (
    <div
      data-testid="live-positions-table"
      className="overflow-hidden rounded-card border border-border bg-surface-card"
    >
      <div className="border-b border-border px-4 py-2.5">
        <span className="text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
          Active positions
        </span>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-[12.5px] tabular-nums">
          <thead>
            <tr className="text-left text-[10px] font-mono uppercase tracking-[0.12em] text-text-3">
              <Th className="text-left">Symbol</Th>
              <Th className="text-right">Entry price</Th>
              <Th className="text-right">Qty</Th>
              <Th className="text-left">Entry time</Th>
              <Th className="text-right">Current value</Th>
              <Th className="text-right">Unrealized PnL</Th>
              <Th className="text-right">% gain/loss</Th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td
                  colSpan={7}
                  className="px-4 py-8 text-center text-[13px] text-text-3"
                >
                  No open positions
                </td>
              </tr>
            ) : (
              rows.map((r) => (
                <tr
                  key={r.asset}
                  className="border-t border-border/60 text-text-2"
                >
                  <td className="px-4 py-2.5">
                    <span className="font-mono text-text">{r.asset}</span>
                    <span
                      className={`ml-2 text-[10px] uppercase ${
                        r.side === "long" ? "text-gold" : "text-danger"
                      }`}
                    >
                      {r.side}
                    </span>
                  </td>
                  <td className="px-4 py-2.5 text-right">
                    {fmtUsdPlain(r.entry_price)}
                  </td>
                  <td className="px-4 py-2.5 text-right">{fmtQty(r.qty)}</td>
                  <td className="px-4 py-2.5 text-left text-text-3">
                    {fmtEntryTime(r.entry_time)}
                  </td>
                  <td className="px-4 py-2.5 text-right">
                    {fmtUsdPlain(r.current_value)}
                  </td>
                  <td
                    className={`px-4 py-2.5 text-right ${pnlTone(r.unrealized_pnl)}`}
                  >
                    {fmtUsdSigned(r.unrealized_pnl)}
                  </td>
                  <td
                    className={`px-4 py-2.5 text-right ${pnlTone(r.pct_change)}`}
                  >
                    {fmtPctSigned(r.pct_change)}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Th({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return <th className={`px-4 py-2 font-medium ${className}`}>{children}</th>;
}
