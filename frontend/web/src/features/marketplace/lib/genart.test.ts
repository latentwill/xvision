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
