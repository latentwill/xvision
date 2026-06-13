// frontend/web/src/api/cost.test.ts
//
// Covers the cost API client (bead-8wn):
//   - buildRollupUrl threads an optional `since` into the query string,
//     URL-encoded, and omits it when absent/empty (mirroring eval.ts).
//   - costKeys vary the rollup cache key on `since` so a window change
//     refetches, while absent/empty collapse onto the same key.
//   - getCostRollup / getCostBudget / setCostBudget call the right paths
//     and methods against a stubbed `apiFetch`.
//
// `buildRollupUrl` and `costKeys` are exported for pure-unit verification —
// the contract is "the SPA sends `?since=<rfc3339>`", proven without a fetch.

import { afterEach, describe, expect, it, vi } from "vitest";

import * as client from "./client";
import {
  buildRollupUrl,
  costKeys,
  getCostBudget,
  getCostRollup,
  setCostBudget,
} from "./cost";

afterEach(() => vi.restoreAllMocks());

describe("buildRollupUrl — since", () => {
  it("omits since when not provided", () => {
    expect(buildRollupUrl()).toBe("/api/cost/rollup");
    expect(buildRollupUrl({})).toBe("/api/cost/rollup");
  });

  it("appends a URL-encoded since param", () => {
    const url = buildRollupUrl({ since: "2026-06-06T00:00:00Z" });
    // Colons are percent-encoded by URLSearchParams.
    expect(url).toBe("/api/cost/rollup?since=2026-06-06T00%3A00%3A00Z");
  });

  it("treats an empty since as absent (no filter)", () => {
    expect(buildRollupUrl({ since: "" })).toBe("/api/cost/rollup");
  });
});

describe("costKeys.rollup — since", () => {
  it("varies the cache key on since so a window change refetches", () => {
    const a = costKeys.rollup();
    const windowed = costKeys.rollup("2026-06-06T00:00:00Z");
    expect(windowed).not.toEqual(a);
  });

  it("treats absent and empty since as the same key", () => {
    expect(costKeys.rollup("")).toEqual(costKeys.rollup());
  });

  it("keeps an identical key for identical since (stable cache hit)", () => {
    const a = costKeys.rollup("2026-06-06T00:00:00Z");
    const b = costKeys.rollup("2026-06-06T00:00:00Z");
    expect(a).toEqual(b);
  });

  it("exposes a stable budget key", () => {
    expect(costKeys.budget()).toEqual(costKeys.budget());
  });
});

describe("getCostRollup", () => {
  it("fetches the rollup URL and returns the parsed body (null-preserving)", async () => {
    const body = {
      since: "2026-06-06T00:00:00Z",
      spend_usd: 1.25,
      eval_cost_usd: 1.0,
      optimizer_cost_usd: 0.25,
      daily_cap_usd: null,
    };
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue(body as never);

    const out = await getCostRollup({ since: "2026-06-06T00:00:00Z" });
    expect(spy).toHaveBeenCalledWith(
      "/api/cost/rollup?since=2026-06-06T00%3A00%3A00Z",
    );
    expect(out).toEqual(body);
    // honesty: a null source field is preserved as null, never coerced to 0.
    expect(out.daily_cap_usd).toBeNull();
  });

  it("fetches the bare rollup URL when no since is supplied", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({} as never);
    await getCostRollup();
    expect(spy).toHaveBeenCalledWith("/api/cost/rollup");
  });
});

describe("getCostBudget", () => {
  it("fetches the budget URL and returns the cap", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ daily_cap_usd: 50 } as never);
    const out = await getCostBudget();
    expect(spy).toHaveBeenCalledWith("/api/cost/budget");
    expect(out.daily_cap_usd).toBe(50);
  });

  it("preserves a null (UNSET) cap", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue({
      daily_cap_usd: null,
    } as never);
    const out = await getCostBudget();
    expect(out.daily_cap_usd).toBeNull();
  });
});

describe("setCostBudget", () => {
  it("PUTs the cap and returns the persisted value", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ daily_cap_usd: 25 } as never);
    const out = await setCostBudget(25);
    expect(spy).toHaveBeenCalledWith("/api/cost/budget", {
      method: "PUT",
      body: JSON.stringify({ daily_cap_usd: 25 }),
    });
    expect(out.daily_cap_usd).toBe(25);
  });
});
