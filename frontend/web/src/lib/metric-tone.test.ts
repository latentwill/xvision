import { describe, expect, it } from "vitest";

import { drawdownMetricTone, drawdownToneClass } from "./metric-tone";

describe("drawdownToneClass", () => {
  it("renders danger for positive non-zero magnitude", () => {
    expect(drawdownToneClass(4.5)).toBe("text-danger");
    expect(drawdownToneClass(0.001)).toBe("text-danger");
    expect(drawdownToneClass(100)).toBe("text-danger");
  });

  it("renders danger for negative non-zero magnitude", () => {
    expect(drawdownToneClass(-4.5)).toBe("text-danger");
    expect(drawdownToneClass(-0.001)).toBe("text-danger");
    expect(drawdownToneClass(-100)).toBe("text-danger");
  });

  it("renders neutral for zero", () => {
    expect(drawdownToneClass(0)).toBe("text-text");
  });

  it("renders neutral for null and undefined", () => {
    expect(drawdownToneClass(null)).toBe("text-text");
    expect(drawdownToneClass(undefined)).toBe("text-text");
  });
});

describe("drawdownMetricTone", () => {
  it("returns 'neg' for any non-zero magnitude", () => {
    expect(drawdownMetricTone(4.5)).toBe("neg");
    expect(drawdownMetricTone(-4.5)).toBe("neg");
    expect(drawdownMetricTone(0.001)).toBe("neg");
  });

  it("returns undefined for zero and nullish so Metric falls back to neutral", () => {
    expect(drawdownMetricTone(0)).toBeUndefined();
    expect(drawdownMetricTone(null)).toBeUndefined();
    expect(drawdownMetricTone(undefined)).toBeUndefined();
  });
});
