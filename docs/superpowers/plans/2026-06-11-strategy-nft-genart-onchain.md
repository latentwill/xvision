# Strategy NFT Genart Onchain (Bitfields v3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Bitfields-v3 "unified family" generative-art engine as byte-identical Rust/TS twins, render it on marketplace cards, and wire it into the mint/publish flow so a minted strategy NFT carries a fully-onchain `data:` tokenURI on Mantle.

**Architecture:** A pure deterministic engine (FNV-1a 32 → mulberry32 → 28×28 composited bitfield → symmetry mapping → RLE SVG → base64 tokenURI) implemented twice — `crates/xvision-identity/src/genart.rs` (mint path) and `frontend/web/src/features/marketplace/lib/` (preview path) — with parity enforced by a shared golden-fixture file. Wiring: a new `POST /api/marketplace/publish` dashboard route computes the manifest hash, generates the URI, mints via `IdentityClient::register`, then lists via `Erc8004MantleDriver::publish_listing`. No contract changes.

**Tech Stack:** Rust (alloy, axum, serde_json), TypeScript (React, vitest), existing deployed contracts on Mantle Sepolia.

**Spec:** `docs/superpowers/specs/2026-06-11-strategy-nft-genart-onchain-design.md` (read it first — §2–§6 are the normative algorithm; Appendix A is the palette roster).

**Two spec corrections to apply during Task 9** (discovered while grounding this plan):
1. §4 says "weighted bag of 14" — the weights sum to **13**. The normative bag is the 13-entry list in Task 1.
2. §7 places the identity pre-mint inside `publish_listing` — but `adapter.rs:282-285` deliberately keeps mint and listing separable ("Pre-mint agent registration … is the caller's responsibility"). The pre-mint is orchestrated by the **dashboard route** instead (Task 7). `PublishRequest` is NOT modified.

---

## Conventions for every task

- Work in an isolated worktree (`git worktree add .worktrees/genart-v3 -b feat/genart-v3-onchain`), `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"`.
- Rust: build/test through the guard wrapper: `scripts/cargo test -p xvision-identity`.
- Frontend: `cd frontend/web && npm test -- <pattern>` (vitest).
- Before any code: `bd create --title="Bitfields v3 genart onchain" --type=feature --priority=1` and `bd update <id> --claim`.

## The normative algorithm (referenced by Tasks 1 and 6)

Both implementations MUST follow this draw order exactly. All floats are f64.

```
seed_string   = agent_id + ":" + manifest_hash
traits_rng    = mulberry32(fnv1a32(seed_string))
  draw 1: palette_idx = floor(r() * 33)
  draw 2: symmetry    = BAG[floor(r() * 13)]
BAG (13) = [free, free, free, mirror-x, mirror-y, quad, quad, quad,
            diagonal, anti-diagonal, rot180, rot90, rot90]

grid_rng per attempt = mulberry32(fnv1a32(seed_string + suffix))
  suffix: "" then "#1" "#2" "#3" "#4" then "#final"
raw grid: N=28, layers=6, states=6, transparent starts at 7
  per layer L, draws IN ORDER: op_idx=floor(r()*3) over [AND,XOR,OR];
    band=1+floor(r()*7); base=2+floor(r()*9); xo=floor(r()*64); yo=floor(r()*64);
    radial = r() > 0.7; invert = r() > 0.8
  per cell (x,y): cx=N/2+xo, cy=N/2+yo
    step = radial ? floor(sqrt((x+xo-cx)^2 + (y+yo-cy)^2) / band)   // sqrt, NOT hypot
                  : floor((y+yo) / band)
    t = base + step
    v = op(x+y+xo, y-x+yo) on 32-bit ints; if invert: v = !v (bitwise NOT)
    v = ((v mod t) + t) mod t          // t > 0; v may be negative pre-mod
    s = v mod (states + transparent); if s < transparent: skip
    grid[y][x] = (s - transparent) mod 7        // 7 = palette length
density floor: if filled/(28*28) < 0.14 retry next suffix with transparent = max(2, transparent-2);
  after 5 attempts use suffix "#final" with transparent=2 unconditionally.
display grid: g[x][y] = raw[canon(x,y)] per symmetry table (spec §4, plus anti-diagonal:
  x+y > N-1 ? (N-1-y, N-1-x) : (x,y))
density attribute = round(100 * filled_display / 784)
```

Integer/bitwise semantics: all bitwise ops on 32-bit two's-complement (JS `|&^~` semantics; Rust: cast to `i32`, use `wrapping` ops). `fnv1a32` = `h=2166136261u32; for byte: h^=byte; h=h.wrapping_mul(16777619)`. `mulberry32(seed u32)`: `s=s.wrapping_add(0x6D2B79F5); t=s; t=(t^(t>>15)).wrapping_mul(t|1); t^=t.wrapping_add((t^(t>>7)).wrapping_mul(t|61)); ((t^(t>>14)) as f64)/4294967296.0` (all on u32, `>>` logical).

Input validation (both impls): `agent_id` must match `^[0-9A-Za-z_-]{1,64}$`; `manifest_hash` must match `^[0-9a-f]{64}$`. Violation → throw/Err. **No silent fallback.**

---

### Task 1: TS engine — `genartGrid.ts`

**Files:**
- Create: `frontend/web/src/features/marketplace/lib/genartGrid.ts`
- Test: `frontend/web/src/features/marketplace/lib/genartGrid.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
// genartGrid.test.ts
import { describe, expect, it } from "vitest";
import {
  N, PALETTES, buildGrid, buildGridFromSeedString, fnv1a32, mulberry32,
} from "./genartGrid";

const HASH = "a".repeat(64);

describe("primitives", () => {
  it("fnv1a32 matches reference vectors", () => {
    expect(fnv1a32("")).toBe(2166136261);
    expect(fnv1a32("a")).toBe(0xe40c292c);
    expect(fnv1a32("foobar")).toBe(0xbf9cf968);
  });
  it("mulberry32 is deterministic", () => {
    const a = mulberry32(123), b = mulberry32(123);
    for (let i = 0; i < 16; i++) expect(a()).toBe(b());
  });
});

describe("buildGrid", () => {
  it("is deterministic", () => {
    const a = buildGrid("01HXVNAAAA", HASH);
    const b = buildGrid("01HXVNAAAA", HASH);
    expect(Array.from(a.grid)).toEqual(Array.from(b.grid));
    expect(a.traits).toEqual(b.traits);
  });
  it("validates inputs loudly", () => {
    expect(() => buildGrid("", HASH)).toThrow();
    expect(() => buildGrid("ok", "xyz")).toThrow();
    expect(() => buildGrid("bad id!", HASH)).toThrow();
  });
  it("meets the 14% density floor for 500 seeds", () => {
    for (let i = 0; i < 500; i++) {
      const { grid } = buildGridFromSeedString(`floor-test-${i}`);
      const filled = Array.from(grid).filter((v) => v >= 0).length;
      expect(filled / (N * N)).toBeGreaterThanOrEqual(0.14);
    }
  });
  it("traits come from the locked roster and bag", () => {
    const { traits } = buildGrid("01HXVNAAAA", HASH);
    expect(PALETTES.map(([n]) => n)).toContain(traits.palette);
    expect(["free","mirror-x","mirror-y","quad","diagonal","anti-diagonal","rot180","rot90"])
      .toContain(traits.symmetry);
    expect(traits.layers).toBe(6);
  });
  it("symmetry laws hold", () => {
    // hunt one seed per mode, then assert its invariant
    const seen = new Map<string, Int8Array>();
    for (let i = 0; seen.size < 8 && i < 4000; i++) {
      const { grid, traits } = buildGridFromSeedString(`law-${i}`);
      if (!seen.has(traits.symmetry)) seen.set(traits.symmetry, grid);
    }
    const g = (grid: Int8Array, x: number, y: number) => grid[y * N + x];
    const quad = seen.get("quad")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
      expect(g(quad, x, y)).toBe(g(quad, N - 1 - x, y));
      expect(g(quad, x, y)).toBe(g(quad, x, N - 1 - y));
    }
    const diag = seen.get("diagonal")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(diag, x, y)).toBe(g(diag, y, x));
    const anti = seen.get("anti-diagonal")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(anti, x, y)).toBe(g(anti, N - 1 - y, N - 1 - x));
    const r180 = seen.get("rot180")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(r180, x, y)).toBe(g(r180, N - 1 - x, N - 1 - y));
    const r90 = seen.get("rot90")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(r90, x, y)).toBe(g(r90, y, N - 1 - x));
  });
  it("PALETTES has 33 entries of 7 colors", () => {
    expect(PALETTES.length).toBe(33);
    for (const [, cols] of PALETTES) {
      expect(cols.length).toBe(7);
      for (const c of cols) expect(c).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd frontend/web && npm test -- genartGrid`
