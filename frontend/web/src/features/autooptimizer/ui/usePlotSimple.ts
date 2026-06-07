/**
 * usePlotSimple — lightweight uPlot lifecycle hook for the autooptimizer charts.
 *
 * Unlike the chart-v2 `usePlot`, this hook does NOT wire up the PaneStack
 * sync-cursor events or the chart-frame range/zoom bus. It is intentionally
 * minimal so the optimizer feature charts stay independent of the chart-v2
 * subsystem.
 */
import { useEffect, useRef } from "react";
import uPlot from "uplot";

export function usePlotSimple(
  opts: uPlot.Options,
  data: uPlot.AlignedData,
  hostRef: React.RefObject<HTMLDivElement | null>,
  height: number,
  enabled = true,
): void {
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!enabled) return;

    const host = hostRef.current;
    if (!host) return;

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
      console.warn("[usePlotSimple] uPlot constructor threw:", err);
      return;
    }
    plotRef.current = plot;

    const obs = new ResizeObserver(() => {
      if (plotRef.current && host.clientWidth > 0) {
        plotRef.current.setSize({ width: host.clientWidth, height });
      }
    });
    obs.observe(host);

    return () => {
      obs.disconnect();
      if (plotRef.current) {
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(opts), data, height, enabled]);
}
