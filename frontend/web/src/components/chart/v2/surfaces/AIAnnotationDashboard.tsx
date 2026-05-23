/**
 * AIAnnotationDashboard — surface for `/charts/annotated` (B3).
 *
 * Header: xvn lozenge + symbol pill + price + 24h change +
 * AiEnginePill + filter toggle row (Patterns / Risk / Flow / All).
 * Body: <grid 1fr 280|36px> of (KlineCandlePane + AnnotationOverlay)
 * and InsightLog. Footer: JetBrains-Mono 10.5px status line.
 *
 * The candle pane is the existing Track-A KlineCandlePane primitive.
 * Annotations live in their own overlay above it; the overlay
 * geometrically approximates anchor positions using the kline-anchor
 * adapter (DEFAULT_BOUNDS-based — B3 ships without pan/zoom
 * re-anchoring; tracked as a follow-up).
 */
import { useMemo, useState, type ReactElement } from "react";

import type { AnnotatedChartPayload, Annotation } from "../types";
import { KlineCandlePane } from "../primitives/KlineCandlePane";
import { AnnotationOverlay } from "../primitives/AnnotationOverlay";
import { InsightLog } from "../primitives/InsightLog";
import { AiEnginePill } from "../primitives/AiEnginePill";
import { EmptyState } from "../primitives/EmptyState";

export interface AIAnnotationDashboardProps {
  payload: AnnotatedChartPayload;
}

const FILTER_TABS: { id: "ALL" | "PATTERN" | "RISK" | "FLOW"; label: string }[] = [
  { id: "PATTERN", label: "Patterns" },
  { id: "RISK", label: "Risk" },
  { id: "FLOW", label: "Flow" },
  { id: "ALL", label: "All" },
];

/** Map a UI filter selection to the set of annotation types it allows.
 *  Exported for tests. */
export function typesForFilter(
  filter: "ALL" | "PATTERN" | "RISK" | "FLOW",
): ReadonlySet<Annotation["type"]> | undefined {
  switch (filter) {
    case "ALL":
      return undefined; // no filter
    case "PATTERN":
      return new Set<Annotation["type"]>(["PATTERN", "STRUCTURE"]);
    case "RISK":
      return new Set<Annotation["type"]>(["RISK"]);
    case "FLOW":
      return new Set<Annotation["type"]>(["FLOW", "REVERSION"]);
  }
}

function lastClose(payload: AnnotatedChartPayload): number | undefined {
  const c = payload.candles.close;
  return c.length > 0 ? c[c.length - 1] : undefined;
}

function pct24h(payload: AnnotatedChartPayload): number | undefined {
  const c = payload.candles.close;
  if (c.length < 24) return undefined;
  const last = c[c.length - 1];
  const ref = c[c.length - 24];
  if (ref === 0) return undefined;
  return ((last - ref) / ref) * 100;
}

export function AIAnnotationDashboard({
  payload,
}: AIAnnotationDashboardProps): ReactElement {
  const [filter, setFilter] = useState<"ALL" | "PATTERN" | "RISK" | "FLOW">(
    "ALL",
  );
  const [logOpen, setLogOpen] = useState(true);

  const visibleTypes = typesForFilter(filter);
  const last = lastClose(payload);
  const change24 = pct24h(payload);

  const isLiveEmpty =
    payload.source === "live" && payload.annotations.length === 0;

  // Insight log is filtered by the same type set as the chart overlay.
  const filteredAnnotations = useMemo(
    () =>
      visibleTypes
        ? payload.annotations.filter((a) => visibleTypes.has(a.type))
        : payload.annotations,
    [payload.annotations, visibleTypes],
  );

  return (
    <div className="flex flex-col gap-3">
      <header className="flex items-center justify-between gap-4 px-1">
        <div className="flex items-center gap-4">
          <span
            className="text-[22px] italic text-text leading-none"
            style={{ fontFamily: '"Cormorant Garamond", serif' }}
          >
            xvn
          </span>
          <div className="h-5 w-px bg-border-soft" aria-hidden="true" />
          <div>
            <div className="caps">
              {payload.asset} · {payload.granularity}
              {payload.source === "live" ? " · live" : " · run"}
            </div>
            <div className="flex items-center gap-2 mt-1">
              {last != null && (
                <span
                  className="text-[20px] text-text"
                  style={{ fontFamily: '"Cormorant Garamond", serif' }}
                >
                  {last.toLocaleString(undefined, { maximumFractionDigits: 2 })}
                </span>
              )}
              {change24 != null && (
                <span
                  className={[
                    "text-[12px] tabular-nums",
                    change24 >= 0 ? "text-[#3FAE6B]" : "text-danger",
                  ].join(" ")}
                  style={{ fontFamily: '"JetBrains Mono", monospace' }}
                >
                  {change24 >= 0 ? "+" : ""}
                  {change24.toFixed(2)}%
                </span>
              )}
              <span className="caps">24h</span>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <AiEnginePill />
          <span className="inline-flex items-center px-2.5 py-1 rounded-full border border-border-soft text-[11px] text-text-3">
            model · xvn-annot-v3
          </span>
          <div
            className="inline-flex rounded border border-border-soft overflow-hidden"
            role="tablist"
            aria-label="Annotation filter"
          >
            {FILTER_TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={filter === t.id}
                onClick={() => setFilter(t.id)}
                className={[
                  "px-2.5 py-1 text-[11.5px] border-r border-border-soft last:border-r-0 transition-colors",
                  filter === t.id
                    ? "bg-gold/[0.10] text-gold"
                    : "text-text-3 hover:text-text",
                ].join(" ")}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>
      </header>

      <div
        className="grid border border-border rounded-card overflow-hidden bg-surface-card"
        style={{
          gridTemplateColumns: logOpen ? "1fr 280px" : "1fr 36px",
          transition: "grid-template-columns 200ms ease",
        }}
      >
        <div className="relative" style={{ minHeight: 480 }}>
          <KlineCandlePane candles={payload.candles} height={480} />
          {isLiveEmpty ? (
            <div className="absolute inset-0 flex items-center justify-center p-6">
              <EmptyState
                title="Annotation producer not configured"
                message="Live annotations require the producer wiring (out of scope for chart-rework Track B). Switch to a stored run via the source picker once it ships."
              />
            </div>
          ) : (
            <AnnotationOverlay
              candles={payload.candles}
              annotations={payload.annotations}
              visibleTypes={visibleTypes}
            />
          )}
        </div>
        <InsightLog
          annotations={filteredAnnotations}
          open={logOpen}
          onToggle={() => setLogOpen((v) => !v)}
        />
      </div>

      <footer
        className="text-[10.5px] text-text-3 px-1"
        style={{ fontFamily: '"JetBrains Mono", monospace' }}
      >
        EMA(21) · candle_pane · drag to pan · callouts approximate-anchored ·{" "}
        {payload.annotations.length} annotations · source: {payload.source}
      </footer>
    </div>
  );
}
