// genartGrid.ts — Bitfields v3 engine. NORMATIVE: spec 2026-06-11 §2-§5.
// Any change here must be mirrored in crates/xvision-identity/src/genart.rs
// and re-validated against tests/fixtures/genart_v3.json.

export const N = 28;
const LAYERS = 6;
const STATES = 6;
const PAL_LEN = 7;
const DENSITY_FLOOR = 0.14;

export type SymmetryMode =
  | "free" | "mirror-x" | "mirror-y" | "quad"
  | "diagonal" | "anti-diagonal" | "rot180" | "rot90";

export interface Traits {
  palette: string;
  symmetry: SymmetryMode;
  density: number; // 0-100, % of filled display cells
  layers: number;  // always 6
}

// Locked roster — order is normative (spec Appendix A). Append-only post-launch.
export const PALETTES: ReadonlyArray<readonly [string, readonly string[]]> = [
  ["risoBlue", ["#0d1026","#1c2a6b","#2f4bb8","#3f6df2","#7fa3ff","#ffd23f","#fff6e0"]],
  ["risoRedTeal", ["#140a0d","#5c1a2e","#c1224f","#ff5470","#1ca3a3","#9fe3d4","#fff3e8"]],
  ["candyArcade", ["#0d0714","#2e1245","#5a1f7d","#9032a8","#e84393","#ffd24f","#fff5dc"]],
  ["circuit", ["#041013","#08242b","#0f5260","#18a98f","#a5f3dc","#ff3b73","#ffe95e"]],
  ["coldSignal", ["#071019","#102936","#23545b","#44a3a3","#c5e4dc","#f44465","#ffe6a7"]],
  ["grapeSoda", ["#0c0714","#231140","#41207a","#6a39b8","#9c6be0","#cda8f0","#f3e8fd"]],
  ["punolit", ["#11151f","#1e3442","#35665f","#89a36a","#d5c686","#df9d8b","#e8d7cf"]],
  ["calmSunset", ["#2c1534","#5c2751","#a94768","#df8584","#f3cda9","#f7e6b0","#fff7d6"]],
  ["lineage", ["#080916","#182044","#263b71","#426e91","#75a57d","#d2bc72","#f6ead0"]],
  ["signalRust", ["#0c0b0a","#26211c","#4f4138","#8a6450","#c97f4f","#f25c3a","#ffe9d4"]],
  ["magmaCore", ["#0c0508","#330a12","#70101c","#bf1f26","#f2542d","#ffa552","#ffe8c2"]],
  ["tidalDusk", ["#0a1012","#103035","#1a5e60","#2f9a8c","#e6c36b","#ef9f63","#fcefd2"]],
  ["ultraviolet", ["#08051a","#160d44","#2a1a80","#4730c4","#7a5ef2","#b49cff","#e9e2ff"]],
  ["voltYellow", ["#0a0a10","#1d2433","#2f4866","#4a7ab8","#7fb3e8","#ffe83f","#fdf8e2"]],
  ["mintMagenta", ["#070f0d","#0f2e26","#1a5c47","#2f9a73","#8fe0bb","#f23fa0","#fff0f7"]],
  ["tealEmber", ["#06100f","#0e3331","#176561","#2aa39a","#aee8df","#ff7733","#ffeed9"]],
  ["indigoCoral", ["#08081a","#161a4d","#2a2f8f","#4a55d6","#9aa3f2","#ff6f61","#fff1e8"]],
  ["limeViolet", ["#0b0d06","#222e0d","#3f5c14","#6f9a1f","#b8e040","#8a3ff2","#f4eaff"]],
  ["roseCyan", ["#120710","#3a0f2e","#73195c","#b8268f","#f060c4","#2ee6e6","#e8feff"]],
  ["amberInk", ["#0b0a12","#1f1d33","#3a3866","#5c59a8","#9a97d9","#ffb347","#fff3da"]],
  ["crimsonMint", ["#120709","#3f0d18","#7c142b","#c41f44","#f25c77","#5ce8b8","#eafff6"]],
  ["cobaltTangerine", ["#06091a","#0e1f56","#1a3b9e","#2f63e0","#85aaf2","#ff9433","#fff0dc"]],
  ["orchidLime", ["#100818","#2e1247","#5a2080","#9438c4","#d685f0","#cfe83f","#f9ffe0"]],
  ["pinkPitch", ["#0d0c0c","#1f1d1f","#3b373b","#6b6168","#b3a6ad","#ff3f8e","#ffe6f1"]],
  ["acidTeal", ["#0c1206","#1f330d","#3f6618","#6fa826","#b8e84a","#1fb8c9","#e0fbff"]],
  ["goldGrape", ["#0e0814","#291245","#4d1f7d","#7d33b8","#b370e0","#ffd23f","#fff6dc"]],
  ["rustTurquoise", ["#120b08","#3b1c10","#73331a","#b85426","#e88a4f","#2ec9b8","#e8fcf7"]],
  ["cherryCola", ["#100808","#330f12","#661a21","#a82a35","#e0525c","#ffc26b","#fff0d9"]],
  ["duskNeon", ["#0a0814","#1d1640","#352a73","#5444a8","#8a73d9","#3fffb8","#eafff5"]],
  ["peachAbyss", ["#050811","#0d1c3a","#173366","#2a52a3","#6f8fd9","#ffb38a","#fff0e2"]],
  ["saffronSea", ["#071013","#0f2c38","#1a5366","#2f85a3","#73c2d9","#ffc63f","#fff6da"]],
  ["furnacePink", ["#0f070c","#360d2b","#6e1452","#b81f7d","#f23fb0","#ffae3f","#ffeed4"]],
  ["glacierPunch", ["#070b10","#13283d","#234a73","#3f78b3","#8fc1e8","#f2543f","#ffe9e0"]],
];

