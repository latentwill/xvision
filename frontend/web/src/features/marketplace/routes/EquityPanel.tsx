// src/features/marketplace/routes/EquityPanel.tsx
// Equity curve card wrapping HeroGradientEquity (uPlot v2).
// Two-layer approach per OQ-1:
//   - Faded dashed SVG polyline for the backtest segment
//   - HeroGradientEquity for the live segment (gold gradient)
// No primitive changes to HeroGradientEquity required.
import { useState } from "react";
import { HeroGradientEquity } from "@/components/chart/v2/primitives/HeroGradientEquity";
import type { EquityCurve } from "@/features/marketplace/data/types";

interface Props {
  curve: EquityCurve;
}

type Window = "30d" | "90d";

export function EquityPanel({ curve }: Props) {
  const [window, setWindow] = useState<Window>("90d");
  const [mintToggle, setMintToggle] = useState(false);

  const nowSec = Math.floor(Date.now() / 1000);
  const totalPts = curve.points.length;
  const windowPts = window === "30d" ? Math.min(30, totalPts) : totalPts;

  const sliced = curve.points.slice(totalPts - windowPts);
  const liveStartIdx = sliced.findIndex((p) => p.phase === "live");

  const timeAll = sliced.map((_, i) => nowSec - (sliced.length - 1 - i) * 86400);
  const valuesAll = sliced.map((p) => p.value);

  const backtestEnd = liveStartIdx === -1 ? sliced.length : liveStartIdx + 1;
  const valuesBacktest = sliced.slice(0, backtestEnd).map((p) => p.value);

  const timeLive =
    liveStartIdx === -1
      ? []
      : sliced.slice(liveStartIdx).map((_, i) => timeAll[liveStartIdx + i]);
  const valuesLive =
    liveStartIdx === -1 ? [] : sliced.slice(liveStartIdx).map((p) => p.value);

  const hasFinalLive = timeLive.length > 0;

  // For the SVG backtest polyline normalization
  const allVals = valuesAll;
  const minV = Math.min(...allVals);
  const maxV = Math.max(...allVals);
  const range = maxV - minV || 1;

  return (
    <div className="rounded-md border border-border bg-surface-card">
      {/* Card header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div>
          <span className="text-[13px] font-medium text-foreground">Return %</span>
          <span className="ml-2 font-mono text-[11px] text-text-3">
            base ${curve.base.toLocaleString()} · backtest (faded) + live (solid)
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setMintToggle((v) => !v)}
            className={[
              "px-2.5 py-1 rounded border text-[11px] font-medium",
              mintToggle
                ? "border-gold-soft bg-gold/[0.10] text-gold"
                : "border-border-strong bg-transparent text-text-2",
            ].join(" ")}
          >
            If I bought at mint
          </button>
          {(["30d", "90d"] as Window[]).map((w) => (
            <button
              key={w}
              onClick={() => setWindow(w)}
              className={[
                "px-2.5 py-1 rounded border text-[11px] font-medium",
                window === w
                  ? "border-gold-soft text-gold"
                  : "border-border-strong bg-transparent text-text-2",
              ].join(" ")}
            >
              {w}
            </button>
          ))}
        </div>
      </div>

      {/* Chart area */}
      <div className="px-4 pt-3 pb-2 relative" style={{ height: 200 }}>
        {/* Backtest layer — dashed SVG polyline, faded grey */}
        {valuesBacktest.length > 1 && (
          <svg
            className="absolute inset-0 w-full h-full pointer-events-none opacity-50"
            preserveAspectRatio="none"
            viewBox="0 0 100 100"
          >
            <polyline
              points={valuesBacktest
                .map((v, i) => {
                  const x = (i / (valuesBacktest.length - 1)) * 100;
                  const y = 100 - ((v - minV) / range) * 80 - 10;
                  return `${x},${y}`;
                })
                .join(" ")}
              fill="none"
              stroke="var(--text-3, #5F6670)"
              strokeWidth="0.5"
              strokeDasharray="2 2"
            />
          </svg>
        )}

        {/* Live layer — HeroGradientEquity (gold gradient fill + halo) */}
        {hasFinalLive ? (
          <HeroGradientEquity time={timeLive} values={valuesLive} height={180} />
        ) : (
          // All backtest — show full curve in live style
          <HeroGradientEquity time={timeAll} values={valuesAll} height={180} />
        )}

        {/* LIVE marker — positioned at the backtest/live boundary */}
        {hasFinalLive && liveStartIdx > 0 && (
          <span
            className="absolute top-2 font-mono text-[9.5px] tracking-[0.16em] text-gold pointer-events-none"
            style={{
              left: `${(liveStartIdx / sliced.length) * 100}%`,
            }}
          >
            LIVE
          </span>
        )}
      </div>
    </div>
  );
}
