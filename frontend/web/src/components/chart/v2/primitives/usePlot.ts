/**
 * usePlot — shared hook that owns the uPlot instance lifecycle.
 *
 * Destroy + recreate on any opts/data change is acceptable for M0 (see comment
 * in caller files). The hook returns nothing — side-effects only. All
 * interaction with the uPlot instance is handled internally here.
 */
import { useEffect, useRef } from "react";
import uPlot from "uplot";
import { CHART_V2_RANGE_EVENT, CHART_V2_ZOOM_EVENT } from "./ChartFrame";
import { rangeWindowSeconds } from "./range-window";

/**
 * Produce a stable string key that captures both the JSON-serialisable
 * structure of opts AND any function-valued fields (e.g. `axes[].values`)
 * that `JSON.stringify` silently drops.
 *
 * Strategy:
 * 1. Walk `opts.axes` and collect `.values` function sources (`.toString()`).
 *    Other function-valued fields follow the same pattern if needed.
 * 2. Stringify the rest of opts normally.
 * 3. Concatenate so any change to either layer busts the dep.
 *
 * This is intentionally conservative — it extracts only the axes formatter
 * functions that are the known source of the "00.0%" stale-tick bug (W14).
 * Adding more function fields is a one-line extension below.
 */
function optsKey(opts: uPlot.Options): string {
  const axisValueSources = (opts.axes ?? [])
    .map((ax) => (typeof ax.values === "function" ? ax.values.toString() : ""))
    .join("|");
  return JSON.stringify(opts) + "||fn:" + axisValueSources;
}

/**
 * Constructs, owns, and destroys a uPlot instance.
 *
 * @param opts    uPlot.Options (width/height come from ResizeObserver; pass 0 as
 *                initial placeholder — the observer fires immediately on mount).
 * @param data    AlignedData fed to the constructor.
 * @param hostRef Ref to the container div that uPlot should render into.
 * @param height  Desired height in CSS pixels.
 *
 * Dep strategy: we use `optsKey(opts)` instead of bare `JSON.stringify(opts)`
 * because JSON.stringify silently drops function-valued fields (e.g.
 * `axes[].values` formatters). `optsKey` appends the `.toString()` of known
 * function-valued opts so a changed formatter triggers a plot recreation.
 * `data` is included directly so new data always recreates the plot.
 */
export function usePlot(
  opts: uPlot.Options,
  data: uPlot.AlignedData,
  hostRef: React.RefObject<HTMLDivElement | null>,
  height: number,
): void {
  // Keep a stable ref to the plot so the ResizeObserver callback always
  // sees the current instance without re-running the effect.
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    // Destroy any previous instance before creating a new one.
    if (plotRef.current) {
      plotRef.current.destroy();
      plotRef.current = null;
    }

    const finalOpts: uPlot.Options = {
      ...opts,
      width: host.clientWidth || 300,
      height,
    };

    let plot: uPlot;
    try {
      plot = new uPlot(finalOpts, data, host);
    } catch (err) {
      console.warn("[usePlot] uPlot constructor threw:", err);
      return;
    }
    plotRef.current = plot;

    const obs = new ResizeObserver(() => {
      if (plotRef.current && host.clientWidth > 0) {
        plotRef.current.setSize({ width: host.clientWidth, height });
      }
    });
    obs.observe(host);

    const over = host.querySelector(".u-over");
    const onWheel = (event: Event) => {
      const wheel = event as WheelEvent;
      if (!plotRef.current) return;
      if (wheel.deltaY === 0 && wheel.deltaX === 0) return;
      wheel.preventDefault();
      if (wheel.ctrlKey || wheel.metaKey) {
        zoomPlot(plotRef.current, wheel.deltaY > 0 ? "out" : "in");
        return;
      }
      panPlot(plotRef.current, wheel.deltaX + wheel.deltaY);
    };
    over?.addEventListener("wheel", onWheel, { passive: false });

    const onZoom = (event: Event) => {
      const detail = (event as CustomEvent<"in" | "out">).detail;
      if (plotRef.current && (detail === "in" || detail === "out")) {
        zoomPlot(plotRef.current, detail);
      }
    };
    window.addEventListener(CHART_V2_ZOOM_EVENT, onZoom);

    const onRange = (event: Event) => {
      const preset = (event as CustomEvent).detail;
      const plot = plotRef.current;
      if (!plot) return;
      const xs = plot.data[0] as number[] | undefined;
      if (!xs || xs.length === 0) return;
      const maxX = xs[xs.length - 1];
      const win = rangeWindowSeconds(preset);
      if (win == null) {
        plot.setScale("x", { min: xs[0], max: maxX });
        return;
      }
      plot.setScale("x", { min: maxX - win, max: maxX });
    };
    window.addEventListener(CHART_V2_RANGE_EVENT, onRange);

    return () => {
      obs.disconnect();
      over?.removeEventListener("wheel", onWheel);
      window.removeEventListener(CHART_V2_ZOOM_EVENT, onZoom);
      window.removeEventListener(CHART_V2_RANGE_EVENT, onRange);
      if (plotRef.current) {
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [optsKey(opts), data, height]);
}

function zoomPlot(plot: uPlot, direction: "in" | "out") {
  const range = currentXRange(plot);
  if (!range) return;
  const [fullMin, fullMax, min, max] = range;
  const span = max - min;
  if (span <= 0) return;
  const center = min + span / 2;
  const factor = direction === "in" ? 0.82 : 1.22;
  const nextSpan = Math.min(fullMax - fullMin, Math.max(span * factor, 1));
  const nextMin = center - nextSpan / 2;
  setClampedXRange(plot, fullMin, fullMax, nextMin, nextMin + nextSpan);
}

function panPlot(plot: uPlot, delta: number) {
  const range = currentXRange(plot);
  if (!range) return;
  const [fullMin, fullMax, min, max] = range;
  const span = max - min;
  if (span <= 0) return;
  const shift = span * delta * 0.0012;
  setClampedXRange(plot, fullMin, fullMax, min + shift, max + shift);
}

function currentXRange(plot: uPlot): [number, number, number, number] | null {
  const xs = plot.data[0] as number[] | undefined;
  if (!xs || xs.length < 2) return null;
  const fullMin = xs[0];
  const fullMax = xs[xs.length - 1];
  const min = typeof plot.scales.x.min === "number" ? plot.scales.x.min : fullMin;
  const max = typeof plot.scales.x.max === "number" ? plot.scales.x.max : fullMax;
  return [fullMin, fullMax, min, max];
}

function setClampedXRange(
  plot: uPlot,
  fullMin: number,
  fullMax: number,
  min: number,
  max: number,
) {
  const fullSpan = fullMax - fullMin;
  const span = Math.min(fullSpan, max - min);
  let nextMin = min;
  let nextMax = max;
  if (nextMin < fullMin) {
    nextMin = fullMin;
    nextMax = fullMin + span;
  }
  if (nextMax > fullMax) {
    nextMax = fullMax;
    nextMin = fullMax - span;
  }
  plot.setScale("x", { min: nextMin, max: nextMax });
}
