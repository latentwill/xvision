import { describe, expect, it } from "vitest";
import {
  coerceThemePreference,
  resolveTheme,
  themeDefinitions,
} from "./themes";

describe("theme model", () => {
  it("migrates retired theme ids to dark", () => {
    expect(coerceThemePreference("folio-dark")).toBe("dark");
    expect(coerceThemePreference("black")).toBe("dark");
    expect(coerceThemePreference("light")).toBe("light");
    expect(coerceThemePreference(null)).toBe("dark");
  });

  it("falls back to dark for invalid saved preferences", () => {
    expect(coerceThemePreference(null)).toBe("dark");
    expect(coerceThemePreference("sepia")).toBe("dark");
  });

  it("resolves auto from browser color scheme", () => {
    expect(resolveTheme("auto", "light")).toBe("light");
    expect(resolveTheme("auto", "dark")).toBe("dark");
  });

  it("defines exactly two concrete palettes: dark and light", () => {
    expect(Object.keys(themeDefinitions)).toEqual(["dark", "light"]);
    expect(themeDefinitions.dark.cssVars["--bg"]).toBe("#000000");
    expect(themeDefinitions.dark.cssVars["--gold"]).toBe("#00E676");
    expect(themeDefinitions.light.cssVars["--bg"]).toBe("#F7F8FA");
    expect(themeDefinitions.light.mode).toBe("light");
    expect(themeDefinitions.dark.mode).toBe("dark");
  });
});
