// uPlot draw-hook plugins used by the Charts dashboard surfaces
// (chart-rework Track B). Ports of the handoff's `chart-helpers.js`
// helpers into typed uPlot plugins.
//
// Plugins return `{ hooks: { draw } }` objects shaped the way uPlot
// expects. Callers attach them to the `plugins:` array on their uPlot
// options.
//
// Reference: docs/design/trading-charts/XVN.zip
//   → design_handoff_charts/source/charts/chart-helpers.js
//     (`xvnLastDot`, `xvnAreaFill`, `xvnRegimeBands` are direct ports;
//      `xvnGradientFill` and `xvnSheen` are Track-B-4 additions for
//      the hero gradient variant).

import type uPlot from "uplot";
import type { V2Marker } from "../types";

type DrawHookPlugin = { hooks: { draw: (u: uPlot) => void } };

/**
 * True when every argument is a finite number. Canvas gradient + path APIs
 * throw (`createLinearGradient: non-finite`) or silently corrupt the draw
 * when fed NaN/Infinity — which happens with empty data, zero-height bboxes,
 * or un-ranged scales (audit F8). Every plugin below guards through this.
 */
export function allFinite(...values: number[]): boolean {
  for (const v of values) {
    if (!Number.isFinite(v)) return false;
  }
  return true;
}

/**
 * Halo + solid dot on the LAST data point of a series. Used to mark
 * the "where we are now" position on the lead equity curve.
 */
