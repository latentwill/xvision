import { describe, expect, test } from "vitest";
import type { RunChartPayload } from "@/api/types.gen";
import {
  DASH,
  barsByAsset,
  fmtPctSigned,
  fmtUsdPlain,
  fmtUsdSigned,
  pnlTone,
} from "./live-format";

describe("pnlTone", () => {
  test("null/zero -> neutral, positive -> gold, negative -> danger", () => {
    expect(pnlTone(null)).toBe("text-text");
    expect(pnlTone(0)).toBe("text-text");
    expect(pnlTone(1)).toBe("text-gold");
    expect(pnlTone(-1)).toBe("text-danger");
  });
});

describe("fmtUsdSigned", () => {
  test("formats sign + thousands; unicode minus for negatives", () => {
    expect(fmtUsdSigned(null)).toBe(DASH);
    expect(fmtUsdSigned(0)).toBe("$0.00");
    expect(fmtUsdSigned(1200)).toBe("+$1,200.00");
    expect(fmtUsdSigned(-3.4)).toBe("−$3.40");
  });
});

describe("fmtUsdPlain", () => {
  test("unsigned dollars; dash for null", () => {
    expect(fmtUsdPlain(null)).toBe(DASH);
    expect(fmtUsdPlain(12000)).toBe("$12,000.00");
  });
});

describe("fmtPctSigned", () => {
  test("signed percent with two decimals", () => {
    expect(fmtPctSigned(null)).toBe(DASH);
    expect(fmtPctSigned(2.5)).toBe("+2.50%");
    expect(fmtPctSigned(-1.1)).toBe("−1.10%");
    expect(fmtPctSigned(0)).toBe("0.00%");
  });
});

describe("barsByAsset", () => {
  test("wraps single-asset payload into a one-entry map", () => {
    const payload = {
      asset: "BTC",
      bars: [{ time: 1, open: 1, high: 1, low: 1, close: 1 }],
    } as unknown as RunChartPayload;
    const m = barsByAsset(payload);
    expect(m.size).toBe(1);
    expect(m.get("BTC")).toHaveLength(1);
  });
});
