/**
 * usePlot — shared hook that owns the uPlot instance lifecycle.
 *
 * Destroy + recreate on any opts/data change is acceptable for M0 (see comment
 * in caller files). The hook returns nothing — side-effects only. All
 * interaction with the uPlot instance is handled internally here.
 */
import { useEffect, useRef } from "react";
import uPlot from "uplot";

/**
 * Constructs, owns, and destroys a uPlot instance.
 *
 * @param opts    uPlot.Options (width/height come from ResizeObserver; pass 0 as
 *                initial placeholder — the observer fires immediately on mount).
 * @param data    AlignedData fed to the constructor.
 * @param hostRef Ref to the container div that uPlot should render into.
 * @param height  Desired height in CSS pixels.
 *
 * NOTE: We deliberately stringify opts in the dep array (M0 approach). The
 * alternative — tracking structural identity across renders — is M1 work.
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

    return () => {
      obs.disconnect();
      if (plotRef.current) {
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(opts), data, height]);
}