Expected: FAIL — module `./genartGrid` not found.

- [ ] **Step 3: Implement `genartGrid.ts`**

```ts
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
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd frontend/web && npm test -- genartGrid`
Expected: PASS (all). If the fnv1a32 vectors fail, the implementation is wrong, not the vectors — `fnv1a32("a") === 0xe40c292c` and `fnv1a32("foobar") === 0xbf9cf968` are published FNV-1a test vectors.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/lib/genartGrid.ts \
        frontend/web/src/features/marketplace/lib/genartGrid.test.ts
git commit -m "feat(genart): bitfields v3 TS engine — grid, traits, symmetry, density floor"
```

---

### Task 2: TS SVG + tokenURI — rewrite `genart.ts`

**Files:**
- Modify: `frontend/web/src/features/marketplace/lib/genart.ts` (full rewrite; keep `base64Encode` helper)
- Test: `frontend/web/src/features/marketplace/lib/genart.test.ts` (create; if an old test file exists for v1 behavior, replace it)

- [ ] **Step 1: Write the failing tests**

```ts
// genart.test.ts
import { describe, expect, it } from "vitest";
import { N, buildGrid } from "./genartGrid";
import { generateSvg, generateTokenUri } from "./genart";

const HASH = "b".repeat(64);
const AID = "01HXVNTESTAGENT";

function decodeRects(svg: string): Int8Array {
  // RLE round-trip: parse rects back into a grid using fill->index from buildGrid's palette
  const { palette } = buildGrid(AID, HASH);
  const grid = new Int8Array(N * N).fill(-1);
  const re = /<rect x="(\d+)" y="(\d+)" width="(\d+)" height="1" fill="(#[0-9a-f]{6})"\/>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(svg))) {
    const [, xs, ys, ws, fill] = m;
    const idx = palette.indexOf(fill);
    expect(idx).toBeGreaterThanOrEqual(0);
    for (let dx = 0; dx < Number(ws); dx++) grid[Number(ys) * N + Number(xs) + dx] = idx;
  }
  return grid;
}

describe("generateSvg", () => {
  it("round-trips RLE back to the display grid", () => {
    const { grid } = buildGrid(AID, HASH);
    const svg = generateSvg(AID, HASH);
    expect(Array.from(decodeRects(svg))).toEqual(Array.from(grid));
  });
  it("has the normative envelope", () => {
    const svg = generateSvg(AID, HASH);
    expect(svg.startsWith(
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 28 28" width="560" height="560" shape-rendering="crispEdges">`,
    )).toBe(true);
    expect(svg.endsWith("</svg>")).toBe(true);
    expect(svg).not.toContain("\n");
  });
  it("rejects bad input", () => {
    expect(() => generateSvg("", HASH)).toThrow();
    expect(() => generateSvg(AID, "nothex")).toThrow();
  });
});

describe("generateTokenUri", () => {
  it("emits base64 JSON with traits and stays under the 12KB ceiling", () => {
    const uri = generateTokenUri(AID, HASH);
    expect(uri.startsWith("data:application/json;base64,")).toBe(true);
    const json = JSON.parse(atob(uri.slice("data:application/json;base64,".length)));
    expect(json.name).toBe(`xvn strategy ${AID.slice(0, 8)}`);
    expect(json.agent_id).toBe(AID);
    expect(json.image.startsWith("data:image/svg+xml;base64,")).toBe(true);
    const types = json.attributes.map((a: { trait_type: string }) => a.trait_type);
    expect(types).toEqual(["Symmetry", "Palette", "Density", "Layers"]);
    expect(uri.length).toBeLessThanOrEqual(12 * 1024);
  });
  it("size ceiling holds across 300 seeds", () => {
    for (let i = 0; i < 300; i++) {
      const h = i.toString(16).padStart(64, "0");
      expect(generateTokenUri("01HXVNSZ", h).length).toBeLessThanOrEqual(12 * 1024);
    }
  });
});
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd frontend/web && npm test -- "lib/genart\.test"`
Expected: FAIL — `generateSvg` signature/behavior is still v1 (hash-shapes).

- [ ] **Step 3: Rewrite `genart.ts`**

Replace the entire file (the v1 hash-shapes generator is retired):

```ts
// genart.ts — Bitfields v3 SVG + tokenURI. Byte-identical twin of
// crates/xvision-identity/src/genart.rs; parity enforced by tests/fixtures/genart_v3.json.
import { N, buildGrid, type Traits } from "./genartGrid";

function base64Encode(data: Uint8Array): string {
  const CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const n = data.length;
  const full = Math.floor(n / 3);
  const rem = n % 3;
  let out = "";
  for (let i = 0; i < full; i++) {
    const b = (data[i * 3] << 16) | (data[i * 3 + 1] << 8) | data[i * 3 + 2];
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + CHARS[b & 0x3f];
  }
  if (rem === 1) {
    const b = data[full * 3] << 16;
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + "==";
  } else if (rem === 2) {
    const b = (data[full * 3] << 16) | (data[full * 3 + 1] << 8);
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + "=";
  }
  return out;
}

export function deriveTraits(agentId: string, manifestHash: string): Traits {
  return buildGrid(agentId, manifestHash).traits;
}

export function generateSvg(agentId: string, manifestHash: string): string {
  const { grid, palette } = buildGrid(agentId, manifestHash);
  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${N} ${N}" width="560" height="560" shape-rendering="crispEdges">`,
    `<rect width="${N}" height="${N}" fill="${palette[0]}"/>`,
  ];
  for (let y = 0; y < N; y++) {
    let x = 0;
    while (x < N) {
      const v = grid[y * N + x];
      if (v < 0) { x++; continue; }
      let x2 = x + 1;
      while (x2 < N && grid[y * N + x2] === v) x2++;
      parts.push(`<rect x="${x}" y="${y}" width="${x2 - x}" height="1" fill="${palette[v]}"/>`);
      x = x2;
    }
  }
  parts.push("</svg>");
  return parts.join("");
}

