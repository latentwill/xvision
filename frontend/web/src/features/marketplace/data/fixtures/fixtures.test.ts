// src/features/marketplace/data/fixtures/fixtures.test.ts
import { describe, expect, it } from "vitest";
import { ALL_LISTINGS, LISTING_DETAILS, NAMED_LISTINGS, makeWallListings } from "./listings";
import { CREATORS } from "./creators";
import { SLICES } from "./slices";
import { RECEIPTS } from "./receipts";
import { buildPublishDraft } from "./seller";

describe("fixtures", () => {
  it("wall generator is deterministic and 200 rows", () => {
    expect(makeWallListings()).toHaveLength(200);
    expect(makeWallListings()[5]).toEqual(makeWallListings()[5]);
  });

  it("QA1: ALL_LISTINGS in dev (import.meta.env.DEV=true in vitest) includes named + wall", () => {
    // In the vitest environment, import.meta.env.DEV is true, so ALL_LISTINGS
    // should contain both NAMED_LISTINGS and the 200 wall rows.
    expect(ALL_LISTINGS.length).toBe(NAMED_LISTINGS.length + 200);
  });

  it("every detail extends a known row", () => {
    for (const [id, d] of Object.entries(LISTING_DETAILS)) {
      expect(d.id).toBe(id);
      expect(NAMED_LISTINGS.some((r) => r.id === id)).toBe(true);
    }
  });
  it("creator strategies reference real listing fields", () => {
    expect(CREATORS["@ed"].strategies.every((s) => "status" in s)).toBe(true);
  });
  it("slices + receipts present", () => {
    expect(SLICES.length).toBeGreaterThanOrEqual(6);
    expect(RECEIPTS["0xdemo-tx"].license.netToCreatorUsdc).toBeCloseTo(46.55);
  });
  it("publish draft flags missing assets", () => {
    const d = buildPublishDraft("local-wip-draft");
    expect(d.listable.find((c) => c.label.includes("asset"))?.ok).toBe(false);
  });

  it("makeWallListings is still exported and usable for dev use", () => {
    const wall = makeWallListings(10);
    expect(wall).toHaveLength(10);
    // Every row has a non-empty id
    expect(wall.every((r) => r.id.startsWith("wall-strat-"))).toBe(true);
  });
});
