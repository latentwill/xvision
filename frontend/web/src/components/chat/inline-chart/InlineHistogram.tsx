import { DEFAULT_VIEWBOX, histogramBars } from "./shape";
import { toneColors } from "./palette";
import type { InlineChartSeries } from "./types";

export function InlineHistogram({ series }: { series: InlineChartSeries }) {
  const bars = histogramBars(series.points, DEFAULT_VIEWBOX);
  const positive = toneColors(series.tone ?? "gold");
  const negative = toneColors("danger");

  return (
    <svg
      viewBox={`0 0 ${DEFAULT_VIEWBOX.width} ${DEFAULT_VIEWBOX.height}`}
      className="w-full h-[112px]"
      role="img"
      aria-label={`${series.label} histogram`}
    >
      <line
        x1={DEFAULT_VIEWBOX.padX}
        x2={DEFAULT_VIEWBOX.width - DEFAULT_VIEWBOX.padX}
        y1={DEFAULT_VIEWBOX.height / 2}
        y2={DEFAULT_VIEWBOX.height / 2}
        stroke="rgba(210,206,196,0.18)"
      />
      {bars.map((bar, index) => (
        <rect
          key={index}
          x={bar.x}
          y={bar.y}
          width={bar.width}
          height={bar.height}
          rx="1.5"
          fill={bar.positive ? positive.stroke : negative.stroke}
          opacity="0.82"
        />
      ))}
    </svg>
  );
}
