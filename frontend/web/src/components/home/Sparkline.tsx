// frontend/web/src/components/home/Sparkline.tsx
//
// Tiny inline-SVG sparkline for KPI cells and leaderboard rows. Pure SVG —
// no canvas, no uPlot — so it renders in jsdom and costs nothing at runtime.
// Strokes read theme tokens via CSS variables; non-finite samples are
// filtered before layout so the polyline can never receive NaN coordinates.

export type SparklineTone = "gold" | "danger" | "info" | "warn" | "muted";

const TONE_VAR: Record<SparklineTone, string> = {
  gold: "var(--gold)",
  danger: "var(--danger)",
  info: "var(--info)",
  warn: "var(--warn)",
  muted: "var(--text-4)",
};

export interface SparklineProps {
  values: number[];
  tone?: SparklineTone;
  width?: number;
  height?: number;
  /** Render a soft area fill under the line (default true). */
  fill?: boolean;
  className?: string;
  "data-testid"?: string;
}

export function Sparkline({
  values,
  tone = "gold",
  width = 72,
  height = 20,
  fill = true,
  className = "",
  "data-testid": testId,
}: SparklineProps) {
  const finite = values.filter((v) => Number.isFinite(v));
  if (finite.length < 2) return null;

  const pad = 1.5;
  const min = Math.min(...finite);
  const max = Math.max(...finite);
  const span = max - min;
  const innerH = height - pad * 2;
  const innerW = width - pad * 2;
  const stepX = innerW / (finite.length - 1);

  const pts = finite.map((v, i) => {
    const x = pad + i * stepX;
    // Flat series renders as a midline rather than hugging an edge.
    const norm = span === 0 ? 0.5 : (v - min) / span;
    const y = pad + (1 - norm) * innerH;
    return [x, y] as const;
  });
  const linePoints = pts.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
  const areaPoints = `${pad},${height - pad} ${linePoints} ${(pad + (finite.length - 1) * stepX).toFixed(2)},${height - pad}`;
  const color = TONE_VAR[tone];

  return (
    <svg
      data-testid={testId}
      aria-hidden="true"
      className={`shrink-0 ${className}`}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="presentation"
    >
      {fill ? (
        <polygon points={areaPoints} fill={color} fillOpacity={0.12} stroke="none" />
      ) : null}
      <polyline
        points={linePoints}
        fill="none"
        stroke={color}
        strokeWidth={1.25}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
