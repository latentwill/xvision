import { describe, expect, it } from "vitest";
import {
  coerceDarkTheme,
  coerceThemePreference,
  resolveTheme,
  themeDefinitions,
} from "./themes";

describe("theme model", () => {
  it("falls back to folio dark for invalid saved preferences", () => {
    expect(coerceThemePreference(null)).toBe("folio-dark");
    expect(coerceThemePreference("sepia")).toBe("folio-dark");
  });

  it("falls back to folio dark for invalid saved dark themes", () => {
    expect(coerceDarkTheme(null)).toBe("folio-dark");
    expect(coerceDarkTheme("light")).toBe("folio-dark");
    expect(coerceDarkTheme("black")).toBe("black");
  });

  it("resolves auto from browser color scheme without choosing black", () => {
    expect(resolveTheme("auto", "light")).toBe("light");
    expect(resolveTheme("auto", "dark")).toBe("folio-dark");
  });

  it("defines all concrete palettes and swatches", () => {
    expect(Object.keys(themeDefinitions)).toEqual([
      "light",
      "folio-dark",
      "black",
    ]);
    expect(themeDefinitions.black.cssVars["--bg"]).toBe("#000000");
    expect(themeDefinitions["folio-dark"].cssVars["--bg"]).toBe("#0f0e0c");
    expect(themeDefinitions.light.mode).toBe("light");
  });
});
