import { describe, expect, it } from "vitest";

import { DEFAULT_VIEWBOX, histogramBars } from "./shape";

describe("histogramBars", () => {
  it("filters non-finite payload values before computing SVG geometry", () => {
    const bars = histogramBars([
      { x: 0, y: 2 },
      { x: 1, y: Number.NaN },
      { x: Number.POSITIVE_INFINITY, y: 4 },
      { x: 2, y: Number.NEGATIVE_INFINITY },
      { x: 3, y: -1 },
    ]);

    expect(bars).toHaveLength(2);
    for (const bar of bars) {
      expect(Number.isFinite(bar.x)).toBe(true);
      expect(Number.isFinite(bar.y)).toBe(true);
      expect(Number.isFinite(bar.width)).toBe(true);
      expect(Number.isFinite(bar.height)).toBe(true);
    }
  });

  it("returns no bars when every point is invalid", () => {
    expect(
      histogramBars(
        [
          { x: Number.NaN, y: 1 },
          { x: 1, y: Number.POSITIVE_INFINITY },
        ],
        DEFAULT_VIEWBOX,
      ),
    ).toEqual([]);
  });
});
