// GenArtPlaceholder.tsx — renders the Bitfields v3 engine (the actual NFT art).
// Same engine as lib/genartGrid.ts; what cards show is what mints.
import { useEffect, useRef } from "react";
import { N, buildGridFromSeedString } from "../lib/genartGrid";

const CELL = 5;
const INTERNAL = N * CELL; // 140

function drawV3(ctx: CanvasRenderingContext2D, seed: string): void {
  const { grid, palette } = buildGridFromSeedString(seed);
  ctx.fillStyle = palette[0];
  ctx.fillRect(0, 0, INTERNAL, INTERNAL);
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const v = grid[y * N + x];
      if (v < 0) continue;
      ctx.fillStyle = palette[v];
      ctx.fillRect(x * CELL, y * CELL, CELL, CELL);
    }
  }
}

export function GenArtPlaceholder({
  seed,
  size = 80,
  className = "",
}: {
  seed: string;
  size?: number;
  className?: string;
}) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    drawV3(ctx, seed);
  }, [seed]);

  return (
    <canvas
      ref={ref}
      width={INTERNAL}
      height={INTERNAL}
      style={{ width: size, height: size, imageRendering: "pixelated" }}
      className={`block rounded-sm ${className}`}
      aria-label="strategy generative art"
      data-genart="bitfields-v3"
    />
  );
}