export function generateTokenUri(agentId: string, manifestHash: string): string {
  const { traits } = buildGrid(agentId, manifestHash);
  const svg = generateSvg(agentId, manifestHash);
  const svgB64 = base64Encode(new TextEncoder().encode(svg));
  const short = agentId.substring(0, 8);
  // Field order is normative — the Rust twin emits the identical byte string.
  const json =
    `{"name":"xvn strategy ${short}",` +
    `"image":"data:image/svg+xml;base64,${svgB64}",` +
    `"agent_id":"${agentId}",` +
    `"attributes":[` +
    `{"trait_type":"Symmetry","value":"${traits.symmetry}"},` +
    `{"trait_type":"Palette","value":"${traits.palette}"},` +
    `{"trait_type":"Density","value":${traits.density}},` +
    `{"trait_type":"Layers","value":${traits.layers}}]}`;
  return `data:application/json;base64,${base64Encode(new TextEncoder().encode(json))}`;
}
```

- [ ] **Step 4: Run tests + check for broken v1 importers**

Run: `cd frontend/web && npm test -- "lib/genart"` — expected: PASS.
Run: `grep -rn "from .*lib/genart'" src/ --include="*.ts*" | grep -v test` — v1 exports were never imported outside tests; if anything new shows up, fix the import.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/lib/genart.ts \
        frontend/web/src/features/marketplace/lib/genart.test.ts
git commit -m "feat(genart): bitfields v3 TS SVG/tokenURI, retire v1 hash-shapes"
```

---

### Task 3: Golden fixtures (parity contract)

**Files:**
- Create: `crates/xvision-identity/tests/fixtures/genart_v3.json` (generated)
- Create: `frontend/web/src/features/marketplace/lib/genartFixtures.test.ts`

- [ ] **Step 1: Write the fixture generator/golden test**

```ts
// genartFixtures.test.ts — golden parity contract with the Rust twin.
// Regenerate with: REGEN_GENART_FIXTURES=1 npm test -- genartFixtures
import { describe, expect, it } from "vitest";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { deriveTraits, generateSvg, generateTokenUri } from "./genart";

const FIXTURE = resolve(
  __dirname,
  "../../../../../../crates/xvision-identity/tests/fixtures/genart_v3.json",
);

const SEEDS: Array<[string, string]> = Array.from({ length: 24 }, (_, i) => [
  `01HXVNFIX${i.toString(36).toUpperCase().padStart(2, "0")}`,
  // deterministic synthetic manifest hashes (any 64-hex is valid input)
  (BigInt(i + 1) * 0x9e3779b97f4a7c15n).toString(16).padStart(64, "0").slice(0, 64),
]);

describe("genart v3 golden fixtures", () => {
  it("matches (or regenerates) the fixture file", () => {
    const computed = SEEDS.map(([agentId, manifestHash]) => ({
      agent_id: agentId,
      manifest_hash: manifestHash,
      traits: deriveTraits(agentId, manifestHash),
      svg: generateSvg(agentId, manifestHash),
      token_uri: generateTokenUri(agentId, manifestHash),
    }));
    if (process.env.REGEN_GENART_FIXTURES) {
      writeFileSync(FIXTURE, JSON.stringify(computed, null, 2) + "\n");
      return;
    }
    const golden = JSON.parse(readFileSync(FIXTURE, "utf8"));
    expect(computed).toEqual(golden);
  });
});
```

- [ ] **Step 2: Generate the fixture, then verify the golden test passes**

```bash
cd frontend/web
REGEN_GENART_FIXTURES=1 npm test -- genartFixtures   # writes the file
npm test -- genartFixtures                            # PASS against it
```

- [ ] **Step 3: Eyeball the fixture**

Run: `jq '[.[].traits.symmetry] | group_by(.) | map({mode: .[0], n: length})' crates/xvision-identity/tests/fixtures/genart_v3.json`
Expected: several modes represented (sanity that the bag works). Also `jq '[.[].token_uri | length] | max'` ≤ 12288.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-identity/tests/fixtures/genart_v3.json \
        frontend/web/src/features/marketplace/lib/genartFixtures.test.ts
git commit -m "test(genart): golden fixture parity contract (24 seeds)"
```

---### Task 4: Rust twin — rewrite `genart.rs`

**Files:**
- Modify: `crates/xvision-identity/src/genart.rs` (full rewrite; keep `base64_encode` and `hex` helpers)
- Modify: `crates/xvision-identity/src/lib.rs:43` (exports — add `derive_traits`, `Traits`, `manifest_hash_hex`)
- Modify: `crates/xvision-identity/tests/genart.rs` (replace v1 tests)

- [ ] **Step 1: Replace the integration tests**

```rust
// tests/genart.rs
use xvision_identity::{derive_traits, generate_svg, generate_token_uri};

#[derive(serde::Deserialize)]
struct Fixture {
    agent_id: String,
    manifest_hash: String,
    traits: FixtureTraits,
    svg: String,
    token_uri: String,
}
#[derive(serde::Deserialize)]
struct FixtureTraits {
    palette: String,
    symmetry: String,
    density: u32,
    layers: u32,
}

fn fixtures() -> Vec<Fixture> {
    let raw = include_str!("fixtures/genart_v3.json");
    serde_json::from_str(raw).expect("fixture parses")
}

#[test]
fn golden_parity_with_ts() {
    for f in fixtures() {
        let svg = generate_svg(&f.agent_id, &f.manifest_hash).expect("svg");
        assert_eq!(svg, f.svg, "SVG parity failed for {}", f.agent_id);
        let uri = generate_token_uri(&f.agent_id, &f.manifest_hash).expect("uri");
        assert_eq!(uri, f.token_uri, "tokenURI parity failed for {}", f.agent_id);
        let t = derive_traits(&f.agent_id, &f.manifest_hash).expect("traits");
        assert_eq!(t.palette, f.traits.palette);
        assert_eq!(t.symmetry.as_str(), f.traits.symmetry);
        assert_eq!(t.density, f.traits.density);
        assert_eq!(t.layers, f.traits.layers);
    }
}

#[test]
fn density_floor_holds_for_1000_seeds() {
    let hash = "c".repeat(64);
    for i in 0..1000 {
        let t = derive_traits(&format!("01HXVNPROP{i}"), &hash).expect("traits");
        assert!(t.density >= 14, "seed {i} density {} below floor", t.density);
    }
}

#[test]
fn token_uri_size_ceiling() {
    let hash = "d".repeat(64);
    for i in 0..300 {
        let uri = generate_token_uri(&format!("01HXVNSZ{i}"), &hash).expect("uri");
        assert!(uri.len() <= 12 * 1024, "seed {i}: {} bytes", uri.len());
    }
}

