import { describe, expect, it } from "vitest";
import { applyFilter, defaultFilterState } from "./filter";
import type { ListingRow } from "./types";

function row(p: Partial<ListingRow>): ListingRow {
  return {
    id: "x", lineageId: "x", version: "v1.0",
    creator: { address: "0xabc" }, model: "Claude", style: "Day",
    assets: ["BTC"], return30dPct: 10, sharpe: 1, buyers: { humans: 5, agents: 0 },
    priceUsdc: 49, tier: "sealed", transferableLicense: false, verification: "unverified",
    acceptsX402: false, clones: 0, genArtSeed: "x", ...p,
  };
}

describe("applyFilter", () => {
  const rows = [
    row({ id: "a", assets: ["BTC"], return30dPct: 50, verification: "verified", buyers: { humans: 100, agents: 4 } }),
    row({ id: "b", assets: ["SOL"], return30dPct: 90, verification: "unverified", buyers: { humans: 10, agents: 0 } }),
    row({ id: "c", assets: ["BTC", "ETH"], return30dPct: 20, verification: "verified", buyers: { humans: 300, agents: 1 } }),
  ];

  it("filters by asset", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), assets: ["SOL"] });
    expect(out.rows.map((r) => r.id)).toEqual(["b"]);
    expect(out.matched).toBe(1);
    expect(out.total).toBe(3);
  });

  it("filters verified-only", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), trust: { verifiedOnly: true, acceptsAgents: false, auditedOnly: false } });
    expect(out.rows.map((r) => r.id).sort()).toEqual(["a", "c"]);
  });

  it("sorts by 30d return desc by default (segment does not override sort)", () => {
    // Segment is a pure filter — the user's sort choice is always authoritative.
    // The Toolbar sets sort: "buyers" when clicking "trending", sort: "newest"
    // when clicking "new" — but applyFilter itself never silently overrides f.sort.
    const out = applyFilter(rows, defaultFilterState());
    expect(out.rows.map((r) => r.id)).toEqual(["b", "a", "c"]);
  });

  it("trending segment does NOT override sort — respects the requested sort key", () => {
    // With segment:"trending" + sort:"return30d", applyFilter sorts by return30d,
    // not by buyers. The canonical sort is set by the Toolbar click handler, not
    // forced inside applyFilter.
    const out = applyFilter(rows, { ...defaultFilterState(), segment: "trending", sort: "return30d" });
    expect(out.rows.map((r) => r.id)).toEqual(["b", "a", "c"]);
  });

  it("new segment does NOT override sort — respects the requested sort key", () => {
    // Toolbar sets sort:"newest" when the user clicks "new", so by the time
    // applyFilter is called, f.sort is already "newest". applyFilter trusts f.sort.
    const out = applyFilter(rows, { ...defaultFilterState(), segment: "new", sort: "newest" });
    // "newest" sort: id.localeCompare descending → "c" > "b" > "a"
    expect(out.rows.map((r) => r.id)).toEqual(["c", "b", "a"]);
  });

  it("sorts by buyers (humans+agents) desc when sort:buyers", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), sort: "buyers" });
    expect(out.rows[0].id).toBe("c");
  });

  it("matches search over id and creator handle", () => {
    const withHandle = [row({ id: "btc-momentum", creator: { address: "0x1", handle: "@ed" } })];
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "mom" }).rows).toHaveLength(1);
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "@ed" }).rows).toHaveLength(1);
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "zzz" }).rows).toHaveLength(0);
  });

  it("matches search over the display name, model, style and assets", () => {
    // Operator report: searching "test" returned nothing for a listing named
    // "test" because the old query only looked at id + handle.
    const named = [
      row({ id: "abc123", name: "Test Strategy", model: "Claude", style: "Swing", assets: ["DOGE"] }),
    ];
    const search = (q: string) =>
      applyFilter(named, { ...defaultFilterState(), search: q }).rows;
    expect(search("test")).toHaveLength(1); // display name
    expect(search("claude")).toHaveLength(1); // model
    expect(search("swing")).toHaveLength(1); // style
    expect(search("doge")).toHaveLength(1); // asset
    expect(search("nope")).toHaveLength(0);
  });

  it("filters by tier: open returns only open-tier rows", () => {
    const mixed = [
      row({ id: "a", tier: "open" }),
      row({ id: "b", tier: "sealed" }),
      row({ id: "c", tier: "open" }),
    ];
    const out = applyFilter(mixed, { ...defaultFilterState(), tier: ["open"] });
    expect(out.rows.map((r) => r.id).sort()).toEqual(["a", "c"]);
    expect(out.matched).toBe(2);
  });

  it("tier filter: empty array = no tier filter (all pass through)", () => {
    const mixed = [row({ id: "a", tier: "open" }), row({ id: "b", tier: "sealed" })];
    const out = applyFilter(mixed, { ...defaultFilterState(), tier: [] });
    expect(out.matched).toBe(2);
  });
});
