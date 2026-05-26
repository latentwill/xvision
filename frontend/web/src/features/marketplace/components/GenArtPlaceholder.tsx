// src/features/marketplace/components/GenArtPlaceholder.tsx
// PLACEHOLDER ONLY — gen-art is unscoped until Phase 4 (program strategy H3/D-2).
// Deterministic, seed-keyed gradient block. The real generator replaces this
// behind the same props (seed, size). Do NOT treat as canonical art.

function fnv1a(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

const HUES = [150, 210, 265, 45, 330, 190]; // green, sky, violet, amber, pink, teal

export function GenArtPlaceholder({
  seed,
  size = 80,
  className = "",
}: {
  seed: string;
  size?: number;
  className?: string;
}) {
  const h = fnv1a(seed);
  const hueA = HUES[h % HUES.length];
  const hueB = HUES[(h >>> 3) % HUES.length];
  const id = `gp-${h.toString(36)}`;
  return (
    <svg
      data-genart="placeholder"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      role="img"
      aria-label="strategy art placeholder"
      className={`block rounded-sm ${className}`}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor={`hsl(${hueA} 70% 22%)`} />
          <stop offset="100%" stopColor={`hsl(${hueB} 65% 12%)`} />
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="100" height="100" fill={`url(#${id})`} />
      <circle cx={20 + (h % 60)} cy={20 + ((h >>> 5) % 60)} r={10 + (h % 18)} fill={`hsl(${hueA} 80% 55% / 0.35)`} />
      <rect x={(h >>> 7) % 70} y={(h >>> 9) % 70} width="26" height="26" fill={`hsl(${hueB} 80% 60% / 0.25)`} />
    </svg>
  );
}