#[test]
fn invalid_input_fails_loudly() {
    assert!(generate_token_uri("", &"a".repeat(64)).is_err());
    assert!(generate_token_uri("ok", "nothex").is_err());
    assert!(generate_token_uri("bad id!", &"a".repeat(64)).is_err());
    assert!(generate_token_uri("ok", &"A".repeat(64)).is_err(), "uppercase hex rejected");
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `scripts/cargo test -p xvision-identity --test genart`
Expected: FAIL — compile errors (`derive_traits` missing; `generate_svg` is infallible v1).

- [ ] **Step 3: Rewrite `genart.rs`**

The full module. Mirror the TS twin exactly — same draw order, same `u32`/`i32` semantics, same string formats:

```rust
//! Bitfields v3 generative identity art. NORMATIVE: spec 2026-06-11 §2-§6.
//! Byte-identical twin of frontend/web/src/features/marketplace/lib/{genartGrid,genart}.ts;
//! parity enforced by tests/fixtures/genart_v3.json. Any change must update both twins
//! and regenerate fixtures.

use std::fmt::Write as FmtWrite;

pub const N: usize = 28;
const LAYERS: u32 = 6;
const STATES: i32 = 6;
const PAL_LEN: i32 = 7;
const DENSITY_FLOOR: f64 = 0.14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symmetry {
    Free, MirrorX, MirrorY, Quad, Diagonal, AntiDiagonal, Rot180, Rot90,
}

impl Symmetry {
    pub fn as_str(self) -> &'static str {
        match self {
            Symmetry::Free => "free",
            Symmetry::MirrorX => "mirror-x",
            Symmetry::MirrorY => "mirror-y",
            Symmetry::Quad => "quad",
            Symmetry::Diagonal => "diagonal",
            Symmetry::AntiDiagonal => "anti-diagonal",
            Symmetry::Rot180 => "rot180",
            Symmetry::Rot90 => "rot90",
        }
    }
}

const SYMMETRY_BAG: [Symmetry; 13] = [
    Symmetry::Free, Symmetry::Free, Symmetry::Free,
    Symmetry::MirrorX, Symmetry::MirrorY,
    Symmetry::Quad, Symmetry::Quad, Symmetry::Quad,
    Symmetry::Diagonal, Symmetry::AntiDiagonal, Symmetry::Rot180,
    Symmetry::Rot90, Symmetry::Rot90,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traits {
    pub palette: String,
    pub symmetry: Symmetry,
    pub density: u32,
    pub layers: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum GenartError {
    #[error("invalid agent_id: must match ^[0-9A-Za-z_-]{{1,64}}$")]
    InvalidAgentId,
    #[error("invalid manifest_hash: must be 64-char lowercase hex")]
    InvalidManifestHash,
}

/// Locked roster — order is normative (spec Appendix A). Append-only post-launch.
pub const PALETTES: [(&str, [&str; 7]); 33] = [
    ("risoBlue", ["#0d1026","#1c2a6b","#2f4bb8","#3f6df2","#7fa3ff","#ffd23f","#fff6e0"]),
    ("risoRedTeal", ["#140a0d","#5c1a2e","#c1224f","#ff5470","#1ca3a3","#9fe3d4","#fff3e8"]),
    ("candyArcade", ["#0d0714","#2e1245","#5a1f7d","#9032a8","#e84393","#ffd24f","#fff5dc"]),
    ("circuit", ["#041013","#08242b","#0f5260","#18a98f","#a5f3dc","#ff3b73","#ffe95e"]),
    ("coldSignal", ["#071019","#102936","#23545b","#44a3a3","#c5e4dc","#f44465","#ffe6a7"]),
    ("grapeSoda", ["#0c0714","#231140","#41207a","#6a39b8","#9c6be0","#cda8f0","#f3e8fd"]),
    ("punolit", ["#11151f","#1e3442","#35665f","#89a36a","#d5c686","#df9d8b","#e8d7cf"]),
    ("calmSunset", ["#2c1534","#5c2751","#a94768","#df8584","#f3cda9","#f7e6b0","#fff7d6"]),
    ("lineage", ["#080916","#182044","#263b71","#426e91","#75a57d","#d2bc72","#f6ead0"]),
    ("signalRust", ["#0c0b0a","#26211c","#4f4138","#8a6450","#c97f4f","#f25c3a","#ffe9d4"]),
    ("magmaCore", ["#0c0508","#330a12","#70101c","#bf1f26","#f2542d","#ffa552","#ffe8c2"]),
    ("tidalDusk", ["#0a1012","#103035","#1a5e60","#2f9a8c","#e6c36b","#ef9f63","#fcefd2"]),
    ("ultraviolet", ["#08051a","#160d44","#2a1a80","#4730c4","#7a5ef2","#b49cff","#e9e2ff"]),
    ("voltYellow", ["#0a0a10","#1d2433","#2f4866","#4a7ab8","#7fb3e8","#ffe83f","#fdf8e2"]),
    ("mintMagenta", ["#070f0d","#0f2e26","#1a5c47","#2f9a73","#8fe0bb","#f23fa0","#fff0f7"]),
    ("tealEmber", ["#06100f","#0e3331","#176561","#2aa39a","#aee8df","#ff7733","#ffeed9"]),
    ("indigoCoral", ["#08081a","#161a4d","#2a2f8f","#4a55d6","#9aa3f2","#ff6f61","#fff1e8"]),
    ("limeViolet", ["#0b0d06","#222e0d","#3f5c14","#6f9a1f","#b8e040","#8a3ff2","#f4eaff"]),
    ("roseCyan", ["#120710","#3a0f2e","#73195c","#b8268f","#f060c4","#2ee6e6","#e8feff"]),
    ("amberInk", ["#0b0a12","#1f1d33","#3a3866","#5c59a8","#9a97d9","#ffb347","#fff3da"]),
    ("crimsonMint", ["#120709","#3f0d18","#7c142b","#c41f44","#f25c77","#5ce8b8","#eafff6"]),
    ("cobaltTangerine", ["#06091a","#0e1f56","#1a3b9e","#2f63e0","#85aaf2","#ff9433","#fff0dc"]),
    ("orchidLime", ["#100818","#2e1247","#5a2080","#9438c4","#d685f0","#cfe83f","#f9ffe0"]),
    ("pinkPitch", ["#0d0c0c","#1f1d1f","#3b373b","#6b6168","#b3a6ad","#ff3f8e","#ffe6f1"]),
    ("acidTeal", ["#0c1206","#1f330d","#3f6618","#6fa826","#b8e84a","#1fb8c9","#e0fbff"]),
    ("goldGrape", ["#0e0814","#291245","#4d1f7d","#7d33b8","#b370e0","#ffd23f","#fff6dc"]),
    ("rustTurquoise", ["#120b08","#3b1c10","#73331a","#b85426","#e88a4f","#2ec9b8","#e8fcf7"]),
    ("cherryCola", ["#100808","#330f12","#661a21","#a82a35","#e0525c","#ffc26b","#fff0d9"]),
    ("duskNeon", ["#0a0814","#1d1640","#352a73","#5444a8","#8a73d9","#3fffb8","#eafff5"]),
    ("peachAbyss", ["#050811","#0d1c3a","#173366","#2a52a3","#6f8fd9","#ffb38a","#fff0e2"]),
    ("saffronSea", ["#071013","#0f2c38","#1a5366","#2f85a3","#73c2d9","#ffc63f","#fff6da"]),
    ("furnacePink", ["#0f070c","#360d2b","#6e1452","#b81f7d","#f23fb0","#ffae3f","#ffeed4"]),
    ("glacierPunch", ["#070b10","#13283d","#234a73","#3f78b3","#8fc1e8","#f2543f","#ffe9e0"]),
];

fn fnv1a32(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

struct Mulberry32 {
    s: u32,
}
impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self { s: seed }
    }
    fn next(&mut self) -> f64 {
        self.s = self.s.wrapping_add(0x6d2b_79f5);
        let mut t = self.s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

fn raw_grid(seed_str: &str, transparent: i32) -> [i8; N * N] {
    let mut r = Mulberry32::new(fnv1a32(seed_str));
    let mut grid = [-1i8; N * N];
    for _layer in 0..LAYERS {
        let op_idx = (r.next() * 3.0).floor() as u32;
        let band = 1 + (r.next() * 7.0).floor() as i32;
        let base = 2 + (r.next() * 9.0).floor() as i32;
        let xo = (r.next() * 64.0).floor() as i32;
        let yo = (r.next() * 64.0).floor() as i32;
        let radial = r.next() > 0.7;
        let invert = r.next() > 0.8;
        let cx = N as f64 / 2.0 + xo as f64;
        let cy = N as f64 / 2.0 + yo as f64;
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let step = if radial {
                    let dx = (x + xo) as f64 - cx;
                    let dy = (y + yo) as f64 - cy;
                    ((dx * dx + dy * dy).sqrt() / band as f64).floor() as i32
                } else {
                    (((y + yo) as f64) / band as f64).floor() as i32
                };
                let t = base + step;
                let a = x + y + xo;
                let b = y - x + yo;
                let mut v = match op_idx {
                    0 => a & b,
                    1 => a ^ b,
                    _ => a | b,
                };
                if invert {
                    v = !v;
                }
                v = ((v % t) + t) % t;
                let s = v % (STATES + transparent);
                if s < transparent {
                    continue;
                }
                grid[(y as usize) * N + x as usize] = ((s - transparent) % PAL_LEN) as i8;
            }
        }
    }
    grid
}

fn filled_ratio(grid: &[i8; N * N]) -> f64 {
    grid.iter().filter(|&&v| v >= 0).count() as f64 / (N * N) as f64
}

fn dense_grid(seed_str: &str) -> [i8; N * N] {
    let mut transparent = 7i32;
    for attempt in 0..5 {
        let s = if attempt == 0 {
            seed_str.to_string()
        } else {
            format!("{seed_str}#{attempt}")
        };
        let g = raw_grid(&s, transparent);
        if filled_ratio(&g) >= DENSITY_FLOOR {
            return g;
        }
        transparent = (transparent - 2).max(2);
    }
    raw_grid(&format!("{seed_str}#final"), 2)
}

fn canon(mode: Symmetry, x: usize, y: usize) -> (usize, usize) {
    let n = N - 1;
    match mode {
        Symmetry::Free => (x, y),
        Symmetry::MirrorX => (x.min(n - x), y),
        Symmetry::MirrorY => (x, y.min(n - y)),
        Symmetry::Quad => (x.min(n - x), y.min(n - y)),
        Symmetry::Diagonal => if x < y { (y, x) } else { (x, y) },
        Symmetry::AntiDiagonal => if x + y > n { (n - y, n - x) } else { (x, y) },
        Symmetry::Rot180 => {
            if y * N + x <= (n - y) * N + (n - x) { (x, y) } else { (n - x, n - y) }
        }
        Symmetry::Rot90 => {
            let (mut bx, mut by, mut bi) = (x, y, y * N + x);
            let (mut cx, mut cy) = (x, y);
            for _ in 0..3 {
                let (nx, ny) = (cy, n - cx);
                cx = nx;
                cy = ny;
                let i = cy * N + cx;
                if i < bi {
                    bi = i;
                    bx = cx;
                    by = cy;
                }
            }
            (bx, by)
        }
    }
}

fn validate(agent_id: &str, manifest_hash: &str) -> Result<(), GenartError> {
    let id_ok = !agent_id.is_empty()
        && agent_id.len() <= 64
        && agent_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !id_ok {
        return Err(GenartError::InvalidAgentId);
    }
    let hash_ok = manifest_hash.len() == 64
        && manifest_hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !hash_ok {
        return Err(GenartError::InvalidManifestHash);
    }
    Ok(())
}

struct Built {
    grid: [i8; N * N],
    palette: [&'static str; 7],
    traits: Traits,
}

fn build(agent_id: &str, manifest_hash: &str) -> Result<Built, GenartError> {
    validate(agent_id, manifest_hash)?;
    let seed_string = format!("{agent_id}:{manifest_hash}");
    let mut r = Mulberry32::new(fnv1a32(&seed_string));
    let (pal_name, palette) = PALETTES[(r.next() * PALETTES.len() as f64).floor() as usize];
    let symmetry = SYMMETRY_BAG[(r.next() * SYMMETRY_BAG.len() as f64).floor() as usize];
    let raw = dense_grid(&seed_string);
    let mut grid = [-1i8; N * N];
    let mut filled = 0u32;
    for y in 0..N {
        for x in 0..N {
            let (sx, sy) = canon(symmetry, x, y);
            let v = raw[sy * N + sx];
            grid[y * N + x] = v;
            if v >= 0 {
                filled += 1;
            }
        }
    }
    let density = ((100.0 * filled as f64) / (N * N) as f64).round() as u32;
    Ok(Built {
        grid,
        palette,
        traits: Traits { palette: pal_name.to_string(), symmetry, density, layers: LAYERS },
    })
}

pub fn derive_traits(agent_id: &str, manifest_hash: &str) -> Result<Traits, GenartError> {
    Ok(build(agent_id, manifest_hash)?.traits)
}

pub fn generate_svg(agent_id: &str, manifest_hash: &str) -> Result<String, GenartError> {
    let b = build(agent_id, manifest_hash)?;
    let mut s = String::with_capacity(8192);
    write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {N} {N}" width="560" height="560" shape-rendering="crispEdges">"#
    )
    .unwrap();
    write!(s, r#"<rect width="{N}" height="{N}" fill="{}"/>"#, b.palette[0]).unwrap();
    for y in 0..N {
        let mut x = 0;
        while x < N {
            let v = b.grid[y * N + x];
            if v < 0 {
                x += 1;
                continue;
            }
            let mut x2 = x + 1;
            while x2 < N && b.grid[y * N + x2] == v {
                x2 += 1;
            }
            write!(
                s,
                r#"<rect x="{x}" y="{y}" width="{}" height="1" fill="{}"/>"#,
                x2 - x,
                b.palette[v as usize]
            )
            .unwrap();
            x = x2;
        }
    }
    s.push_str("</svg>");
    Ok(s)
}

pub fn generate_token_uri(agent_id: &str, manifest_hash: &str) -> Result<String, GenartError> {
    let b = build(agent_id, manifest_hash)?;
    let svg = generate_svg(agent_id, manifest_hash)?;
    let svg_b64 = base64_encode(svg.as_bytes());
    let short = &agent_id[..agent_id.len().min(8)];
    let json = format!(
        r#"{{"name":"xvn strategy {short}","image":"data:image/svg+xml;base64,{svg_b64}","agent_id":"{agent_id}","attributes":[{{"trait_type":"Symmetry","value":"{}"}},{{"trait_type":"Palette","value":"{}"}},{{"trait_type":"Density","value":{}}},{{"trait_type":"Layers","value":{}}}]}}"#,
        b.traits.symmetry.as_str(),
        b.traits.palette,
        b.traits.density,
        b.traits.layers,
    );
    Ok(format!("data:application/json;base64,{}", base64_encode(json.as_bytes())))
}

/// keccak256 over the canonical JSON encoding of a strategy manifest, as
/// lowercase hex — the `manifest_hash` input to the generator and the
/// `contentHash` stored by `ListingRegistry.createListing`.
pub fn manifest_hash_hex(canonical_json: &str) -> String {
    use alloy::primitives::keccak256;
    let h = keccak256(canonical_json.as_bytes());
    let mut out = String::with_capacity(64);
    for b in h.0 {
        write!(out, "{b:02x}").unwrap();
    }
    out
}

fn base64_encode(data: &[u8]) -> String {
    // keep the existing implementation from the v1 module verbatim
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let n = data.len();
    let full = n / 3;
    let rem = n % 3;
    let mut out = String::with_capacity((full + usize::from(rem != 0)) * 4);
    for i in 0..full {
        let b = ((data[i * 3] as u32) << 16) | ((data[i * 3 + 1] as u32) << 8) | (data[i * 3 + 2] as u32);
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(b & 0x3f) as usize] as char);
    }
    if rem == 1 {
        let b = (data[full * 3] as u32) << 16;
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((data[full * 3] as u32) << 16) | ((data[full * 3 + 1] as u32) << 8);
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}
```

Notes for the implementer:
- If `thiserror` is not already a dependency of `xvision-identity`, check `Cargo.toml`; it almost certainly is (the crate defines `IdentityError`) — if not, mirror however `IdentityError` is defined instead of adding a new dependency.
- `alloy::primitives::keccak256` is already available (the crate imports alloy throughout).
- In `lib.rs`, update the line `pub use genart::{generate_svg, generate_token_uri};` to `pub use genart::{derive_traits, generate_svg, generate_token_uri, manifest_hash_hex, GenartError, Symmetry, Traits};`

- [ ] **Step 4: Run the full identity test suite**

Run: `scripts/cargo test -p xvision-identity`
Expected: PASS, including `golden_parity_with_ts`. **If parity fails**, diff the first mismatching SVG against the fixture — the divergence is almost always (a) draw-order drift, (b) `Math.imul`/`wrapping_mul` mismatch, or (c) `%` semantics on negatives (JS `%` truncates toward zero like Rust — but only if `v` is `i32`, not `u32`).

- [ ] **Step 5: Fix any other v1 callers**

Run: `grep -rn "generate_svg\|generate_token_uri" crates/ --include="*.rs" | grep -v "genart\|tests"`
Expected: no production callers (v1 was tests-only). Fix anything that appears.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-identity/src/genart.rs crates/xvision-identity/src/lib.rs \
        crates/xvision-identity/tests/genart.rs
git commit -m "feat(genart): bitfields v3 Rust twin with golden parity + manifest_hash_hex"
```

---

### Task 5: `GenArtPlaceholder` renders the v3 engine

**Files:**
- Modify: `frontend/web/src/features/marketplace/components/GenArtPlaceholder.tsx`
- Modify: `frontend/web/src/features/marketplace/components/GenArtPlaceholder.test.tsx`

- [ ] **Step 1: Update the test**

Open the existing test file, keep its rendering-harness style, and make the assertions v3-aware. The component contract: same props (`seed`, `size`, `className`), `data-genart` flips to `"bitfields-v3"`, canvas internal resolution becomes `140` (28 cells × 5px):

```tsx
// Key assertions to have (adapt to the file's existing structure):
it("renders a canvas tagged bitfields-v3", () => {
  render(<GenArtPlaceholder seed="listing-abc" />);
  const canvas = screen.getByLabelText("strategy generative art");
  expect(canvas).toHaveAttribute("data-genart", "bitfields-v3");
  expect(canvas).toHaveAttribute("width", "140");
});
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd frontend/web && npm test -- GenArtPlaceholder`
Expected: FAIL on `data-genart`/`width`.

- [ ] **Step 3: Rewrite the component body**

Replace the file's drawing internals (`OPS`, `PALETTES`, `STUDIES`, `mkRng`, `fnv1a`, `pick`, `drawBitfield`) with the shared engine; keep the component shell:

```tsx
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
```

- [ ] **Step 4: Run component tests + the full frontend suite**

Run: `cd frontend/web && npm test -- GenArtPlaceholder` → PASS.
Run: `npm test` → all marketplace tests pass (callers pass `seed` strings; the engine accepts any string via `buildGridFromSeedString`).

- [ ] **Step 5: Visual smoke check**

Run the dashboard dev server, open `/marketplace` and eyeball ~20 cards: varied palettes, mix of free/mirrored/rotated structures, nothing near-empty. (This is the live surface — judge it.)

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/marketplace/components/GenArtPlaceholder.tsx \
        frontend/web/src/features/marketplace/components/GenArtPlaceholder.test.tsx
git commit -m "feat(marketplace): cards render bitfields v3 engine (preview == minted art)"
```

---

### Task 6: Dashboard publish route — mint + list

**Files:**
- Modify: `crates/xvision-dashboard/Cargo.toml` (add deps)
- Create: `crates/xvision-dashboard/src/routes/marketplace.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs` (register module)
- Modify: `crates/xvision-dashboard/src/server.rs` (~line 463, mutating router) (add route)
- Test: inline `#[cfg(test)]` in `marketplace.rs` + one route test alongside existing server tests

- [ ] **Step 1: Add dependencies**

In `crates/xvision-dashboard/Cargo.toml` under the existing path deps:

```toml
xvision-identity    = { path = "../xvision-identity" }
xvision-marketplace = { path = "../xvision-marketplace" }
url                 = "2"
```

(Use the workspace's existing `url` version if it's a workspace dep — check `Cargo.toml` at the root first; prefer `url.workspace = true`.)

Run: `scripts/cargo check -p xvision-dashboard` — expected: compiles (unused deps warning at most).

- [ ] **Step 2: Write the failing tests**

In `crates/xvision-dashboard/src/routes/marketplace.rs` (bottom of the new file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_env_missing_is_none() {
        // Clear env inside the test scope (serial-safe: unique var names are read fresh)
        std::env::remove_var("XVN_RPC_URL");
        assert!(ChainEnv::from_env().is_none());
    }

    #[test]
    fn tier_mapping() {
        assert_eq!(tier_code("open").unwrap(), 0);
        assert_eq!(tier_code("sealed").unwrap(), 1);
        assert!(tier_code("bogus").is_err());
    }

    #[test]
    fn price_to_usdc6() {
        assert_eq!(usdc6(49.0).unwrap().to_string(), "49000000");
        assert_eq!(usdc6(0.5).unwrap().to_string(), "500000");
        assert!(usdc6(-1.0).is_err());
    }
}
```

Run: `scripts/cargo test -p xvision-dashboard marketplace` — expected: FAIL (module missing).

- [ ] **Step 3: Implement the route**

```rust
//! `POST /api/marketplace/publish` — mint the strategy identity NFT with its
//! genart tokenURI, then create the marketplace listing.
//!
//! Chain access is env-gated (same convention as the adapter's anvil tests):
//!   XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK, XVN_IDENTITY_REGISTRY,
//!   plus the XVN_* marketplace addresses read by MarketplaceAddresses::from_env().
//! Without them the route answers 503 so dev environments degrade loudly.

use alloy::primitives::U256;
use alloy::signers::local::PrivateKeySigner;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::DashboardError;
use crate::state::AppState;
use xvision_engine::autooptimizer::content_hash::canonical_json;
use xvision_identity::{generate_token_uri, manifest_hash_hex, IdentityClient, MarketplaceAddresses, RegistryAddresses};
use xvision_marketplace::{Erc8004MantleDriver, PublishRequest};

#[derive(Debug, Deserialize)]
pub struct PublishBody {
    pub strategy_id: String,
    /// "open" | "sealed"
    pub tier: String,
    pub price_usdc: f64,
    #[serde(default)]
    pub transferable_license: bool,
}

#[derive(Debug, Serialize)]
pub struct PublishOut {
    pub agent_id: String,
    pub manifest_hash: String,
    pub token_id: String,
    pub listing_id: String,
    pub token_uri_bytes: usize,
}

pub(crate) struct ChainEnv {
    pub rpc_url: String,
    pub chain_id: u64,
    pub publisher_pk: String,
}

impl ChainEnv {
    pub(crate) fn from_env() -> Option<Self> {
        Some(Self {
            rpc_url: std::env::var("XVN_RPC_URL").ok()?,
            chain_id: std::env::var("XVN_CHAIN_ID").ok()?.parse().ok()?,
            publisher_pk: std::env::var("XVN_PUBLISHER_PK").ok()?,
        })
    }
}

pub(crate) fn tier_code(tier: &str) -> Result<u8, DashboardError> {
    match tier {
        "open" => Ok(0),
        "sealed" => Ok(1),
        other => Err(DashboardError::Validation {
            message: format!("unknown tier: {other}"),
        }),
    }
}

pub(crate) fn usdc6(price: f64) -> Result<U256, DashboardError> {
    if !(price.is_finite() && price >= 0.0) {
        return Err(DashboardError::Validation {
            message: "price_usdc must be a non-negative number".into(),
        });
    }
    Ok(U256::from((price * 1_000_000.0).round() as u64))
}

pub async fn post_publish(
    State(state): State<AppState>,
    Json(body): Json<PublishBody>,
) -> Result<(StatusCode, Json<PublishOut>), DashboardError> {
    // 1. Load the strategy and derive its canonical manifest hash.
    let strategy = crate::strategy::get(&state.api_context(), &body.strategy_id).await?;
    let manifest_value = serde_json::to_value(&strategy).map_err(|e| DashboardError::Validation {
        message: format!("strategy serialization failed: {e}"),
    })?;
    let canonical = canonical_json(&manifest_value);
    let manifest_hash = manifest_hash_hex(&canonical);
    let agent_id = body.strategy_id.clone(); // agent_id IS the strategy's pre-mint ULID

    // 2. Generate the onchain tokenURI (validates inputs; fails loudly).
    let token_uri = generate_token_uri(&agent_id, &manifest_hash).map_err(|e| {
        DashboardError::Validation { message: format!("genart generation failed: {e}") }
    })?;

    // 3. Chain env gate.
    let Some(chain) = ChainEnv::from_env() else {
        return Err(DashboardError::Unavailable {
            message: "chain env not configured (XVN_RPC_URL/XVN_CHAIN_ID/XVN_PUBLISHER_PK)".into(),
        });
    };
    let signer: PrivateKeySigner = chain.publisher_pk.parse().map_err(|_| {
        DashboardError::Validation { message: "XVN_PUBLISHER_PK is not a valid private key".into() }
    })?;

    // 4. Mint the identity NFT with the genart URI.
    let registry_addresses = RegistryAddresses::from_env().ok_or_else(|| DashboardError::Unavailable {
        message: "XVN_IDENTITY_REGISTRY (+reputation) not configured".into(),
    })?;
    let identity = IdentityClient::connect(&chain.rpc_url, registry_addresses, chain.chain_id)
        .await
        .map_err(|e| DashboardError::Upstream { message: format!("identity connect: {e}") })?;
    let uri = url::Url::parse(&token_uri).map_err(|e| DashboardError::Validation {
        message: format!("token URI parse: {e}"),
    })?;
    let token_id = identity
        .register(&uri, &signer)
        .await
        .map_err(|e| DashboardError::Upstream { message: format!("identity register: {e}") })?;

    // 5. Create the listing referencing the freshly minted agent NFT.
    let mp_addresses = MarketplaceAddresses::from_env().ok_or_else(|| DashboardError::Unavailable {
        message: "XVN_* marketplace addresses not configured".into(),
    })?;
    let driver = Erc8004MantleDriver::new(chain.rpc_url.clone(), mp_addresses, signer.clone());
    let listing = driver
        .publish_listing(PublishRequest {
            agent_nft_id: token_id.as_u256(),
            content_hash: alloy::primitives::B256::from_slice(
                &alloy::hex::decode(&manifest_hash).expect("validated hex"),
            ),
            content_uri: format!("xvn://strategy/{agent_id}"),
            tier: tier_code(&body.tier)?,
            price_usdc: usdc6(body.price_usdc)?,
            transferable_license: body.transferable_license,
        })
        .await
        .map_err(|e| DashboardError::Upstream { message: format!("publish listing: {e}") })?;

    Ok((
        StatusCode::CREATED,
        Json(PublishOut {
            agent_id,
            manifest_hash,
            token_id: token_id.to_string(),
            listing_id: listing.listing_id.to_string(),
            token_uri_bytes: token_uri.len(),
        }),
    ))
}
```

**Implementer adaptation points (verify against the real code, do not guess):**
- `DashboardError` variants: open `crates/xvision-dashboard/src/error.rs` and use its actual variants for validation / 503 / upstream failure (the names `Validation`/`Unavailable`/`Upstream` above are illustrative — match what exists; if no 503-shaped variant exists, add one mapping to `StatusCode::SERVICE_UNAVAILABLE`).
- `Erc8004MantleDriver::new(...)` signature: open `adapter.rs` and match its real constructor (it may take a config struct).
- `TokenId` API: check `client.rs` for how to convert `TokenId` → `U256`/`String` (`as_u256()`/`to_string()` above are illustrative).
- `RegistryAddresses::from_env()`: check `contracts.rs:202+`; if only `MarketplaceAddresses::from_env()` exists, add the sibling or use `RegistryAddresses::custom(...)` from `XVN_IDENTITY_REGISTRY`/`XVN_REPUTATION_REGISTRY` env vars.
- `canonical_json` path: confirm `xvision_engine::autooptimizer::content_hash::canonical_json` is `pub` and re-exported; adjust the `use` path to reality.
- `content_uri`: `xvn://strategy/{agent_id}` is a placeholder scheme accepted by the contract (it stores any string); a real IPFS pointer is a later track.

- [ ] **Step 4: Register the route**

In `routes/mod.rs`: `pub mod marketplace;`
In `server.rs` mutating router (next to `.route("/api/strategies", post(strategies::post_create))` at ~line 463):

```rust
.route("/api/marketplace/publish", post(marketplace::post_publish))
```

- [ ] **Step 5: Run tests and build**

Run: `scripts/cargo test -p xvision-dashboard marketplace` → PASS (unit tests).
Run: `scripts/cargo build --workspace` → compiles clean.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-dashboard
git commit -m "feat(api): POST /api/marketplace/publish — genart mint + listing, env-gated"
```

---

### Task 7: Frontend — real `submitListing` + mint preview

**Files:**
- Create: `frontend/web/src/features/marketplace/data/publish.ts`
- Modify: `frontend/web/src/features/marketplace/data/MarketplaceData.ts:83-85`
- Modify: `frontend/web/src/features/marketplace/routes/SellRoute.tsx` (Step3 preview seed)
- Test: `frontend/web/src/features/marketplace/data/publish.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// publish.test.ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { publishListing } from "./publish";

describe("publishListing", () => {
  afterEach(() => vi.restoreAllMocks());

  it("POSTs the draft to /api/marketplace/publish and maps the TxRef", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({
        agent_id: "01HX", manifest_hash: "ab".repeat(32),
        token_id: "7", listing_id: "3", token_uri_bytes: 4200,
      }), { status: 201, headers: { "content-type": "application/json" } }),
    );
    const tx = await publishListing({
      strategyId: "01HX", tier: "open", priceUsdc: 49,
      acceptedPayers: { humans: true, agents: true },
      listable: [], ingredients: [], preview: {} as never,
    });
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/api/marketplace/publish");
    expect(JSON.parse(String(init?.body))).toMatchObject({
      strategy_id: "01HX", tier: "open", price_usdc: 49,
    });
    expect(tx.network).toBe("mantle-sepolia");
  });

  it("throws on a 503 (chain env unset) instead of faking success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 503 }));
    await expect(publishListing({ strategyId: "x", tier: "open", priceUsdc: 1 } as never))
      .rejects.toThrow();
  });
});
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd frontend/web && npm test -- publish.test` — expected: FAIL (module missing).

- [ ] **Step 3: Implement `publish.ts` and wire it**

```ts
// publish.ts — real publish path: backend mints the identity NFT with its
// genart tokenURI, then creates the listing. Throws on failure; no fake TXs.
import { apiFetch } from "../../../api/client";
import type { PublishDraft, TxRef } from "./types";

