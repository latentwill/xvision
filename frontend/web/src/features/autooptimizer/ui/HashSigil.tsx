// Deterministic 5×5 mirrored identicon from a content hash. Replaces the
// gen-art thumbnails from the design mockups (gen-art pipeline is out of
// scope for this redesign — see the design spec §2 non-goals).

import type { ReactNode } from "react";

function hashToInt(hash: string): number {
  let h = 2166136261;
  for (let i = 0; i < hash.length; i++) {
    h ^= hash.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

// Signal theme accent vars — auto-adapt between light/dark.
const ACCENTS = ["var(--gold)", "var(--info)", "var(--violet)"];

export function HashSigil({
  hash,
  size = 40,
}: {
  hash: string;
  size?: number;
}) {
  const seed = hashToInt(hash || "•");
  const fg = ACCENTS[seed % ACCENTS.length];
  const cells = 5;
  const unit = size / cells;
  const rects: ReactNode[] = [];
  // Build a left half (3 cols) and mirror it for visual symmetry.
  for (let row = 0; row < cells; row++) {
    for (let col = 0; col < 3; col++) {
      const on = ((seed >> (row * 3 + col)) & 1) === 1;
      if (!on) continue;
      const mirror = cells - 1 - col;
      for (const c of new Set([col, mirror])) {
        rects.push(
          <rect
            key={`${row}-${c}`}
            x={c * unit}
            y={row * unit}
            width={unit}
            height={unit}
            fill={fg}
          />,
        );
      }
    }
  }
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      role="img"
      aria-label={`identity ${hash.slice(0, 8)}`}
      className="rounded border border-border bg-surface-elev"
    >
      {rects}
    </svg>
  );
}
