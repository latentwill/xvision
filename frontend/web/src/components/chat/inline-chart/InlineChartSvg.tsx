import {
  areaPath,
  DEFAULT_VIEWBOX,
  linePath,
  normalizePoints,
  seriesBounds,
} from "./shape";
import { SERIES_TONES, toneColors } from "./palette";
import { InlineHistogram } from "./InlineHistogram";
import type { InlineChartContentBlock, InlineChartSeries } from "./types";

export function InlineChartSvg({ payload }: { payload: InlineChartContentBlock }) {
  if (payload.series.length === 0) return <EmptyChart />;
  if (payload.kind === "histogram") {
    return <InlineHistogram series={payload.series[0]} />;
  }
  if (payload.kind === "sparkline") {
    return <LineSeriesChart series={payload.series.slice(0, 1)} area={false} />;
  }
  if (payload.kind === "drawdown") {
    return <LineSeriesChart series={payload.series.slice(0, 1)} area negative />;
  }
  return (
    <LineSeriesChart
      series={payload.series.slice(0, payload.kind === "compare" ? 4 : 1)}
      area={payload.kind === "equity"}
    />
  );
}

function LineSeriesChart({
  series,
  area = false,
  negative = false,
}: {
  series: InlineChartSeries[];
  area?: boolean;
  negative?: boolean;
}) {
  const bounds = seriesBounds(series);
  return (
    <svg
      viewBox={`0 0 ${DEFAULT_VIEWBOX.width} ${DEFAULT_VIEWBOX.height}`}
      className="w-full h-[112px]"
      aria-hidden
      focusable="false"
    >
      <GridLines />
      {series.map((item, index) => {
        const tone = item.tone ?? SERIES_TONES[index % SERIES_TONES.length];
        const colors = toneColors(negative ? "danger" : tone);
        const points = normalizePoints(item.points, bounds, DEFAULT_VIEWBOX);
        const line = linePath(points);
        const areaD = areaPath(points, DEFAULT_VIEWBOX);
        return (
          <g key={item.id || index}>
            {area && areaD ? <path d={areaD} fill={colors.fill} /> : null}
            <path
              d={line}
              fill="none"
              stroke={colors.stroke}
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </g>
        );
      })}
    </svg>
  );
}

function GridLines() {
  const ys = [22, 56, 90];
  return (
    <g>
      {ys.map((y) => (
        <line
          key={y}
          x1={DEFAULT_VIEWBOX.padX}
          x2={DEFAULT_VIEWBOX.width - DEFAULT_VIEWBOX.padX}
          y1={y}
          y2={y}
          stroke="rgba(210,206,196,0.11)"
        />
      ))}
    </g>
  );
}

function EmptyChart() {
  return (
    <div className="h-[112px] rounded border border-border-soft bg-surface-elev flex items-center justify-center text-[12px] text-text-3">
      No chart data
    </div>
  );
}