interface PublishOut {
  agent_id: string;
  manifest_hash: string;
  token_id: string;
  listing_id: string;
  token_uri_bytes: number;
}

export async function publishListing(d: PublishDraft): Promise<TxRef & { out: PublishOut }> {
  const out = await apiFetch<PublishOut>("/api/marketplace/publish", {
    method: "POST",
    body: JSON.stringify({
      strategy_id: d.strategyId,
      tier: d.tier,
      price_usdc: d.priceUsdc ?? 0,
      transferable_license: false,
    }),
  });
  return { txHash: out.listing_id, network: "mantle-sepolia", out };
}
```

(Adapt the import path/signature of `apiFetch` to `frontend/web/src/api/client.ts` — it already attaches the bearer token for POST. If `apiFetch` is not exported, export it or use the file's existing request helper. If `TxRef` rejects the extra `out` field, return plain `TxRef` and surface `out` via a second call-site-specific function.)

Wire into `FixtureMarketplaceData.submitListing` (MarketplaceData.ts:83):

```ts
  async submitListing(d: PublishDraft): Promise<TxRef> {
    return publishListing(d);
  }
```

with `import { publishListing } from "./publish";` at the top.

- [ ] **Step 4: Mint preview in SellRoute Step 3**

In `SellRoute.tsx` / its `Step3Preview` child, render the actual NFT art next to the confirm action, full-width inline (NO side panel — chat-rail rule), using the strategy id as seed:

```tsx
<GenArtPlaceholder seed={draft.strategyId} size={160} />
```

Caveat to leave as a code comment: the final minted seed is `{agent_id}:{manifest_hash}` computed server-side; the preview seeded by `strategyId` alone is **not** the minted art. If the backend exposes the manifest hash cheaply (it does — response of publish, but that's post-mint), keep the placeholder-seed preview and label it "art finalizes at mint". Do not add a popup/modal for this.

- [ ] **Step 5: Run the frontend suite**

Run: `cd frontend/web && npm test` — expected: PASS, including existing `SellRoute.test.tsx` (update its mocks if `submitListing` now hits `fetch` — stub `publishListing` or `fetch` in that test).

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/marketplace
git commit -m "feat(marketplace): real submitListing via /api/marketplace/publish + mint preview"
```

