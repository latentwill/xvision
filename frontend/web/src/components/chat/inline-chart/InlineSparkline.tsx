import { linePath, normalizePoints, seriesBounds } from "./shape";
import { toneColors } from "./palette";
import type { InlineChartSeries, ViewBox } from "./types";

const SPARK_VIEWBOX: ViewBox = {
  width: 88,
  height: 28,
  padX: 2,
  padY: 3,
};

export function InlineSparkline({ series }: { series: InlineChartSeries }) {
  const bounds = seriesBounds([series]);
  const points = normalizePoints(series.points, bounds, SPARK_VIEWBOX);
  const colors = toneColors(series.tone);
  const path = linePath(points);

  return (
    <svg
      viewBox={`0 0 ${SPARK_VIEWBOX.width} ${SPARK_VIEWBOX.height}`}
      className="w-[88px] h-7"
      role="img"
      aria-label={`${series.label} sparkline`}
    >
      <path
        d={path}
        fill="none"
        stroke={colors.stroke}
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
