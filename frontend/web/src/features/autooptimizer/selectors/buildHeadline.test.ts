import { describe, expect, it } from "vitest";
import { buildHeadline } from "./buildHeadline";

describe("buildHeadline", () => {
  it("running state names the cycle count and lineages", () => {
    const h = buildHeadline({
      state: "running",
      activeLineages: 5,
      lastCycle: null,
      lastCycleAgo: null,
    });
    expect(h.title).toBe("A run is in progress.");
    expect(h.subtitle).toBe("1 cycle running · 5 active lineages.");
  });

  it("idle state reports the last cycle outcome", () => {
    const h = buildHeadline({
      state: "idle",
      activeLineages: 5,
      lastCycle: { kept: 2, total: 14 },
      lastCycleAgo: "3h ago",
    });
    expect(h.title).toBe("Last ran 3h ago — kept 2 of 14 experiments.");
  });

  it("idle state appends the best find one-liner when available", () => {
    const h = buildHeadline({
      state: "idle",
      activeLineages: 5,
      lastCycle: { kept: 2, total: 14 },
      lastCycleAgo: "3h ago",
      bestFind: { hash: "abcd1234ef", delta: 0.21 },
    });
    expect(h.subtitle).toBe("Best find: abcd1234 (ΔSharpe +0.21) · 5 active lineages.");
  });

  it("includes the best-find CI band when available", () => {
    const h = buildHeadline({
      state: "idle",
      activeLineages: 5,
      lastCycle: { kept: 2, total: 14 },
      lastCycleAgo: "3h ago",
      bestFind: { hash: "abcd1234ef", delta: 0.21, ciLow: -0.04, ciHigh: 0.39 },
    });
    expect(h.subtitle).toContain("ΔSharpe +0.21 CI -0.04..0.39");
  });

  it("paused state names the state", () => {
    expect(buildHeadline({ state: "paused", activeLineages: 0, lastCycle: null, lastCycleAgo: null }).title)
      .toBe("A run is paused.");
  });

  it("cancelling state names the state", () => {
    expect(buildHeadline({ state: "cancelling", activeLineages: 0, lastCycle: null, lastCycleAgo: null }).title)
      .toBe("A run is cancelling.");
  });

  it("never-ran state invites the first launch", () => {
    const h = buildHeadline({ state: "idle", activeLineages: 0, lastCycle: null, lastCycleAgo: null });
    expect(h.title).toBe("The optimizer hasn't run yet.");
    expect(h.subtitle).toBe("Launch its first cycle.");
  });

  it("never says tonight", () => {
    for (const state of ["running", "paused", "idle"] as const) {
      const h = buildHeadline({ state, activeLineages: 3, lastCycle: { kept: 1, total: 5 }, lastCycleAgo: "1d ago" });
      expect(`${h.title} ${h.subtitle}`.toLowerCase()).not.toContain("tonight");
    }
  });
});
