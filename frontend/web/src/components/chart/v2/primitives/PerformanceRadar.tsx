/**
 * PerformanceRadar — pure-SVG radar chart. 6 axes (Return / Sharpe /
 * Stability / Win Rate / Consistency / Drawdown), 4 ring polygons at
 * 25/50/75/100%, overlays the top-N strategies as filled polygons
 * with vertex dots.
 *
 * Pure presentational — caller normalises each strategy's metric
 * vector to [0,1] before passing it in. Geometry helpers are exported
 * for tests.
 */
import type { ReactElement } from "react";

export const RADAR_DIMENSIONS = 6;
export const RADAR_RINGS = [0.25, 0.5, 0.75, 1.0];

export const RADAR_AXIS_LABELS: readonly string[] = [
  "RETURN",
  "SHARPE",
  "STABILITY",
  "WIN RATE",
  "CONSISTENCY",
  "DRAWDOWN",
];

export interface RadarStrategy {
  id: string;
  label: string;
  color: string;
  /** Normalised to [0,1], one value per axis (length = RADAR_DIMENSIONS). */
  values: number[];
}

export interface PerformanceRadarProps {
  /** Top-N (typically 3). Caller is responsible for the slice. */
  strategies: RadarStrategy[];
  width?: number;
  height?: number;
}

/**
 * Convert a normalised [0,1] series into the polygon path string used
 * by the radar fill. `cx`/`cy` are the chart center; `radius` scales
 * the unit vector. Exported for tests.
 */
export function polygonPoints(
  values: number[],
  cx: number,
  cy: number,
  radius: number,
): string {
  const n = values.length;
  if (n === 0) return "";
  const pts: string[] = [];
  for (let i = 0; i < n; i++) {
    const angle = -Math.PI / 2 + (i * 2 * Math.PI) / n;
    const r = Math.max(0, Math.min(1, values[i])) * radius;
    const x = cx + r * Math.cos(angle);
    const y = cy + r * Math.sin(angle);
    pts.push(`${x.toFixed(2)},${y.toFixed(2)}`);
  }
  return pts.join(" ");
}

/**
 * Polygon coordinates for a single ring at fractional `r` (e.g. 0.5)
 * on a `RADAR_DIMENSIONS`-side polygon. Exported for tests.
 */
export function ringPoints(
  r: number,
  cx: number,
  cy: number,
  radius: number,
): string {
  return polygonPoints(Array(RADAR_DIMENSIONS).fill(r), cx, cy, radius);
}

export function PerformanceRadar({
  strategies,
  width = 260,
  height = 220,
}: PerformanceRadarProps): ReactElement {
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(cx, cy) - 30; // leave room for labels at 1.18×

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
      role="img"
      aria-label="Performance radar"
      style={{ display: "block" }}
      data-testid="performance-radar"
    >
      {/* Rings */}
      {RADAR_RINGS.map((r) => (
        <polygon
          key={r}
          points={ringPoints(r, cx, cy, radius)}
          fill="none"
          stroke="var(--border-soft)"
          strokeWidth={1}
          opacity={0.55}
        />
      ))}

      {/* Axis spokes */}
      {Array.from({ length: RADAR_DIMENSIONS }).map((_, i) => {
        const angle = -Math.PI / 2 + (i * 2 * Math.PI) / RADAR_DIMENSIONS;
        const x = cx + radius * Math.cos(angle);
        const y = cy + radius * Math.sin(angle);
        return (
          <line
            key={i}
            x1={cx}
            y1={cy}
            x2={x}
            y2={y}
            stroke="var(--border-soft)"
            strokeWidth={1}
            opacity={0.35}
          />
        );
      })}

      {/* Strategy polygons */}
      {strategies.map((s) => (
        <g key={s.id}>
          <polygon
            points={polygonPoints(s.values, cx, cy, radius)}
            fill={s.color}
            fillOpacity={0.10}
            stroke={s.color}
            strokeWidth={1.4}
          />
          {s.values.map((v, i) => {
            const angle = -Math.PI / 2 + (i * 2 * Math.PI) / s.values.length;
            const r = Math.max(0, Math.min(1, v)) * radius;
            const x = cx + r * Math.cos(angle);
            const y = cy + r * Math.sin(angle);
            return (
              <circle
                key={`${s.id}-${i}`}
                cx={x}
                cy={y}
                r={2.2}
                fill={s.color}
              />
            );
          })}
        </g>
      ))}

      {/* Axis labels at 1.18× radius */}
      {RADAR_AXIS_LABELS.map((label, i) => {
        const angle = -Math.PI / 2 + (i * 2 * Math.PI) / RADAR_DIMENSIONS;
        const lr = radius * 1.18;
        const x = cx + lr * Math.cos(angle);
        const y = cy + lr * Math.sin(angle);
        return (
          <text
            key={label}
            x={x}
            y={y}
            fontSize={9}
            fill="var(--text-3)"
            textAnchor="middle"
            dominantBaseline="middle"
            style={{
              fontFamily: '"Inter", sans-serif',
              letterSpacing: "0.08em",
            }}
          >
            {label}
          </text>
        );
      })}
    </svg>
  );
}