export function xvnLastDot(
  seriesIdx: number,
  color: string,
  opts: { halo?: boolean; radius?: number; backgroundFill?: string } = {},
): DrawHookPlugin {
  const radius = opts.radius ?? 3.2;
  const drawHalo = opts.halo ?? true;
  const bg = opts.backgroundFill ?? "#000000";
  return {
    hooks: {
      draw: (u) => {
        const xs = u.data[0];
        const ys = u.data[seriesIdx];
        if (!xs || !ys || xs.length === 0) return;
        // Find the last numeric sample (uPlot allows nulls for gaps).
        let lastIdx = -1;
        for (let i = xs.length - 1; i >= 0; i--) {
          if (ys[i] != null && Number.isFinite(ys[i] as number)) {
            lastIdx = i;
            break;
          }
        }
        if (lastIdx < 0) return;
        const x = u.valToPos(xs[lastIdx] as number, "x", true);
        const y = u.valToPos(ys[lastIdx] as number, "y", true);
        if (!allFinite(x, y)) return;
        const ctx = u.ctx;
        ctx.save();
        if (drawHalo) {
          ctx.beginPath();
          ctx.arc(x, y, radius * 2.2, 0, Math.PI * 2);
          ctx.fillStyle = color + "22";
          ctx.fill();
        }
        ctx.beginPath();
        ctx.arc(x, y, radius, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.strokeStyle = bg;
        ctx.lineWidth = 1.2;
        ctx.stroke();
        ctx.restore();
      },
    },
  };
}

/**
 * Single-color gradient area fill below a series, fading to transparent
 * at the baseline. Baseline defaults to `0` so equity / drawdown curves
 * fill toward the zero line.
 */
export function xvnAreaFill(
  seriesIdx: number,
  topColor: string,
  opts: { bottomAlpha?: number; baseline?: number } = {},
): DrawHookPlugin {
  const bottomAlpha = opts.bottomAlpha ?? 0;
  const baseline = opts.baseline ?? 0;
  return {
    hooks: {
      draw: (u) => {
        const xs = u.data[0];
        const ys = u.data[seriesIdx];
        if (!xs || !ys || xs.length === 0) return;
        const ctx = u.ctx;
        const { top, height, left, width } = u.bbox;
        if (!allFinite(top, height, left, width) || height <= 0) return;
        ctx.save();
        ctx.beginPath();
        let started = false;
        let firstX: number | null = null;
        let lastX: number | null = null;
        for (let i = 0; i < xs.length; i++) {
          const v = ys[i];
          if (v == null || !Number.isFinite(v as number)) continue;
          const x = u.valToPos(xs[i] as number, "x", true);
          const y = u.valToPos(v as number, "y", true);
          if (!allFinite(x, y)) continue;
          if (!started) {
            ctx.moveTo(x, y);
            firstX = x;
            started = true;
          } else {
            ctx.lineTo(x, y);
          }
          lastX = x;
        }
        const zeroY = u.valToPos(baseline, "y", true);
        if (!started || firstX === null || lastX === null || !allFinite(zeroY)) {
          ctx.restore();
          return;
        }
        ctx.lineTo(lastX, zeroY);
        ctx.lineTo(firstX, zeroY);
        ctx.closePath();
        const grad = ctx.createLinearGradient(0, top, 0, top + height);
        grad.addColorStop(0, topColor);
        grad.addColorStop(1, `rgba(0,0,0,${bottomAlpha})`);
        // Clip to the plot bbox so the fill doesn't bleed into axes.
        ctx.save();
        ctx.beginPath();
        ctx.rect(left, top, width, height);
        ctx.clip();
        ctx.fillStyle = grad;
        ctx.fill();
        ctx.restore();
        ctx.restore();
      },
    },
  };
}

/**
 * Vertical bands behind the series, one per regime span. Each band:
 * `{ x0, x1, fill, label? }`. Drawn under the lines via the
 * `drawClear` phase so the chart geometry sits on top.
 */
export function xvnRegimeBands(
  bands: Array<{ x0: number; x1: number; fill: string; label?: string }>,
): { hooks: { drawClear: (u: uPlot) => void } } {
  return {
    hooks: {
      drawClear: (u) => {
        const ctx = u.ctx;
        const { top, height } = u.bbox;
        ctx.save();
        for (const b of bands) {
          const x0 = u.valToPos(b.x0, "x", true);
          const x1 = u.valToPos(b.x1, "x", true);
          ctx.fillStyle = b.fill;
          ctx.fillRect(x0, top, Math.max(1, x1 - x0), height);
        }
        ctx.restore();
      },
    },
  };
}

/**
 * Multi-stop warm gradient fill — used by the B4 GradientHeroDashboard
 * `HeroGradientEquity` pane. Stops default to the handoff's 5-step
 * gold → amber → ember → copper → fade-to-zero ramp.
 */
export function xvnGradientFill(
  seriesIdx: number,
  opts: { stops?: Array<{ offset: number; color: string }>; baseline?: number } = {},
): DrawHookPlugin {
  const stops = opts.stops ?? [
    { offset: 0.0, color: "rgba(0,230,118,0.42)" },
    { offset: 0.25, color: "rgba(94,234,212,0.30)" },
    { offset: 0.55, color: "rgba(0,184,95,0.18)" },
    { offset: 0.85, color: "rgba(0,120,60,0.06)" },
    { offset: 1.0, color: "rgba(0,0,0,0)" },
  ];
  const baseline = opts.baseline ?? 0;
  return {
    hooks: {
      draw: (u) => {
        const xs = u.data[0];
        const ys = u.data[seriesIdx];
        if (!xs || !ys || xs.length === 0) return;
        const ctx = u.ctx;
        const { top, height, left, width } = u.bbox;
        if (!allFinite(top, height, left, width) || height <= 0) return;
        ctx.save();
        ctx.beginPath();
        let started = false;
        let firstX: number | null = null;
        let lastX: number | null = null;
        for (let i = 0; i < xs.length; i++) {
          const v = ys[i];
          if (v == null || !Number.isFinite(v as number)) continue;
          const x = u.valToPos(xs[i] as number, "x", true);
          const y = u.valToPos(v as number, "y", true);
          if (!allFinite(x, y)) continue;
          if (!started) {
            ctx.moveTo(x, y);
            firstX = x;
            started = true;
          } else {
            ctx.lineTo(x, y);
          }
          lastX = x;
        }
        const zeroY = u.valToPos(baseline, "y", true);
        if (!started || firstX === null || lastX === null || !allFinite(zeroY)) {
          ctx.restore();
          return;
        }
        ctx.lineTo(lastX, zeroY);
        ctx.lineTo(firstX, zeroY);
        ctx.closePath();
        const grad = ctx.createLinearGradient(0, top, 0, top + height);
        for (const s of stops) grad.addColorStop(s.offset, s.color);
        ctx.save();
        ctx.beginPath();
        ctx.rect(left, top, width, height);
        ctx.clip();
        ctx.fillStyle = grad;
        ctx.fill();
        ctx.restore();
        ctx.restore();
      },
    },
  };
}

/**
 * Green-above/red-below gradient fill split at y=0. Used as `series.fill`
 * callback on the return-% equity pane.
 */
export function buildReturnFillGradient(u: uPlot): CanvasGradient | string {
  const yMax = u.scales.y.max ?? 10;
  const yMin = u.scales.y.min ?? -10;
  const top = u.valToPos(yMax, "y", true);
  const bot = u.valToPos(yMin, "y", true);
  const zero = u.valToPos(0, "y", true);
  // F8 guard: empty data / un-ranged scales yield NaN positions, and
  // `createLinearGradient` throws on non-finite args. Fall back to a
  // transparent fill instead of crashing the draw pass.
  if (!allFinite(top, bot, zero)) {
    return "rgba(0,0,0,0)";
  }
  const grad = u.ctx.createLinearGradient(0, top, 0, bot);
  const span = bot - top;
  if (span <= 0) {
    grad.addColorStop(0, "rgba(0,230,118,0.25)");
    grad.addColorStop(1, "rgba(0,230,118,0)");
    return grad;
  }
  const zf = Math.max(0, Math.min(1, (zero - top) / span));
  const eps = 1 / Math.max(1, span);
  grad.addColorStop(0, "rgba(0,230,118,0.25)");
  grad.addColorStop(zf, "rgba(0,230,118,0)");
  grad.addColorStop(Math.min(1, zf + eps), "rgba(239,68,68,0)");
  grad.addColorStop(1, "rgba(239,68,68,0.20)");
  return grad;
}

/**
 * Red underwater gradient for drawdown panes: strongest at the surface
 * (the y-scale ceiling, normally 0) fading toward the deepest drawdown.
 * Used as the `series.fill` callback on `UplotDrawdownPane` - the depth
 * counterpart to `buildReturnFillGradient`.
 */
export function buildDrawdownFillGradient(u: uPlot): CanvasGradient | string {
  const yMax = u.scales.y.max ?? 0;
  const yMin = u.scales.y.min ?? -10;
  const top = u.valToPos(yMax, "y", true);
  const bot = u.valToPos(yMin, "y", true);
  // F8 guard: empty data / un-ranged scales yield NaN positions, and
  // `createLinearGradient` throws on non-finite args. Fall back to a
  // transparent fill instead of crashing the draw pass.
  if (!allFinite(top, bot) || bot - top <= 0) {
    return "rgba(0,0,0,0)";
  }
  const grad = u.ctx.createLinearGradient(0, top, 0, bot);
  grad.addColorStop(0, "rgba(255,77,77,0.32)");
  grad.addColorStop(0.6, "rgba(255,77,77,0.10)");
  grad.addColorStop(1, "rgba(255,77,77,0.02)");
  return grad;
}

/**
 * Dashed zero-baseline drawn across the plot area. Visible only when the
 * y-scale spans zero (positive + negative returns both present).
 */
export function xvnZeroLine(): DrawHookPlugin {
  return {
    hooks: {
      draw: (u) => {
        const { min: yMin, max: yMax } = u.scales.y;
        if (yMin == null || yMax == null || yMin > 0 || yMax < 0) return;
        const y = u.valToPos(0, "y", true);
        if (!allFinite(y, u.bbox.left, u.bbox.width)) return;
        const ctx = u.ctx;
        ctx.save();
        ctx.strokeStyle = "rgba(255,255,255,0.20)";
        ctx.lineWidth = 1;
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.moveTo(u.bbox.left, y);
        ctx.lineTo(u.bbox.left + u.bbox.width, y);
        ctx.stroke();
        ctx.restore();
      },
    },
  };
}

/**
 * On-chain buy/sell trade markers drawn onto the equity pane.
 * Modeled line-for-line on `xvnLastDot` — same `u.valToPos` / `allFinite`
 * guard pattern. Returns `{ hooks: { draw } }` shaped for uPlot's plugins array.
 *
 * - buy  markers: gold upward   triangle ▲ (default #00e676)
 * - sell markers: red  downward triangle ▼ (default #ff4d4d)
 *
 * Markers whose `time` falls outside `u.scales.x.min/max` are skipped so
 * the canvas does not bleed outside the chart's x range.
 */
export function xvnTradeMarkers(
  markers: V2Marker[],
  opts: {
    buyColor?: string;
    sellColor?: string;
    /** "triangle" (default, candle panes) or "letter" (B/S text, equity line). */
    glyph?: "triangle" | "letter";
    /** Ignore marker.price and anchor to the series value at the marker's
     *  time. Required on the %-return equity line, where marker.price (an
     *  absolute fill price) is off-scale. */
    anchorToSeries?: boolean;
  } = {},
): DrawHookPlugin {
  const buyColor = opts.buyColor ?? "#00e676";
  const sellColor = opts.sellColor ?? "#ff4d4d";
  const glyph = opts.glyph ?? "triangle";
  const anchorToSeries = opts.anchorToSeries ?? false;
  const SIZE = 6; // half-size of triangle in pixels

  return {
    hooks: {
      draw: (u) => {
        if (!markers || markers.length === 0) return;
        const ctx = u.ctx;
        const xMin = u.scales.x.min;
        const xMax = u.scales.x.max;

        ctx.save();
        for (const m of markers) {
          // Skip markers outside the visible x range.
          if (xMin != null && m.time < xMin) continue;
          if (xMax != null && m.time > xMax) continue;

          // Determine y: use marker.price when available, else look up series value.
          // anchorToSeries forces the series-value lookup (price is off-scale on
          // the %-return equity line).
          let yVal: number | null | undefined;
          if (!anchorToSeries && m.price != null && Number.isFinite(m.price)) {
            yVal = m.price;
          } else {
            // Fall back to the first series (index 1) value at the nearest time point.
            const times = u.data[0];
            const vals = u.data[1];
            if (times && vals) {
              let bestIdx = -1;
              let bestDiff = Infinity;
              for (let i = 0; i < times.length; i++) {
                const diff = Math.abs((times[i] as number) - m.time);
                if (diff < bestDiff) { bestDiff = diff; bestIdx = i; }
              }
              if (bestIdx >= 0) yVal = vals[bestIdx] as number;
            }
          }
          if (yVal == null || !Number.isFinite(yVal)) continue;

          const x = u.valToPos(m.time, "x", true);
          const y = u.valToPos(yVal, "y", true);
          if (!allFinite(x, y)) continue;

          const isBuy = m.kind === "buy";
          ctx.fillStyle = isBuy ? buyColor : sellColor;

          if (glyph === "letter") {
            // Bold "B"/"S" label: buys above the curve point, sells below, so
            // the letter clears the equity line.
            ctx.font = "700 10px ui-monospace, SFMono-Regular, Menlo, monospace";
            ctx.textAlign = "center";
            ctx.textBaseline = "middle";
            const ty = isBuy ? y - SIZE - 1 : y + SIZE + 1;
            ctx.fillText(isBuy ? "B" : "S", x, ty);
            continue;
          }

          ctx.beginPath();
          if (isBuy) {
            // Upward triangle ▲
            ctx.moveTo(x, y - SIZE);
            ctx.lineTo(x + SIZE, y + SIZE);
            ctx.lineTo(x - SIZE, y + SIZE);
          } else {
            // Downward triangle ▼ (sell / close)
            ctx.moveTo(x, y + SIZE);
            ctx.lineTo(x + SIZE, y - SIZE);
            ctx.lineTo(x - SIZE, y - SIZE);
          }
          ctx.closePath();
          ctx.fill();
        }
        ctx.restore();
      },
    },
  };
}

/**
 * Horizontal sheen highlight in the top band of the plot — adds a
 * subtle "glass on curve" effect to the B4 hero equity pane.
 */
export function xvnSheen(
  opts: { topFraction?: number; color?: string } = {},
): DrawHookPlugin {
  const topFraction = opts.topFraction ?? 0.4;
  const color = opts.color ?? "rgba(241,236,221,0.06)";
  return {
    hooks: {
      draw: (u) => {
        const ctx = u.ctx;
        const { top, height, left, width } = u.bbox;
        if (!allFinite(top, height, left, width) || height <= 0) return;
        const sheenH = Math.max(1, height * topFraction);
        ctx.save();
        const grad = ctx.createLinearGradient(0, top, 0, top + sheenH);
        grad.addColorStop(0, color);
        grad.addColorStop(1, "rgba(0,0,0,0)");
        ctx.fillStyle = grad;
        ctx.fillRect(left, top, width, sheenH);
        ctx.restore();
      },
    },
  };
}