---

### Task 8: End-to-end verification on Mantle Sepolia

**Files:** none (verification only). Requires: `source .op_env`, the deployed addresses from `config/mantle-sepolia.toml`, and a funded test key.

- [ ] **Step 1: Static + unit gates**

```bash
scripts/cargo test --workspace
cd frontend/web && npm test && cd ../..
```
Expected: all green.

- [ ] **Step 2: Live publish smoke test (operator-assisted)**

```bash
source .op_env
export XVN_RPC_URL=https://rpc.sepolia.mantle.xyz
export XVN_CHAIN_ID=5003
export XVN_PUBLISHER_PK=$(op read "op://<vault>/<item>/private key")   # operator supplies
export XVN_IDENTITY_REGISTRY=0x1DE1ccb2bBB5e1dE856BA096698b1A97f4484Fe4
export XVN_LISTING_REGISTRY=0x64b5ae5B91CB2846e7dA8cE883f2023b98E2cD22
export XVN_MARKETPLACE=0x4b9450642f2b3Da248e90b4FEBaA8eCA87E78cE8
export XVN_LICENSE_TOKEN=0xF72BB0526FCdDee8E1c0b56c2DF21C95FE51F978
# start the dashboard server, then:
curl -s -X POST localhost:PORT/api/marketplace/publish \
  -H 'content-type: application/json' -H "authorization: Bearer $TOKEN" \
  -d '{"strategy_id":"<a real strategy ulid>","tier":"open","price_usdc":1}' | jq
```
Expected: 201 with `token_id`, `listing_id`, `token_uri_bytes` < 12288.

