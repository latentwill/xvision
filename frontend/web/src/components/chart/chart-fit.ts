import type { IChartApi } from "lightweight-charts";

/**
 * Force a price-scale auto-fit on the given chart. F-5 from the
 * 2026-05-18 QA round-4 intake: `timeScale().fitContent()` only fits
 * the horizontal axis; the vertical scale stays at whatever range was
 * last computed. Re-asserting `autoScale: true` on every visible
 * price-scale id triggers lightweight-charts to recompute the price
 * range over the *currently visible* time window — so zooming into a
 * 10-bar slice re-fits the price axis to those 10 bars instead of
 * leaving it on the prior range.
 *
 * Charts in this app use different price-scale ids:
 *   - RunChart / StrategyChart / ScenarioChart: default `right` scale
 *     (plus `volume` on Scenario)
 *   - CompareChart: `left` scale (it overlays multiple price series)
 *
 * Pass the ids the caller cares about. Unknown ids return a no-op
 * priceScale handle, so passing a superset is safe but redundant.
 */
export function applyVerticalAutoScale(
  chart: IChartApi | null | undefined,
  priceScaleIds: ReadonlyArray<string> = ["right"],
): void {
  if (!chart) return;
  for (const id of priceScaleIds) {
    try {
      chart.priceScale(id).applyOptions({ autoScale: true });
    } catch {
      // priceScale() throws for unknown ids on some lightweight-charts
      // versions; treat as no-op so chart families that don't use an
      // id don't need to special-case the call site.
    }
  }
}

/**
 * Composite "fit both axes" — calls `timeScale().fitContent()` for the
 * time axis, then `applyVerticalAutoScale` for the price axis. Use
 * this in place of bare `chart.timeScale().fitContent()` to keep the
 * two axes in sync on initial render and on programmatic resets.
 */
export function fitChartContent(
  chart: IChartApi | null | undefined,
  priceScaleIds: ReadonlyArray<string> = ["right"],
): void {
  if (!chart) return;
  chart.timeScale().fitContent();
  applyVerticalAutoScale(chart, priceScaleIds);
}
