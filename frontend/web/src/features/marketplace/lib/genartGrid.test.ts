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
    const seen = new Map<string, Int8Array>();
    for (let i = 0; seen.size < 8 && i < 4000; i++) {
      const { grid, traits } = buildGridFromSeedString(`law-${i}`);
      if (!seen.has(traits.symmetry)) seen.set(traits.symmetry, grid);
    }
    expect(seen.size).toBe(8);
    const g = (grid: Int8Array, x: number, y: number) => grid[y * N + x];
    const quad = seen.get("quad")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
      expect(g(quad, x, y)).toBe(g(quad, N - 1 - x, y));
      expect(g(quad, x, y)).toBe(g(quad, x, N - 1 - y));
    }
    const mirrorX = seen.get("mirror-x")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(mirrorX, x, y)).toBe(g(mirrorX, N - 1 - x, y));
    const mirrorY = seen.get("mirror-y")!;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++)
      expect(g(mirrorY, x, y)).toBe(g(mirrorY, x, N - 1 - y));
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
  it("buildGrid produces same result as buildGridFromSeedString", () => {
    const id = "ok";
    const hash = "a".repeat(64);
    const a = buildGrid(id, hash);
    const b = buildGridFromSeedString(`${id}:${hash}`);
    expect(Array.from(a.grid)).toEqual(Array.from(b.grid));
    expect(a.palette).toEqual(b.palette);
    expect(a.traits).toEqual(b.traits);
  });
  it("buildGrid rejects uppercase hex", () => {
    expect(() => buildGrid("ok", "A".repeat(64))).toThrow();
  });
});
