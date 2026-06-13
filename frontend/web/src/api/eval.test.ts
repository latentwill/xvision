// frontend/web/src/api/eval.test.ts
//
// Covers the `since` time-window wiring (bead-008) on the eval runs list:
//   - buildRunsListUrl threads `since` into the query string (URL-encoded).
//   - evalKeys.runs varies on `since` so a window change refetches.
//
// `buildRunsListUrl` and `evalKeys` are exported for exactly this kind of
// pure-unit verification — the contract is "the SPA sends `?since=<rfc3339>`",
// proven here without booting a fetch.

import { describe, expect, it } from "vitest";
import { buildRunsListUrl, evalKeys } from "./eval";

describe("buildRunsListUrl — since", () => {
  it("omits since when not provided (first paint unchanged)", () => {
    expect(buildRunsListUrl({ limit: 100 })).toBe("/api/eval/runs?limit=100");
    expect(buildRunsListUrl()).toBe("/api/eval/runs");
  });

  it("appends a URL-encoded since param", () => {
    const url = buildRunsListUrl({ since: "2026-06-06T00:00:00Z" });
    // Colons are percent-encoded by URLSearchParams.
    expect(url).toBe("/api/eval/runs?since=2026-06-06T00%3A00%3A00Z");
  });

  it("combines since with the other filters", () => {
    const url = buildRunsListUrl({
      status: "completed",
      limit: 100,
      since: "2026-06-06T00:00:00Z",
    });
    expect(url).toContain("status=completed");
    expect(url).toContain("limit=100");
    expect(url).toContain("since=2026-06-06T00%3A00%3A00Z");
  });

  it("treats an empty since as absent (no filter)", () => {
    expect(buildRunsListUrl({ since: "", limit: 100 })).toBe(
      "/api/eval/runs?limit=100",
    );
  });
});

describe("evalKeys.runs — since", () => {
  it("varies the cache key on since so a window change refetches", () => {
    const all = evalKeys.runs({ limit: 100 });
    const windowed = evalKeys.runs({ limit: 100, since: "2026-06-06T00:00:00Z" });
    expect(windowed).not.toEqual(all);
  });

  it("treats absent and empty since as the same key (no extra fetch on All)", () => {
    const absent = evalKeys.runs({ limit: 100 });
    const empty = evalKeys.runs({ limit: 100, since: "" });
    expect(empty).toEqual(absent);
  });

  it("keeps an identical key for identical since (stable cache hit)", () => {
    const a = evalKeys.runs({ limit: 100, since: "2026-06-06T00:00:00Z" });
    const b = evalKeys.runs({ limit: 100, since: "2026-06-06T00:00:00Z" });
    expect(a).toEqual(b);
  });
});
