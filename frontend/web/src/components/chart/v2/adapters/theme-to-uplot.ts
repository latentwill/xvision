import { type Chart2ThemeDefinition } from "../../../../theme/themes";

/**
 * Pane keys that have a known stroke color in Chart2ThemeDefinition.panes.
 */
type KnownPaneKey =
  | "equity"
  | "drawdown"
  | "rsi"
  | "macdLine"
  | "macdSignal"
  | "macdHist"
  | "atr"
  | "volumeUp"
  | "volumeDown";

/**
 * Return a partial uPlot options object with shared chrome defaults.
 * Returned as Record<string, unknown> — spread or merge with chart-specific
 * options before passing to `new uPlot(opts, data, el)`.
 *
 * Note: uPlot x-axis uses seconds (ms multiplier 1e-3) by default.
 */
export function themeToUplotOptions(t: Chart2ThemeDefinition): Record<string, unknown> {
  return {
    axes: [
      {
        stroke: t.surface.axisText,
        grid: { stroke: t.surface.gridSoft, width: 1 },
        ticks: { stroke: t.surface.axisTick },
        font: t.density.axisFont,
      },
      {
        stroke: t.surface.axisText,
        grid: { stroke: t.surface.gridSoft, width: 1 },
        ticks: { stroke: t.surface.axisTick },
        font: t.density.axisFont,
      },
    ],
    cursor: {
      points: { size: 6 },
      drag: { x: true, y: false },
    },
  };
}

/**
 * Return the correct stroke color for a named pane series key.
 * Returns `undefined` for unknown keys — callers should fall back to a default.
 */
export function paneSeriesStroke(
  t: Chart2ThemeDefinition,
  key: KnownPaneKey | string,
): string | undefined {
  const panes = t.panes;
  switch (key as KnownPaneKey) {
    case "equity":
      return panes.equity;
    case "drawdown":
      return panes.drawdown;
    case "rsi":
      return panes.rsi;
    case "macdLine":
      return panes.macdLine;
    case "macdSignal":
      return panes.macdSignal;
    case "macdHist":
      return panes.macdHist;
    case "atr":
      return panes.atr;
    case "volumeUp":
      return panes.volumeUp;
    case "volumeDown":
      return panes.volumeDown;
    default:
      return undefined;
  }
}
