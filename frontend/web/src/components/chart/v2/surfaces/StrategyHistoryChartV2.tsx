import { useMemo, useState } from "react";

import type { StrategyChartPayload } from "@/api/types.gen/StrategyChartPayload";
import { ChartFrame, type RangePreset } from "../primitives/ChartFrame";
import { rangeWindowSeconds } from "../primitives/range-window";
import {
  MultiStrategyEquityPane,
  type MultiStrategyEquitySeries,
} from "../primitives/MultiStrategyEquityPane";

/**
 * Only day-scale and coarser presets are meaningful for multi-day eval-run
 * completion timestamps. Intraday windows (1h/4h/6h/12h) collapse the chart
 * to a single diagonal line with repeated date labels, so we drop them here.
 */
const STRATEGY_HISTORY_PRESETS: RangePreset[] = ["1d", "1w", "All"];

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

export function StrategyHistoryChartV2({
  payload,
}: {
  payload: StrategyChartPayload;
}) {
  const [range, setRange] = useState<RangePreset>("All");
  const chart = useMemo(() => buildStrategyHistoryChart(payload), [payload]);
  const visibleChart = useMemo(() => applyRange(chart, range), [chart, range]);

  if (payload.run_series.length === 0) {
    return (
      <div className="px-4 py-8 text-text-3 text-[13px] text-center">
        This strategy has no completed runs yet. Launch one from{" "}
        <code className="font-mono text-text-2">/eval-runs</code>.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <ScenarioLegend
        scenarios={payload.scenarios}
        counts={chart.counts}
        colors={chart.scenarioColors}
      />
      <ChartFrame
        title="Return %"
        range={range}
        onRange={setRange}
        presets={STRATEGY_HISTORY_PRESETS}
      >
        <MultiStrategyEquityPane
          time={visibleChart.time}
          series={visibleChart.series}
          leadId={visibleChart.series[0]?.id}
          height={360}
          syncKey={`strategy-history-${payload.strategy_id}`}
          compactXAxisLabels
        />
      </ChartFrame>
    </div>
  );
}

export function buildStrategyHistoryChart(payload: StrategyChartPayload): {
  time: number[];
  series: MultiStrategyEquitySeries[];
  counts: Map<string, number>;
  scenarioColors: Map<string, string>;
} {
  const scenarioColors = new Map<string, string>();
  payload.scenarios.forEach(([id], i) => {
    scenarioColors.set(id, SCENARIO_PALETTE[i % SCENARIO_PALETTE.length]);
  });

  const counts = new Map<string, number>();
  const times = new Set<number>();
  for (const run of payload.run_series) {
    counts.set(run.scenario_id, (counts.get(run.scenario_id) ?? 0) + 1);
    for (const point of run.equity_normalised) {
      times.add(point.time);
    }
  }

  const time = [...times].sort((a, b) => a - b);
  const timeIndex = new Map(time.map((t, i) => [t, i]));
  const series = payload.run_series
    .filter((run) => run.equity_normalised.length > 0)
    .map((run, index) => {
      const values: Array<number | null> = Array.from(
        { length: time.length },
        () => null,
      );
      for (const point of run.equity_normalised) {
        const idx = timeIndex.get(point.time);
        if (idx !== undefined) values[idx] = point.equity_usd;
      }
      return {
        id: run.run_id,
        label: run.label,
        values,
        color:
          scenarioColors.get(run.scenario_id) ??
          SCENARIO_PALETTE[index % SCENARIO_PALETTE.length],
      };
    });

  return { time, series, counts, scenarioColors };
}

function applyRange(
  chart: ReturnType<typeof buildStrategyHistoryChart>,
  range: RangePreset,
): ReturnType<typeof buildStrategyHistoryChart> {
  if (range === "All" || chart.time.length === 0) return chart;

  const windowSec = rangeWindowSeconds(range);
  if (windowSec === null) return chart;

  const maxTime = chart.time[chart.time.length - 1];
  const cutoff = maxTime - windowSec;
  const indexes = chart.time
    .map((time, index) => ({ time, index }))
    .filter(({ time }) => time >= cutoff);

  if (indexes.length === 0) return chart;

  return {
    ...chart,
    time: indexes.map(({ time }) => time),
    series: chart.series.map((series) => ({
      ...series,
      values: indexes.map(({ index }) => series.values[index] ?? null),
    })),
  };
}

function ScenarioLegend({
  scenarios,
  counts,
  colors,
}: {
  scenarios: Array<[string, string]>;
  counts: Map<string, number>;
  colors: Map<string, string>;
}) {
  return (
    <div className="flex flex-wrap gap-3 text-[12px]">
      {scenarios.map(([sid, name]) => (
        <span key={sid} className="inline-flex items-center gap-1.5">
          <span
            className="inline-block w-3 h-1.5 rounded-sm"
            style={{ background: colors.get(sid) }}
          />
          <span className="text-text-2">{name}</span>
          <span className="text-text-3">({counts.get(sid) ?? 0} runs)</span>
        </span>
      ))}
    </div>
  );
}