const SYMMETRY_BAG: readonly SymmetryMode[] = [
  "free", "free", "free", "mirror-x", "mirror-y", "quad", "quad", "quad",
  "diagonal", "anti-diagonal", "rot180", "rot90", "rot90",
];

export function fnv1a32(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

export function mulberry32(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t = (t ^ (t + Math.imul(t ^ (t >>> 7), t | 61))) >>> 0;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Op = (a: number, b: number) => number;
const OPS: readonly Op[] = [(a, b) => a & b, (a, b) => a ^ b, (a, b) => a | b];

function rawGrid(seedStr: string, transparent: number): Int8Array {
  const r = mulberry32(fnv1a32(seedStr));
  const grid = new Int8Array(N * N).fill(-1);
  for (let L = 0; L < LAYERS; L++) {
    const op = OPS[Math.floor(r() * 3)];
    const band = 1 + Math.floor(r() * 7);
    const base = 2 + Math.floor(r() * 9);
    const xo = Math.floor(r() * 64);
    const yo = Math.floor(r() * 64);
    const radial = r() > 0.7;
    const invert = r() > 0.8;
    const cx = N / 2 + xo;
    const cy = N / 2 + yo;
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const dx = x + xo - cx;
        const dy = y + yo - cy;
        const step = radial
          ? Math.floor(Math.sqrt(dx * dx + dy * dy) / band)
          : Math.floor((y + yo) / band);
        const t = base + step;
        let v = op(x + y + xo, y - x + yo);
        if (invert) v = ~v;
        v = ((v % t) + t) % t;
        const s = v % (STATES + transparent);
        if (s < transparent) continue;
        grid[y * N + x] = (s - transparent) % PAL_LEN;
      }
    }
  }
  return grid;
}

function filledRatio(grid: Int8Array): number {
  let filled = 0;
  for (let i = 0; i < grid.length; i++) if (grid[i] >= 0) filled++;
  return filled / (N * N);
}

function denseGrid(seedStr: string): Int8Array {
  let transparent = 7;
  for (let attempt = 0; attempt < 5; attempt++) {
    const g = rawGrid(seedStr + (attempt ? `#${attempt}` : ""), transparent);
    if (filledRatio(g) >= DENSITY_FLOOR) return g;
    transparent = Math.max(2, transparent - 2);
  }
  return rawGrid(seedStr + "#final", 2);
}

function canon(mode: SymmetryMode, x: number, y: number): [number, number] {
  switch (mode) {
    case "free": return [x, y];
    case "mirror-x": return [Math.min(x, N - 1 - x), y];
    case "mirror-y": return [x, Math.min(y, N - 1 - y)];
    case "quad": return [Math.min(x, N - 1 - x), Math.min(y, N - 1 - y)];
    case "diagonal": return x < y ? [y, x] : [x, y];
    case "anti-diagonal": return x + y > N - 1 ? [N - 1 - y, N - 1 - x] : [x, y];
    case "rot180": {
      return y * N + x <= (N - 1 - y) * N + (N - 1 - x) ? [x, y] : [N - 1 - x, N - 1 - y];
    }
    case "rot90": {
      let bx = x, by = y, bi = y * N + x, cx = x, cy = y;
      for (let k = 0; k < 3; k++) {
        const nx = cy, ny = N - 1 - cx;
        cx = nx; cy = ny;
        const i = cy * N + cx;
        if (i < bi) { bi = i; bx = cx; by = cy; }
      }
      return [bx, by];
    }
  }
}

export interface BuiltGrid {
  grid: Int8Array;            // display grid, post-symmetry, values -1..6
  palette: readonly string[]; // 7 hex colors
  traits: Traits;
}

/** Engine entry for arbitrary seed strings (previews without a manifest hash). */
export function buildGridFromSeedString(seedString: string): BuiltGrid {
  const r = mulberry32(fnv1a32(seedString));
  const [paletteName, palette] = PALETTES[Math.floor(r() * PALETTES.length)];
  const symmetry = SYMMETRY_BAG[Math.floor(r() * SYMMETRY_BAG.length)];
  const raw = denseGrid(seedString);
  const grid = new Int8Array(N * N);
  let filled = 0;
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const [sx, sy] = canon(symmetry, x, y);
      const v = raw[sy * N + sx];
      grid[y * N + x] = v;
      if (v >= 0) filled++;
    }
  }
  const density = Math.round((100 * filled) / (N * N));
  return { grid, palette, traits: { palette: paletteName, symmetry, density, layers: LAYERS } };
}

const AGENT_ID_RE = /^[0-9A-Za-z_-]{1,64}$/;
const HASH_RE = /^[0-9a-f]{64}$/;

/** Validated entry for mint-path use. Throws on bad input — never falls back. */
export function buildGrid(agentId: string, manifestHash: string): BuiltGrid {
  if (!AGENT_ID_RE.test(agentId)) throw new Error(`invalid agent_id: ${JSON.stringify(agentId)}`);
  if (!HASH_RE.test(manifestHash)) throw new Error("manifest_hash must be 64-char lowercase hex");
  return buildGridFromSeedString(`${agentId}:${manifestHash}`);
}