- [ ] **Step 3: Verify the art is actually onchain**

```bash
# tokenURI(token_id) via cast (or a tiny xvn helper if cast is unavailable):
cast call $XVN_IDENTITY_REGISTRY "tokenURI(uint256)(string)" <token_id> \
  --rpc-url $XVN_RPC_URL | sed 's/^data:application\/json;base64,//' | base64 -d | jq .
# pull the image field, decode, open it:
... | jq -r .image | sed 's/^data:image\/svg+xml;base64,//' | base64 -d > /tmp/nft.svg && open /tmp/nft.svg
```
Expected: valid JSON with `attributes`, and the SVG opens showing the same art the marketplace card renders for seed `{agent_id}:{manifest_hash}`.

- [ ] **Step 4: Confirm preview parity manually**

In the dashboard, find the strategy's card; confirm the rendered art for the minted listing visually matches `/tmp/nft.svg` (it will once card call-sites pass the real `{agent_id}:{manifest_hash}` seed; listing surfaces that have the hash should be updated opportunistically — at minimum the listing detail page for minted listings).

---

### Task 9: Docs, spec corrections, branch finish

**Files:**
- Modify: `docs/superpowers/specs/2026-06-11-strategy-nft-genart-onchain-design.md` (§4 "bag of 14" → "bag of 13"; §7 adapter row → orchestration in dashboard route, `PublishRequest` unchanged)
- Modify: `docs/testnft/code/README.md` (one paragraph: v3 unified family is the production engine; lanes parked)

