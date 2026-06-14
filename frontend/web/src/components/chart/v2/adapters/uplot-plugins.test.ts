// F8 regression tests: canvas gradient/path construction must never be fed
// non-finite numbers (empty data, zero-height bbox, un-ranged scales used to
// produce repeated `createLinearGradient: non-finite` console errors).

import { describe, expect, it, vi } from "vitest";
import type uPlot from "uplot";

import {
  allFinite,
  buildDrawdownFillGradient,
  buildReturnFillGradient,
  xvnAreaFill,
  xvnGradientFill,
  xvnLastDot,
  xvnSheen,
  xvnTradeMarkers,
  xvnZeroLine,
} from "./uplot-plugins";
import type { V2Marker } from "../types";

/** Canvas-like 2d context that throws on non-finite gradient args, mirroring
 * real browser behaviour, and records every call. */
function strictCtx() {
  const gradient = { addColorStop: vi.fn() };
  return {
    createLinearGradient: vi.fn((...args: number[]) => {
      if (args.some((a) => !Number.isFinite(a))) {
        throw new TypeError("createLinearGradient: non-finite");
      }
      return gradient;
    }),
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    rect: vi.fn(),
    clip: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    arc: vi.fn(),
    fillRect: vi.fn(),
    fillText: vi.fn(),
    setLineDash: vi.fn(),
  };
}

function fakePlot(over: Partial<Record<string, unknown>> = {}): uPlot {
  return {
    ctx: strictCtx(),
    data: [[], []],
    bbox: { top: NaN, height: NaN, left: NaN, width: NaN },
    scales: { x: {}, y: { min: undefined, max: undefined } },
    valToPos: () => NaN,
    ...over,
  } as unknown as uPlot;
}

describe("allFinite", () => {
  it("accepts finite numbers and rejects NaN/Infinity", () => {
    expect(allFinite(0, 1.5, -3)).toBe(true);
    expect(allFinite(0, NaN)).toBe(false);
    expect(allFinite(Infinity)).toBe(false);
    expect(allFinite(-Infinity, 1)).toBe(false);
  });
});

