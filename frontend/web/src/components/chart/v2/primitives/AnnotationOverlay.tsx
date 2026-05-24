/**
 * AnnotationOverlay — absolutely-positioned layer above the
 * KlineCandlePane. Renders two rows of Callouts (top + bottom) with
 * SVG connectors (dashed 3 3) pointing from the candle anchor
 * (r=6 ring + r=2.4 dot) to the nearest callout corner.
 *
 * Two anchoring modes:
 *
 * **Pixel-precise (default when `chart` prop is provided):** uses
 * `createKlineAnchor(chart)` which calls `chart.convertToPixel` for
 * each index/price and subscribes to `onVisibleRangeChange` so the
 * overlay re-anchors on every pan, zoom, or resize.
 *
 * **Geometric fallback (when no `chart` prop):** the original
 * `xForIndex` / `yForPrice` approximation, driven by a ResizeObserver
 * on the overlay host. Used for chart-lab fixture renders before the
 * klinecharts instance mounts.
 */
import { useCallback, useEffect, useRef, useState, type ReactElement } from "react";
import type { Chart } from "klinecharts";

import type { Annotation, CandleColumns } from "../types";
import {
  createKlineAnchor,
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
  /**
   * Live klinecharts Chart instance. When provided, the overlay uses
   * `createKlineAnchor` for pixel-precise anchoring and re-anchors on
   * pan/zoom. When absent, falls back to the geometric approximation.
   */
  chart?: Chart | null;
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
  chart,
}: AnnotationOverlayProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  // `tick` increments whenever the layout changes so the component re-renders
  // with fresh pixel positions from the chart instance.
  const [tick, setTick] = useState(0);
  const bumpTick = useCallback(() => setTick((n) => n + 1), []);

  // Geometric-fallback state: track host bounds via ResizeObserver.
  const [bounds, setBounds] = useState<AnchorBounds>(DEFAULT_BOUNDS);

  // ── Pixel-precise mode: subscribe to layout changes when chart is live ──
  useEffect(() => {
    if (!chart) return;
    const anchor = createKlineAnchor(chart);
    const unsub = anchor.subscribeLayout(bumpTick);
    // Fire once to prime pixel positions.
    bumpTick();
    return unsub;
  }, [chart, bumpTick]);

  // ── Geometric fallback: ResizeObserver on the host div ─────────────────
  useEffect(() => {
    if (chart) return; // pixel-precise mode handles resize via subscribeLayout
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
  }, [chart]);

  const filtered = visibleTypes
    ? annotations.filter((a) => visibleTypes.has(a.type))
    : annotations;

  const count = candles.time.length;
  const range = deriveRange(candles.high, candles.low);

  const tops = filtered.filter((a) => a.side === "top");
  const bots = filtered.filter((a) => a.side === "bottom");

  // When in pixel-precise mode, the host bounds for callout spreading come
  // from the chart's DOM element; fall back to ResizeObserver-tracked bounds.
  const activeBounds: AnchorBounds = (() => {
    if (chart) {
      try {
        const el = chart.getDom();
        if (el) {
          const rect = el.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            return {
              ...DEFAULT_BOUNDS,
              width: rect.width,
              height: rect.height,
            };
          }
        }
      } catch {
        // chart disposed between render calls
      }
    }
    return bounds;
  })();

  // Suppress tick usage from lint — it's only read to trigger re-renders.
  void tick;

  const topXs = spreadRowXs(tops.length, activeBounds, CALLOUT_WIDTH);
  const botXs = spreadRowXs(bots.length, activeBounds, CALLOUT_WIDTH);

  const topY = 12;
  const botY = Math.max(topY + 60, activeBounds.height - 180);

  // Resolve pixel position for a single anchor point, using the chart
  // instance when available, falling back to geometric helpers.
  function resolveAnchor(dataIndex: number, price: number): { ax: number; ay: number } {
    if (chart) {
      try {
        const klineAnchor = createKlineAnchor(chart);
        return { ax: klineAnchor.xForIndex(dataIndex), ay: klineAnchor.yForPrice(price) };
      } catch {
        // fall through to geometric
      }
    }
    return {
      ax: xForIndex(dataIndex, count, activeBounds),
      ay: yForPrice(price, range, activeBounds),
    };
  }

  const positioned: PositionedCallout[] = [];
  for (let i = 0; i < tops.length; i++) {
    const a = tops[i];
    const candle = candles.time[a.idx];
    if (candle == null) continue;
    const { ax, ay } = resolveAnchor(a.idx, candles.high[a.idx] ?? range.max);
    positioned.push({ ann: a, cx: topXs[i] ?? activeBounds.padLeft, cy: topY, ax, ay });
  }
  for (let i = 0; i < bots.length; i++) {
    const a = bots[i];
    const candle = candles.time[a.idx];
    if (candle == null) continue;
    const { ax, ay } = resolveAnchor(a.idx, candles.low[a.idx] ?? range.min);
    positioned.push({ ann: a, cx: botXs[i] ?? activeBounds.padLeft, cy: botY, ax, ay });
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
          const accent = p.ann.danger ? "rgba(255,77,77,0.85)" : "rgba(0,230,118,0.85)";
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
