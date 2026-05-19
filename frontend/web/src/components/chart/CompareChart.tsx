import { useEffect, useRef, useState } from "react";
import { createChart, ColorType, CrosshairMode, type UTCTimestamp } from "lightweight-charts";
import type { CompareChartPayload } from "@/api/types.gen";
import type { ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { chartTheme, normalizeChartTheme } from "./chart-theme";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { applyVerticalAutoScale, fitChartContent } from "./chart-fit";

const RUN_COLORS = [
  "#22d3ee",
  "#a78bfa",
  "#34d399",
  "#fbbf24",
  "#f87171",
  "#60a5fa",
  "#fb923c",
  "#10b981",
  "#e879f9",
  "#f43f5e",
];

export function CompareChart({
  payload,
  theme,
  themeMode,
}: {
  payload: CompareChartPayload;
  themeMode?: "dark" | "light";
  theme?: ResolvedTheme;
}) {
  const appTheme = useTheme();
  const activeTheme = theme ?? normalizeChartTheme(themeMode, appTheme.resolvedTheme);
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>("All");
  const [showBackdrop, setShowBackdrop] = useState(false);

  useEffect(() => {
    if (!ref.current) return;
    const palette = chartTheme(activeTheme);
    const c = createChart(ref.current, {
      layout: {
        background: { type: ColorType.Solid, color: palette.background },
        textColor: palette.text,
      },
      grid: {
        vertLines: { color: palette.grid },
        horzLines: { color: palette.grid },
      },
      crosshair: { mode: CrosshairMode.Normal },
      timeScale: { rightOffset: 12, secondsVisible: false },
    });

    if (showBackdrop && payload.price_backdrop) {
      const bd = c.addCandlestickSeries({
        upColor: "#3f3f46",
        downColor: "#27272a",
        borderVisible: false,
        wickUpColor: "#52525b",
        wickDownColor: "#27272a",
        priceScaleId: "left",
      });
      bd.setData(
        payload.price_backdrop.map((b) => ({
          time: b.time as UTCTimestamp,
          open: b.open,
          high: b.high,
          low: b.low,
          close: b.close,
        })),
      );
    }

    payload.runs.forEach((r, i) => {
      const line = c.addLineSeries({
        color: RUN_COLORS[i % RUN_COLORS.length],
        lineWidth: 1,
        title: r.label,
      });
      line.setData(
        r.equity.map((p) => ({ time: p.time as UTCTimestamp, value: p.equity_usd })),
      );
    });

    applyRange(c, range, collectVisibleTimes(payload, showBackdrop));

    return () => c.remove();
  }, [payload, activeTheme, showBackdrop, range]);

  return (
    <ChartContainer
      range={range}
      onRange={setRange}
      layersPanel={
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            disabled={!payload.shared_scenario}
            checked={showBackdrop}
            onChange={(e) => setShowBackdrop(e.target.checked)}
          />
          Price backdrop{" "}
          {!payload.shared_scenario && (
            <span className="text-text-3">(runs span scenarios)</span>
          )}
        </label>
      }
    >
      <div ref={ref} style={{ height: 480 }} />
    </ChartContainer>
  );
}

function collectVisibleTimes(payload: CompareChartPayload, includeBackdrop: boolean) {
  const times = new Set<number>();
  payload.runs.forEach((run) => {
    run.equity.forEach((point) => times.add(point.time));
  });
  if (includeBackdrop && payload.price_backdrop) {
    payload.price_backdrop.forEach((bar) => times.add(bar.time));
  }
  return [...times].sort((a, b) => a - b);
}

function applyRange(
  chart: ReturnType<typeof createChart>,
  range: RangePreset,
  times: number[],
) {
  if (times.length <= 0) return;
  if (range === "All") {
    // CompareChart's price series live on the `left` scale (see the
    // backdrop `priceScaleId: "left"` config above); fitChartContent's
    // default `right` is a no-op for this chart, so pass the actual id.
    fitChartContent(chart, ["left"]);
    return;
  }

  const barSeconds = inferBarSeconds(times) ?? 60 * 60;
  const rangeSeconds =
    range === "1d" ? 86_400 :
    range === "1w" ? 7 * 86_400 :
    range === "1m" ? 30 * 86_400 :
    90 * 86_400;
  const count = Math.max(1, Math.ceil(rangeSeconds / barSeconds));
  chart.timeScale().setVisibleLogicalRange({
    from: Math.max(0, times.length - count),
    to: times.length + 2,
  });
  applyVerticalAutoScale(chart, ["left"]);
}

function inferBarSeconds(times: number[]): number | null {
  for (let i = times.length - 1; i > 0; i -= 1) {
    const diff = times[i] - times[i - 1];
    if (diff > 0) return diff;
  }
  return null;
}
