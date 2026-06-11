import { describe, expect, it } from "vitest";
import { N, buildGrid } from "./genartGrid";
import { generateSvg, generateTokenUri } from "./genart";

const HASH = "b".repeat(64);
const AID = "01HXVNTESTAGENT";

function decodePaths(svg: string, palette: readonly string[]): Int8Array {
  // RLE round-trip: parse <path> elements back into a grid using stroke->index from buildGrid's palette
  const grid = new Int8Array(N * N).fill(-1);
  const pathRe = /<path stroke="(#[0-9a-f]{6})" stroke-width="1" d="([^"]*)"\s*\/>/g;
  let pm: RegExpExecArray | null;
  while ((pm = pathRe.exec(svg))) {
    const [, stroke, d] = pm;
    const idx = palette.indexOf(stroke);
    expect(idx).toBeGreaterThanOrEqual(0);
    const runRe = /M(\d+) (\d+)\.5h(\d+)/g;
    let rm: RegExpExecArray | null;
    let consumed = "";
    while ((rm = runRe.exec(d))) {
      consumed += rm[0];
      const x = Number(rm[1]);
      const y = Number(rm[2]);
      const w = Number(rm[3]);
      for (let dx = 0; dx < w; dx++) grid[y * N + x + dx] = idx;
    }
    // Assert the full d string is consumed by runs — no garbage can hide
    expect(consumed).toBe(d);
  }
  return grid;
}

describe("generateSvg", () => {
  it("round-trips RLE back to the display grid", () => {
    const { grid, palette } = buildGrid(AID, HASH);
    const svg = generateSvg(AID, HASH);
    expect(Array.from(decodePaths(svg, palette))).toEqual(Array.from(grid));
  });
  it("has the normative envelope", () => {
    const svg = generateSvg(AID, HASH);
    expect(svg.startsWith(
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 28 28" width="560" height="560" shape-rendering="crispEdges">`,
    )).toBe(true);
    expect(svg.endsWith("</svg>")).toBe(true);
    expect(svg).not.toContain("\n");
  });
  it("emits path elements in ascending palette-index order", () => {
    const { palette } = buildGrid(AID, HASH);
    const svg = generateSvg(AID, HASH);
    const pathRe = /<path stroke="(#[0-9a-f]{6})" stroke-width="1"/g;
    const indices: number[] = [];
    let m: RegExpExecArray | null;
    while ((m = pathRe.exec(svg))) {
      const idx = palette.indexOf(m[1]);
      expect(idx).toBeGreaterThanOrEqual(0);
      indices.push(idx);
    }
    // Strictly ascending palette indices
    for (let i = 1; i < indices.length; i++) {
      expect(indices[i]).toBeGreaterThan(indices[i - 1]);
    }
  });
  it("rejects bad input", () => {
    expect(() => generateSvg("", HASH)).toThrow();
    expect(() => generateSvg(AID, "nothex")).toThrow();
  });
});

describe("generateTokenUri", () => {
  it("emits base64 JSON with traits and stays under the 16KB ceiling", () => {
    const uri = generateTokenUri(AID, HASH);
    expect(uri.startsWith("data:application/json;base64,")).toBe(true);
    const json = JSON.parse(atob(uri.slice("data:application/json;base64,".length)));
    expect(json.name).toBe(`xvn strategy ${AID.slice(0, 8)}`);
    expect(json.agent_id).toBe(AID);
    expect(json.image.startsWith("data:image/svg+xml;base64,")).toBe(true);
    const types = json.attributes.map((a: { trait_type: string }) => a.trait_type);
    expect(types).toEqual(["Symmetry", "Palette", "Density", "Layers"]);
    expect(uri.length).toBeLessThanOrEqual(16 * 1024);
  });
  it("enforces byte-order of tokenURI JSON keys", () => {
    const uri = generateTokenUri(AID, HASH);
    const raw = atob(uri.slice("data:application/json;base64,".length));
    expect(raw).toMatch(/^\{"name":"xvn strategy [0-9A-Za-z_-]{1,8}","image":"data:image\/svg\+xml;base64,/);
    expect(raw).toContain('","agent_id":"');
    expect(raw).toContain('"attributes":[{"trait_type":"Symmetry","value":"');
    expect(raw).toContain('{"trait_type":"Palette","value":"');
    expect(raw).toMatch(/\{"trait_type":"Density","value":\d+\},\{"trait_type":"Layers","value":6\}\]\}$/);
  });
  it("size ceiling holds across 300 seeds", () => {
    for (let i = 0; i < 300; i++) {
      const h = i.toString(16).padStart(64, "0");
      const uri = generateTokenUri("01HXVNSZ", h);
      expect(uri.length).toBeLessThanOrEqual(16 * 1024);
      // Every 15th seed: round-trip SVG -> grid
      if (i % 15 === 0) {
        const { grid, palette } = buildGrid("01HXVNSZ", h);
        const svg = generateSvg("01HXVNSZ", h);
        expect(Array.from(decodePaths(svg, palette))).toEqual(Array.from(grid));
      }
    }
  });
});
