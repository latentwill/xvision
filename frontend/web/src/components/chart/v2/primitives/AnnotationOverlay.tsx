/**
 * AnnotationOverlay — absolutely-positioned layer above the
 * KlineCandlePane. Renders two rows of Callouts (top + bottom) with
 * SVG connectors (dashed 3 3) pointing from the candle anchor
 * (r=6 ring + r=2.4 dot) to the nearest callout corner.
 *
 * Callouts are spread evenly across the usable width
 * (full width - padLeft - padRight). The anchor x is computed from
 * `xForIndex(idx, candleCount, bounds)`; the anchor y is computed from
 * `yForPrice(price, range, bounds)` using the candle's high (top
 * callout) or low (bottom callout).
 *
 * Layout re-anchors via a ResizeObserver on the host. Pan/zoom of the
 * underlying candles isn't tracked yet (B3-MVP); the spec's
 * pixel-perfect re-anchoring via `onVisibleRangeChange` is a follow-up.
 */
import { useEffect, useRef, useState, type ReactElement } from "react";

import type { Annotation, CandleColumns } from "../types";
import {
  DEFAULT_BOUNDS,
  deriveRange,
  xForIndex,
  yForPrice,
  type AnchorBounds,
} from "../adapters/kline-anchor";

import { Callout, CALLOUT_WIDTH } from "./Callout";

export interface AnnotationOverlayProps {
  candles: CandleColumns;
  annotations: Annotation[];
  /** Filter for the current visible types. Defaults to all. */
  visibleTypes?: ReadonlySet<Annotation["type"]>;
}

/**
 * Compute the centre x for each callout in `row`, spread evenly
 * across the usable width (`bounds.width - bounds.padLeft - bounds.padRight`).
 * Exported for tests.
 */
export function spreadRowXs(
  rowCount: number,
  bounds: AnchorBounds,
  calloutWidth: number,
): number[] {
  if (rowCount <= 0) return [];
  const usable = bounds.width - bounds.padLeft - bounds.padRight - calloutWidth;
  if (usable <= 0) {
    // Stack at the leftmost slot if the host is too narrow.
    return Array(rowCount).fill(bounds.padLeft);
  }
  if (rowCount === 1) return [bounds.padLeft + usable / 2];
  const step = usable / (rowCount - 1);
  return Array.from({ length: rowCount }, (_, i) => bounds.padLeft + step * i);
}

interface PositionedCallout {
  ann: Annotation;
  /** Top-left of the callout card, in CSS pixels relative to the host. */
  cx: number;
  cy: number;
  /** Anchor pixel position on the candle (where the dot is drawn). */
  ax: number;
  ay: number;
}

export function AnnotationOverlay({
  candles,
  annotations,
  visibleTypes,
}: AnnotationOverlayProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const [bounds, setBounds] = useState<AnchorBounds>(DEFAULT_BOUNDS);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const obs = new ResizeObserver(() => {
      const rect = host.getBoundingClientRect();
      setBounds((prev) => ({
        ...prev,
        width: rect.width,
        height: rect.height,
      }));
    });
    obs.observe(host);
    // Prime once on mount.
    const rect = host.getBoundingClientRect();
    setBounds((prev) => ({ ...prev, width: rect.width, height: rect.height }));
    return () => obs.disconnect();
  }, []);

  const filtered = visibleTypes
    ? annotations.filter((a) => visibleTypes.has(a.type))
    : annotations;

  const count = candles.time.length;
  const range = deriveRange(candles.high, candles.low);

  const tops = filtered.filter((a) => a.side === "top");
  const bots = filtered.filter((a) => a.side === "bottom");

  const topXs = spreadRowXs(tops.length, bounds, CALLOUT_WIDTH);
  const botXs = spreadRowXs(bots.length, bounds, CALLOUT_WIDTH);

  const topY = 12;
  const botY = Math.max(topY + 60, bounds.height - 180);

  const positioned: PositionedCallout[] = [];
  for (let i = 0; i < tops.length; i++) {
    const a = tops[i];
    const candle = candles.time[a.idx];
    if (candle == null) continue;
    const ax = xForIndex(a.idx, count, bounds);
    const ay = yForPrice(candles.high[a.idx] ?? range.max, range, bounds);
    positioned.push({ ann: a, cx: topXs[i] ?? bounds.padLeft, cy: topY, ax, ay });
  }
  for (let i = 0; i < bots.length; i++) {
    const a = bots[i];
    const candle = candles.time[a.idx];
    if (candle == null) continue;
    const ax = xForIndex(a.idx, count, bounds);
    const ay = yForPrice(candles.low[a.idx] ?? range.min, range, bounds);
    positioned.push({ ann: a, cx: botXs[i] ?? bounds.padLeft, cy: botY, ax, ay });
  }

  return (
    <div
      ref={hostRef}
      className="absolute inset-0 pointer-events-none"
      data-testid="annotation-overlay-host"
    >
      <svg
        className="absolute inset-0 w-full h-full"
        style={{ pointerEvents: "none" }}
        aria-hidden="true"
      >
        {positioned.map((p) => {
          if (!Number.isFinite(p.ax) || !Number.isFinite(p.ay)) return null;
          const accent = p.ann.danger ? "rgba(200,68,58,0.85)" : "rgba(212,165,71,0.85)";
          // Connector terminates at the nearest corner of the callout
          // card: bottom-center for top-row callouts, top-center for
          // bottom-row callouts.
          const cardCenterX = p.cx + CALLOUT_WIDTH / 2;
          const cardEdgeY =
            p.ann.side === "top" ? p.cy + 90 /* approx card height */ : p.cy;
          return (
            <g key={`conn-${p.ann.idx}`}>
              <line
                x1={cardCenterX}
                y1={cardEdgeY}
                x2={p.ax}
                y2={p.ay}
                stroke={accent}
                strokeDasharray="3 3"
                strokeWidth={1}
              />
              <circle cx={p.ax} cy={p.ay} r={6} fill="none" stroke={accent} strokeOpacity={0.55} />
              <circle cx={p.ax} cy={p.ay} r={2.4} fill={accent} />
            </g>
          );
        })}
      </svg>

      {positioned.map((p) => (
        <div
          key={`card-${p.ann.idx}`}
          className="absolute pointer-events-auto"
          style={{ left: p.cx, top: p.cy }}
        >
          <Callout annotation={p.ann} />
        </div>
      ))}
    </div>
  );
}
