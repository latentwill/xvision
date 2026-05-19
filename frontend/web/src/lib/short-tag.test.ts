import { describe, expect, test } from "vitest";
import { shortenName, shortTag } from "./short-tag";

describe("shortenName", () => {
  test("takes the first alphanumeric word lowercased and truncates to 4", () => {
    expect(shortenName("mean-reversion-v3")).toBe("mean");
    expect(shortenName("Momentum Breakout v2")).toBe("mome");
    expect(shortenName("pairs-stat-arb-v7")).toBe("pair");
    expect(shortenName("VIX Spike 2018-02")).toBe("vix");
    expect(shortenName("vol")).toBe("vol");
  });

  test("returns null for empty / non-alpha inputs", () => {
    expect(shortenName(null)).toBe(null);
    expect(shortenName(undefined)).toBe(null);
    expect(shortenName("")).toBe(null);
    expect(shortenName("--")).toBe(null);
  });
});

describe("shortTag", () => {
  test("composes strategy·scenario from names when available", () => {
    expect(
      shortTag("mean-reversion-v3", "flash-crash-2010-05", "ag-xyz", "sc-abc"),
    ).toBe("mean·flas");
  });

  test("falls back to id slice when a name is missing", () => {
    expect(
      shortTag(null, "flash-crash-2010-05", "01HXYZAGENT123", "sc-abc"),
    ).toBe("ENT123·flas");
  });

  test("falls back to id slice when both names missing", () => {
    expect(shortTag(null, null, "01HXYZAGENT123", "01HXYZSCEN999")).toBe(
      "ENT123·CEN999",
    );
  });
});
