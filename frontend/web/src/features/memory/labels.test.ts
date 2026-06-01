import { describe, expect, it } from "vitest";
import { formatVerdict, formatPromotionState } from "./labels";

describe("formatVerdict", () => {
  it("maps passed → Kept", () => expect(formatVerdict("passed")).toBe("Kept"));
  it("maps failed → Dropped", () => expect(formatVerdict("failed")).toBe("Dropped"));
  it("passes through unknown values", () => expect(formatVerdict("unknown")).toBe("unknown"));
  it("returns empty string for null", () => expect(formatVerdict(null)).toBe(""));
  it("returns empty string for undefined", () => expect(formatVerdict(undefined)).toBe(""));
});

describe("formatPromotionState", () => {
  it("maps staged → Staged", () => expect(formatPromotionState("staged")).toBe("Staged"));
  it("maps active → Active", () => expect(formatPromotionState("active")).toBe("Active"));
  it("maps forgotten → Forgotten", () => expect(formatPromotionState("forgotten")).toBe("Forgotten"));
  it("maps promoted → Active", () => expect(formatPromotionState("promoted")).toBe("Active"));
  it("maps demoted → Retired", () => expect(formatPromotionState("demoted")).toBe("Retired"));
  it("passes through unknown values", () => expect(formatPromotionState("other")).toBe("other"));
  it("returns empty string for null", () => expect(formatPromotionState(null)).toBe(""));
  it("returns empty string for undefined", () => expect(formatPromotionState(undefined)).toBe(""));
});
