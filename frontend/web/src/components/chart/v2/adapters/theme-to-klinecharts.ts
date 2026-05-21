import { type Chart2ThemeDefinition } from "../../../../theme/themes";

/**
 * Map a Chart2ThemeDefinition to a KlineCharts styles object.
 * Returned as Record<string, unknown> to avoid importing the deep KlineCharts
 * style types — pass straight to chart.setStyles().
 *
 * Reference: https://klinecharts.com/en-US/guide/styles.html
 */
export function themeToKlinechartsStyles(
  t: Chart2ThemeDefinition,
): Record<string, unknown> {
  return {
    grid: {
      horizontal: { color: t.surface.gridSoft },
      vertical: { color: t.surface.gridSoft },
    },
    candle: {
      bar: {
        upColor: t.candle.up,
        downColor: t.candle.down,
        upBorderColor: t.candle.borderUp,
        downBorderColor: t.candle.borderDown,
        upWickColor: t.candle.wickUp,
        downWickColor: t.candle.wickDown,
      },
      tooltip: {
        text: { color: t.surface.axisText },
      },
      priceMark: {
        last: {
          upColor: t.candle.up,
          downColor: t.candle.down,
        },
      },
    },
    indicator: {
      lines: [
        { color: t.overlay.sma20 },
        { color: t.overlay.sma50 },
        { color: t.overlay.sma200 },
        { color: t.overlay.ema20 },
      ],
    },
    xAxis: {
      axisLine: { color: t.surface.axisTick },
      tickText: { color: t.surface.axisText },
    },
    yAxis: {
      axisLine: { color: t.surface.axisTick },
      tickText: { color: t.surface.axisText },
    },
    crosshair: {
      horizontal: { line: { color: t.surface.crosshair } },
      vertical: { line: { color: t.surface.crosshair } },
    },
  };
}