describe("buildReturnFillGradient (F8 guard)", () => {
  it("returns a transparent fill instead of throwing when positions are non-finite", () => {
    const u = fakePlot();
    expect(() => buildReturnFillGradient(u)).not.toThrow();
    expect(buildReturnFillGradient(u)).toBe("rgba(0,0,0,0)");
    expect((u.ctx.createLinearGradient as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
  });

  it("builds a gradient when positions are finite", () => {
    const u = fakePlot({
      scales: { x: {}, y: { min: -1, max: 1 } },
      valToPos: (v: number) => (1 - v) * 50, // top=0, bot=100, zero=50
    });
    const fill = buildReturnFillGradient(u);
    expect(typeof fill).not.toBe("string");
    expect(u.ctx.createLinearGradient).toHaveBeenCalledWith(0, 0, 0, 100);
  });
});

describe("buildDrawdownFillGradient (F8 guard)", () => {
  it("returns a transparent fill instead of throwing when positions are non-finite", () => {
    const u = fakePlot();
    expect(() => buildDrawdownFillGradient(u)).not.toThrow();
    expect(buildDrawdownFillGradient(u)).toBe("rgba(0,0,0,0)");
    expect((u.ctx.createLinearGradient as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
  });

  it("builds a surface-to-depth gradient when positions are finite", () => {
    const u = fakePlot({
      scales: { x: {}, y: { min: -8, max: 0 } },
      // top(0)=0, bot(-8)=100; avoids -0 (fails strict mock-arg equality)
      valToPos: (v: number) => (1 - (v + 8) / 8) * 100,
    });
    const fill = buildDrawdownFillGradient(u);
    expect(typeof fill).not.toBe("string");
    expect(u.ctx.createLinearGradient).toHaveBeenCalledWith(0, 0, 0, 100);
  });

  it("falls back to transparent on a zero/negative span", () => {
    const u = fakePlot({
      scales: { x: {}, y: { min: 0, max: 0 } },
      valToPos: () => 50, // top === bot
    });
    expect(buildDrawdownFillGradient(u)).toBe("rgba(0,0,0,0)");
  });
});

describe("area/gradient/sheen/dot/zero-line plugins (F8 guards)", () => {
  const data = [
    [1, 2, 3],
    [0.1, 0.2, 0.3],
  ] as uPlot.AlignedData;

  it("xvnAreaFill draws nothing on a non-finite bbox", () => {
    const u = fakePlot({ data });
    expect(() => xvnAreaFill(1, "rgba(0,230,118,0.2)").hooks.draw(u)).not.toThrow();
    expect(u.ctx.createLinearGradient).not.toHaveBeenCalled();
  });

  it("xvnAreaFill draws nothing when every position is non-finite", () => {
    const u = fakePlot({
      data,
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      valToPos: () => NaN,
    });
    expect(() => xvnAreaFill(1, "rgba(0,230,118,0.2)").hooks.draw(u)).not.toThrow();
    expect(u.ctx.createLinearGradient).not.toHaveBeenCalled();
  });

  it("xvnAreaFill draws when data and bbox are finite", () => {
    const u = fakePlot({
      data,
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      valToPos: (v: number) => v * 10,
    });
    xvnAreaFill(1, "rgba(0,230,118,0.2)").hooks.draw(u);
    expect(u.ctx.createLinearGradient).toHaveBeenCalledWith(0, 0, 0, 100);
    expect(u.ctx.fill).toHaveBeenCalled();
  });

  it("xvnGradientFill draws nothing on a non-finite bbox", () => {
    const u = fakePlot({ data });
    expect(() => xvnGradientFill(1).hooks.draw(u)).not.toThrow();
    expect(u.ctx.createLinearGradient).not.toHaveBeenCalled();
  });

  it("xvnSheen draws nothing on a non-finite bbox", () => {
    const u = fakePlot({ data });
    expect(() => xvnSheen().hooks.draw(u)).not.toThrow();
    expect(u.ctx.createLinearGradient).not.toHaveBeenCalled();
  });

  it("xvnLastDot draws nothing when positions are non-finite", () => {
    const u = fakePlot({ data });
    expect(() => xvnLastDot(1, "#00e676").hooks.draw(u)).not.toThrow();
    expect(u.ctx.arc).not.toHaveBeenCalled();
  });

  it("xvnZeroLine draws nothing when the zero position is non-finite", () => {
    const u = fakePlot({
      data,
      scales: { x: {}, y: { min: -1, max: 1 } },
    });
    expect(() => xvnZeroLine().hooks.draw(u)).not.toThrow();
    expect(u.ctx.stroke).not.toHaveBeenCalled();
  });
});

describe("xvnTradeMarkers", () => {
  it("does not throw and does not call fill when markers array is empty", () => {
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    expect(() => xvnTradeMarkers([]).hooks.draw(u)).not.toThrow();
    expect(u.ctx.fill).not.toHaveBeenCalled();
  });

  it("skips markers whose time is outside x scale min/max", () => {
    const outOfRange: V2Marker[] = [
      { kind: "buy",  time: 500,  price: 1.0 }, // before x min
      { kind: "sell", time: 9999, price: 1.0 }, // after x max
    ];
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    expect(() => xvnTradeMarkers(outOfRange).hooks.draw(u)).not.toThrow();
    expect(u.ctx.fill).not.toHaveBeenCalled();
  });

  it("calls fill for an in-range buy marker with a known price", () => {
    const markers: V2Marker[] = [
      { kind: "buy", time: 2000, price: 1.1 },
    ];
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    xvnTradeMarkers(markers).hooks.draw(u);
    expect(u.ctx.fill).toHaveBeenCalled();
  });

  it("calls fill for an in-range sell marker with a known price", () => {
    const markers: V2Marker[] = [
      { kind: "sell", time: 2000, price: 1.1 },
    ];
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    xvnTradeMarkers(markers).hooks.draw(u);
    expect(u.ctx.fill).toHaveBeenCalled();
  });

  it("uses custom buy/sell colors when provided", () => {
    const markers: V2Marker[] = [
      { kind: "buy",  time: 2000, price: 1.1 },
      { kind: "sell", time: 2500, price: 1.0 },
    ];
    const u = fakePlot({
      data: [[1000, 2000, 2500, 3000], [1.0, 1.1, 1.0, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    xvnTradeMarkers(markers, { buyColor: "#aabbcc", sellColor: "#112233" }).hooks.draw(u);
    // fill should have been called for both markers
    expect((u.ctx.fill as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it("falls back to series value when price is absent", () => {
    const markers: V2Marker[] = [
      { kind: "buy", time: 2000 }, // no price — should look up series[1][1]
    ];
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: (v: number) => v * 0.1,
    });
    expect(() => xvnTradeMarkers(markers).hooks.draw(u)).not.toThrow();
    expect(u.ctx.fill).toHaveBeenCalled();
  });

  it("skips marker when valToPos returns non-finite for an in-range time", () => {
    const markers: V2Marker[] = [
      { kind: "buy", time: 2000, price: 1.1 },
    ];
    const u = fakePlot({
      data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
      bbox: { top: 0, height: 100, left: 0, width: 300 },
      scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
      valToPos: () => NaN, // all positions non-finite
    });
    expect(() => xvnTradeMarkers(markers).hooks.draw(u)).not.toThrow();
    expect(u.ctx.fill).not.toHaveBeenCalled();
  });

  describe("glyph: 'letter' (QA #1 equity B/S markers)", () => {
    function plotWithSeries() {
      return fakePlot({
        data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
        bbox: { top: 0, height: 100, left: 0, width: 300 },
        scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
        valToPos: (v: number) => v * 0.1,
      });
    }

    it("draws 'B' for a buy marker and 'S' for a sell marker", () => {
      const markers: V2Marker[] = [
        { kind: "buy", time: 2000, price: 1.1 },
        { kind: "sell", time: 3000, price: 1.2 },
      ];
      const u = plotWithSeries();
      xvnTradeMarkers(markers, { glyph: "letter" }).hooks.draw(u);
      const ft = u.ctx.fillText as ReturnType<typeof vi.fn>;
      const letters = ft.mock.calls.map((c) => c[0]);
      expect(letters).toContain("B");
      expect(letters).toContain("S");
    });

    it("anchors letters to the series value, ignoring marker.price", () => {
      // price 99 is off the y-scale; with anchorToSeries we must use the
      // equity series value at the marker's time, so valToPos sees ~1.1 not 99.
      const seen: number[] = [];
      const u = fakePlot({
        data: [[1000, 2000, 3000], [1.0, 1.1, 1.2]],
        bbox: { top: 0, height: 100, left: 0, width: 300 },
        scales: { x: { min: 1000, max: 3000 }, y: { min: 0, max: 2 } },
        valToPos: (v: number, axis?: string) => {
          if (axis === "y") seen.push(v);
          return v * 0.1;
        },
      });
      xvnTradeMarkers([{ kind: "buy", time: 2000, price: 99 }], {
        glyph: "letter",
        anchorToSeries: true,
      }).hooks.draw(u);
      expect(seen).toContain(1.1); // series value, not 99
      expect(seen).not.toContain(99);
    });

    it("does not throw on empty markers in letter mode", () => {
      const u = plotWithSeries();
      expect(() =>
        xvnTradeMarkers([], { glyph: "letter" }).hooks.draw(u),
      ).not.toThrow();
      expect(u.ctx.fillText).not.toHaveBeenCalled();
    });
  });
});
