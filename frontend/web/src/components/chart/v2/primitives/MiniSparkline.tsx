import { useId } from "react";
import type { DrawdownPoint, EquityPoint } from "../types";

type Props = {
  points: EquityPoint[] | DrawdownPoint[];
  color: string;
  variant?: "equity" | "drawdown";
  height?: number;
};

export function MiniSparkline({
  points,
  color,
  variant = "equity",
  height = 54,
}: Props) {
  const gradientId = useId().replace(/:/g, "");
  const width = 240;
  const path = linePath(points, width, height);
  const area = areaPath(points, width, height);
  const stroke = variant === "drawdown" ? "var(--danger)" : color;
  const fillOpacity = variant === "drawdown" ? 0.22 : 0.18;

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="block w-full"
      style={{ height }}
      role="img"
      aria-label={`${variant} sparkline`}
    >
      <defs>
        <linearGradient id={gradientId} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity={fillOpacity} />
          <stop offset="100%" stopColor={stroke} stopOpacity={0} />
        </linearGradient>
      </defs>
      <path d={area} fill={`url(#${gradientId})`} />
      <path d={path} fill="none" stroke={stroke} strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

function linePath(points: EquityPoint[] | DrawdownPoint[], width: number, height: number): string {
  const coords = project(points, width, height);
  if (coords.length === 0) return "";
  return coords.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
}

function areaPath(points: EquityPoint[] | DrawdownPoint[], width: number, height: number): string {
  const coords = project(points, width, height);
  if (coords.length === 0) return "";
  const line = coords.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
  return `${line} L${width},${height} L0,${height} Z`;
}

function project(
  points: EquityPoint[] | DrawdownPoint[],
  width: number,
  height: number,
): Array<[number, number]> {
  if (points.length === 0) return [];
  const values = points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const xSpan = Math.max(1, points.length - 1);
  return points.map((p, i) => {
    const x = (i / xSpan) * width;
    const y = height - ((p.value - min) / span) * (height - 4) - 2;
    return [x, y];
  });
}
