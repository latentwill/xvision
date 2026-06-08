// Bitfields v2 generative art — port of docs/testnft/code/v2/index.html
// Drop-in replacement for the SVG gradient placeholder. Same props: seed, size, className.

import { useEffect, useRef } from "react";

const OPS = {
  AND: (a: number, b: number) => a & b,
  XOR: (a: number, b: number) => a ^ b,
  OR:  (a: number, b: number) => a | b,
} as const;
const OP_KEYS = Object.keys(OPS) as (keyof typeof OPS)[];

const PALETTES: Record<string, readonly string[]> = {
  calmSunset: ['#2c1534','#5c2751','#a94768','#df8584','#f3cda9','#f7e6b0','#fff7d6'],
  punolit:    ['#11151f','#1e3442','#35665f','#89a36a','#d5c686','#df9d8b','#e8d7cf'],
  coldSignal: ['#071019','#102936','#23545b','#44a3a3','#c5e4dc','#f44465','#ffe6a7'],
  xvision:    ['#080911','#171a33','#29265b','#6046a8','#b774d6','#f0c9ff','#e9f6ff'],
};

const STUDIES = [
  { name: 'Calm Sunset Stack',   palette: 'calmSunset', layers: 8, states: 7,  transparent: 6,  modeBias: .72, cset: [8,16,32,64,128]     },
  { name: 'Punolit Signal Mass', palette: 'punolit',    layers: 8, states: 21, transparent: 21, modeBias: .82, cset: [4,8,16,32,64]        },
  { name: 'Liquidity Relic',     palette: 'coldSignal', layers: 6, states: 9,  transparent: 10, modeBias: .55, cset: [8,16,32,64,128,256]  },
  { name: 'Strategy Genome',     palette: 'xvision',    layers: 7, states: 11, transparent: 13, modeBias: .65, cset: [8,16,32,64,128]      },
];

// Internal canvas resolution. 128×128 renders the same cell-count as the 1024×1024
// original (cells stay proportional), scales cleanly to any display size.
const INTERNAL = 128;
const ORIG_GRID = 1024;
const SCALE = INTERNAL / ORIG_GRID;

function mkRng(seed: number): () => number {
  let s = seed;
  return () => {
    let t = (s += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function fnv1a(str: string): number {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

function pick<T>(r: () => number, a: readonly T[]): T {
  return a[Math.floor(r() * a.length)];
}

function drawBitfield(ctx: CanvasRenderingContext2D, seed: string): void {
  const h = fnv1a(seed);
  const spec = STUDIES[h % STUDIES.length];
  const pal = PALETTES[spec.palette];
  // Seed RNG as original: hash(baseSeed + spec.name)
  const r = mkRng(fnv1a(seed + spec.name));

  // Consume one RNG call for bg color (matches original sequence); overwrite with pal[0]
  pick(r, pal.slice(0, 3) as string[]);
  ctx.fillStyle = pal[0];
  ctx.fillRect(0, 0, INTERNAL, INTERNAL);

  for (let L = 0; L < spec.layers; L++) {
    const opName = pick(r, OP_KEYS);
    const op = OPS[opName];
    const origC = pick(r, spec.cset);
    // Scale cell size; cols/rows stay identical to the original (== ORIG_GRID / origC)
    const cs = Math.max(1, Math.round(origC * SCALE));
    const cols = Math.round(INTERNAL / cs);
    const rows = cols;
    const band   = 1 + Math.floor(r() * 9);
    const base   = 1 + Math.floor(r() * 10);
    const xOff   = Math.floor(r() * 256);
    const yOff   = Math.floor(r() * 256);
    const radial = r() > spec.modeBias;
    const mirror = r() > 0.55;
    const invert = r() > 0.76;
    const cx = cols / 2 + xOff;
    const cy = rows / 2 + yOff;

    for (let yy = 0; yy < rows; yy++) {
      for (let xx = 0; xx < cols; xx++) {
        const x = mirror && L % 2 ? cols - 1 - xx : xx;
        const step = radial
          ? Math.floor(Math.hypot(x + xOff - cx, yy + yOff - cy) / band)
          : Math.floor((yy + yOff) / band);
        const t = base + step;
        let v = op(x + yy + xOff, yy - x + yOff);
        if (invert) v = ~v;
        v = ((v % t) + t) % t;
        const state = v % (spec.states + spec.transparent);
        if (state < spec.transparent) continue;
        const pi = (state - spec.transparent) % pal.length;
        ctx.fillStyle = pal[pi] as string;
        ctx.fillRect(xx * cs, yy * cs, cs, cs);
      }
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
    drawBitfield(ctx, seed);
  }, [seed]);

  return (
    <canvas
      ref={ref}
      width={INTERNAL}
      height={INTERNAL}
      style={{ width: size, height: size, imageRendering: "pixelated" }}
      className={`block rounded-sm ${className}`}
      aria-label="strategy generative art"
      data-genart="bitfields-v2"
    />
  );
}
