import { describe, expect, it } from "vitest";
import { chartTheme } from "./chart-theme";

describe("chartTheme", () => {
  it("uses Signal dark chart surfaces for dark theme", () => {
    expect(chartTheme("dark").background).toBe("#0A0A0A");
    expect(chartTheme("dark").grid).toBe("#1A1A1A");
  });

  it("uses Signal dark as the default", () => {
    expect(chartTheme().background).toBe("#0A0A0A");
  });

  it("supports light chart colors", () => {
    expect(chartTheme("light").text).toBe("#0B0E11");
  });
});
