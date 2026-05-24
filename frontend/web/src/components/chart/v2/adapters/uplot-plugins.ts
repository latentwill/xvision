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

type DrawHookPlugin = { hooks: { draw: (u: uPlot) => void } };

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
  const bg = opts.backgroundFill ?? "#0F0E0C";
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
          if (!started) {
            ctx.moveTo(x, y);
            firstX = x;
            started = true;
          } else {
            ctx.lineTo(x, y);
          }
          lastX = x;
        }
        if (!started || firstX === null || lastX === null) {
          ctx.restore();
          return;
        }
        const zeroY = u.valToPos(baseline, "y", true);
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
    { offset: 0.0, color: "rgba(212,165,71,0.42)" },
    { offset: 0.25, color: "rgba(229,184,106,0.30)" },
    { offset: 0.55, color: "rgba(193,106,58,0.18)" },
    { offset: 0.85, color: "rgba(140,74,46,0.06)" },
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
          if (!started) {
            ctx.moveTo(x, y);
            firstX = x;
            started = true;
          } else {
            ctx.lineTo(x, y);
          }
          lastX = x;
        }
        if (!started || firstX === null || lastX === null) {
          ctx.restore();
          return;
        }
        const zeroY = u.valToPos(baseline, "y", true);
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
