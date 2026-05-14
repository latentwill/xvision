import { describe, expect, it } from "vitest";
import { chartTheme } from "./chart-theme";

describe("chartTheme", () => {
  it("uses black chart surfaces for black theme", () => {
    expect(chartTheme("black").background).toBe("#000000");
    expect(chartTheme("black").grid).toBe("#1f1f1f");
  });

  it("keeps folio dark as the dark default", () => {
    expect(chartTheme("folio-dark").background).toBe("#14120e");
  });

  it("supports light chart colors", () => {
    expect(chartTheme("light").text).toBe("#201d18");
  });
});
