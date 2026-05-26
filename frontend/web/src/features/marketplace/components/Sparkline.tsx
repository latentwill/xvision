// src/features/marketplace/components/Sparkline.tsx
// Lightweight seeded SVG sparkline (handoff bc2 approach). The uPlot
// MiniSparkline stays for chart-grid use; this is for dense list rows.
function rng(seed: string) {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  let s = h >>> 0;
  return () => {
    s = Math.imul(s ^ (s >>> 15), 2246822507);
    s = Math.imul(s ^ (s >>> 13), 3266489909);
    s = (s ^ (s >>> 16)) >>> 0;
    return (s % 1_000_000) / 1_000_000;
  };
}

export function Sparkline({
  seed,
  positive,
  width = 88,
  height = 24,
}: {
  seed: string;
  positive: boolean;
  width?: number;
  height?: number;
}) {
  const r = rng(seed);
  let v = 50;
  const pts: number[] = [];
  for (let i = 0; i < 30; i++) {
    v += (positive ? 0.6 : -0.4) + (r() - 0.5) * 6;
    v = Math.max(8, Math.min(92, v));
    pts.push(v);
  }
  const d = pts
    .map((p, i) => `${i === 0 ? "M" : "L"} ${((i / 29) * width).toFixed(2)} ${(height - (p / 100) * height).toFixed(2)}`)
    .join(" ");
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} className="block">
      <path
        d={d}
        fill="none"
        stroke={positive ? "var(--gold)" : "var(--danger)"}
        strokeWidth="1.3"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