- [ ] **Step 1: Apply both spec corrections** (wording only, table row for the adapter points at `routes/marketplace.rs`).

- [ ] **Step 2: Update README** with a short "Bitfields v3 (production)" section pointing at the spec and the two twin implementations.

- [ ] **Step 3: Full final verification**

```bash
scripts/cargo test --workspace && (cd frontend/web && npm test)
```

- [ ] **Step 4: Commit, close the bead, push, PR**

```bash
git add docs/
git commit -m "docs: genart v3 spec corrections + testnft README pointer"
bd close <bead-id>
git push -u origin feat/genart-v3-onchain
gh pr create --title "feat: bitfields v3 onchain genart — engine twins, card renderer, mint wiring" \
  --body "Implements docs/superpowers/specs/2026-06-11-strategy-nft-genart-onchain-design.md ..."
```

Use superpowers:finishing-a-development-branch for the merge decision.

---

## Self-review notes (already applied)

- Spec §4 bag-count error and §7 adapter-orchestration deviation are handled in Task 9 rather than silently diverging.
- The v1 generators' deletion is covered by Tasks 2 and 4 grep steps (no production callers existed).
- Types consistent across tasks: `buildGrid`/`buildGridFromSeedString`/`Traits` (Task 1) are what Tasks 2, 5 import; Rust `derive_traits`/`generate_svg`/`generate_token_uri` (Task 4) are what Task 6 imports.
- Known engineer-adaptation points are explicitly listed (DashboardError variants, driver constructor, TokenId conversions, apiFetch export) — these require reading the named files, not guessing.
