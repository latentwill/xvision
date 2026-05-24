import { describe, expect, it } from "vitest";
import { strategyGridColumns } from "./StrategyCardGrid";

describe("strategyGridColumns", () => {
  it("matches the B2 comparison grid breakpoints", () => {
    expect(strategyGridColumns(1)).toBe(2);
    expect(strategyGridColumns(2)).toBe(2);
    expect(strategyGridColumns(3)).toBe(4);
    expect(strategyGridColumns(4)).toBe(4);
    expect(strategyGridColumns(5)).toBe(3);
    expect(strategyGridColumns(6)).toBe(3);
    expect(strategyGridColumns(7)).toBe(4);
    expect(strategyGridColumns(12)).toBe(4);
  });
});
