import { useEffect, useMemo, useRef, useState } from "react";
import {
  ColorType,
  CrosshairMode,
  createChart,
  type Logical,
  type UTCTimestamp,
} from "lightweight-charts";
import type { StrategyChartPayload } from "@/api/types.gen/StrategyChartPayload";
import type { ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { chartTheme, normalizeChartTheme } from "./chart-theme";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { applyVerticalAutoScale, fitChartContent } from "./chart-fit";

const SCENARIO_PALETTE = [
  "#22d3ee",
  "#a78bfa",
  "#34d399",
  "#fbbf24",
  "#f87171",
  "#60a5fa",
  "#fb923c",
  "#10b981",
];

export function StrategyChart({
  payload,
  theme,
  themeMode,
}: {
  payload: StrategyChartPayload;
  themeMode?: "dark" | "light";
  theme?: ResolvedTheme;
}) {
  const appTheme = useTheme();
  const activeTheme = theme ?? normalizeChartTheme(themeMode, appTheme.resolvedTheme);
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>("All");

  // Stable color per scenario_id (deterministic across re-renders)
  const scenarioColors = useMemo(() => {
    const m = new Map<string, string>();
    payload.scenarios.forEach(([id], i) =>
      m.set(id, SCENARIO_PALETTE[i % SCENARIO_PALETTE.length]),
    );
    return m;
  }, [payload.scenarios]);

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
      timeScale: { rightOffset: 12, timeVisible: false, secondsVisible: false },
    });

    for (const r of payload.run_series) {
      const color = scenarioColors.get(r.scenario_id) ?? "#94a3b8";
      if (r.equity_normalised.length === 0) continue;
      const line = c.addLineSeries({ color, lineWidth: 1, title: r.label });
      line.setData(
        r.equity_normalised.map((p) => ({
          time: p.time as UTCTimestamp,
          value: p.equity_usd,
        })),
      );
    }

    applyRange(c, range, collectVisibleTimes(payload));

    return () => c.remove();
  }, [payload, activeTheme, scenarioColors, range]);

  if (payload.run_series.length === 0) {
    return (
      <div className="px-4 py-8 text-text-3 text-[13px] text-center">
        This strategy has no completed runs yet. Launch one from{" "}
        <code className="font-mono text-text-2">/eval-runs</code>.
      </div>
    );
  }

  return (
    <div>
      <Legend payload={payload} scenarioColors={scenarioColors} />
      <ChartContainer
        range={range}
        onRange={setRange}
        layersPanel={
          <div className="text-text-3 text-[12px]">No layers in v1.</div>
        }
      >
        <div ref={ref} style={{ height: 420 }} />
      </ChartContainer>
    </div>
  );
}

function collectVisibleTimes(payload: StrategyChartPayload) {
  const times = new Set<number>();
  payload.run_series.forEach((run) => {
    run.equity_normalised.forEach((point) => times.add(point.time));
  });
  return [...times].sort((a, b) => a - b);
}

function applyRange(
  chart: ReturnType<typeof createChart>,
  range: RangePreset,
  times: number[],
) {
  if (times.length <= 0) return;
  if (range === "All") {
    fitChartContent(chart);
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
    from: Math.max(0, times.length - count) as Logical,
    to: (times.length + 2) as Logical,
  });
  applyVerticalAutoScale(chart);
}

function inferBarSeconds(times: number[]): number | null {
  for (let i = times.length - 1; i > 0; i -= 1) {
    const diff = times[i] - times[i - 1];
    if (diff > 0) return diff;
  }
  return null;
}

function Legend({
  payload,
  scenarioColors,
}: {
  payload: StrategyChartPayload;
  scenarioColors: Map<string, string>;
}) {
  // Count runs per scenario
  const counts = new Map<string, number>();
  for (const r of payload.run_series) {
    counts.set(r.scenario_id, (counts.get(r.scenario_id) ?? 0) + 1);
  }

  return (
    <div className="flex flex-wrap gap-3 text-[12px] mb-2">
      {payload.scenarios.map(([sid, name]) => (
        <span key={sid} className="inline-flex items-center gap-1.5">
          <span
            className="inline-block w-3 h-1.5 rounded-sm"
            style={{ background: scenarioColors.get(sid) }}
          />
          <span className="text-text-2">{name}</span>
          <span className="text-text-3">({counts.get(sid) ?? 0} runs)</span>
        </span>
      ))}
    </div>
  );
}
