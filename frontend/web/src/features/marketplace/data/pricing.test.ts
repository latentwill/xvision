import { describe, expect, it } from "vitest";
import { isFreeListing } from "./pricing";

describe("isFreeListing", () => {
  it("null price → free", () => expect(isFreeListing({ priceUsdc: null })).toBe(true));
  it("0 price → free", () => expect(isFreeListing({ priceUsdc: 0 })).toBe(true));
  it("positive price → paid", () => expect(isFreeListing({ priceUsdc: 49 })).toBe(false));
  it("1 USDC → paid", () => expect(isFreeListing({ priceUsdc: 1 })).toBe(false));
});
