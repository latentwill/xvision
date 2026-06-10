// F8 regression tests: canvas gradient/path construction must never be fed
// non-finite numbers (empty data, zero-height bbox, un-ranged scales used to
// produce repeated `createLinearGradient: non-finite` console errors).

import { describe, expect, it, vi } from "vitest";
import type uPlot from "uplot";

import {
  allFinite,
  buildReturnFillGradient,
  xvnAreaFill,
  xvnGradientFill,
  xvnLastDot,
  xvnSheen,
  xvnZeroLine,
} from "./uplot-plugins";

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
