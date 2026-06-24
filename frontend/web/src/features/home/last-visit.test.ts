import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import {
  LAST_VISIT_LS,
  __resetVisitSessionForTest,
  computeSinceDelta,
  persistVisitOnce,
  readLastVisit,
  snapshotLastVisit,
  writeLastVisit,
} from "./last-visit";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function run(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "strat-1",
    scenario_id: "scn-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-06-10T09:00:00Z",
    completed_at: "2026-06-10T10:00:00Z",
    sharpe: 1.2,
    max_drawdown_pct: 0.5,
    total_return_pct: 0.1,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    ...over,
  };
}

function finding(created_at: string | null | undefined) {
  return { created_at };
}

// ---------------------------------------------------------------------------
// computeSinceDelta — strict-after boundary
// ---------------------------------------------------------------------------

describe("computeSinceDelta", () => {
  const lastVisitIso = "2026-06-12T00:00:00Z";
  const now = Date.parse("2026-06-12T05:00:00Z"); // 5h after last visit

  it("counts only runs completed STRICTLY AFTER lastVisitIso", () => {
    const runs = [
      run({ id: "before", completed_at: "2026-06-11T23:59:59Z" }),
      run({ id: "exactly", completed_at: lastVisitIso }), // boundary: excluded
      run({ id: "after", completed_at: "2026-06-12T00:00:01Z" }),
      run({ id: "later", completed_at: "2026-06-12T04:00:00Z" }),
    ];
    const delta = computeSinceDelta({ runs, findings: [], lastVisitIso, now });
    expect(delta.runsSince).toBe(2);
  });

  it("counts only findings created STRICTLY AFTER lastVisitIso", () => {
    const findings = [
      finding("2026-06-11T23:59:59Z"),
      finding(lastVisitIso), // boundary: excluded
      finding("2026-06-12T00:00:01Z"),
      finding("2026-06-12T04:00:00Z"),
    ];
    const delta = computeSinceDelta({ runs: [], findings, lastVisitIso, now });
    expect(delta.findingsSince).toBe(2);
  });

  it("ignores runs with no completed_at (queued/running)", () => {
    const runs = [
      run({ id: "running", status: "running", completed_at: null }),
      run({ id: "done", completed_at: "2026-06-12T01:00:00Z" }),
    ];
    const delta = computeSinceDelta({ runs, findings: [], lastVisitIso, now });
    expect(delta.runsSince).toBe(1);
  });

  it("ignores findings with no created_at", () => {
    const findings = [
      finding(null),
      finding(undefined),
      finding("2026-06-12T01:00:00Z"),
    ];
    const delta = computeSinceDelta({ runs: [], findings, lastVisitIso, now });
    expect(delta.findingsSince).toBe(1);
  });

  it("computes hoursAgo from lastVisitIso to now, floored", () => {
    const delta = computeSinceDelta({
      runs: [],
      findings: [],
      lastVisitIso,
      now: Date.parse("2026-06-12T05:59:00Z"), // 5h59m → 5h floored
    });
    expect(delta.hoursAgo).toBe(5);
    expect(delta.firstVisit).toBe(false);
  });

  it("clamps hoursAgo to 0 when now precedes lastVisit (clock skew)", () => {
    const delta = computeSinceDelta({
      runs: [],
      findings: [],
      lastVisitIso,
      now: Date.parse("2026-06-11T22:00:00Z"),
    });
    expect(delta.hoursAgo).toBe(0);
  });

  // ── first-visit: null lastVisitIso → zeros + flag ──────────────────────
  it("returns zeros and firstVisit=true when lastVisitIso is null", () => {
    const runs = [run({ completed_at: "2026-06-12T01:00:00Z" })];
    const findings = [finding("2026-06-12T01:00:00Z")];
    const delta = computeSinceDelta({
      runs,
      findings,
      lastVisitIso: null,
      now,
    });
    expect(delta).toEqual({
      runsSince: 0,
      findingsSince: 0,
      hoursAgo: null,
      firstVisit: true,
    });
  });

  it("treats an unparseable lastVisitIso as a first visit", () => {
    const delta = computeSinceDelta({
      runs: [run({ completed_at: "2026-06-12T01:00:00Z" })],
      findings: [],
      lastVisitIso: "not-a-date",
      now,
    });
    expect(delta.firstVisit).toBe(true);
    expect(delta.runsSince).toBe(0);
    expect(delta.hoursAgo).toBeNull();
  });

  it("defaults now to Date.now() when omitted", () => {
    const delta = computeSinceDelta({
      runs: [],
      findings: [],
      lastVisitIso,
    });
    expect(typeof delta.hoursAgo).toBe("number");
    expect(delta.hoursAgo).toBeGreaterThanOrEqual(0);
  });
});

// ---------------------------------------------------------------------------
// read/write helpers — storage-unavailable tolerance
// ---------------------------------------------------------------------------

describe("readLastVisit / writeLastVisit", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("exports the locked storage key", () => {
    expect(LAST_VISIT_LS).toBe("xvn.home.last_visit");
  });

  it("round-trips a written timestamp", () => {
    writeLastVisit("2026-06-12T00:00:00Z");
    expect(readLastVisit()).toBe("2026-06-12T00:00:00Z");
  });

  it("returns null before any write (first visit)", () => {
    expect(readLastVisit()).toBeNull();
  });

  it("tolerates storage that throws on read (returns null)", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readLastVisit()).toBeNull();
  });

  it("tolerates storage that throws on write (no throw)", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => writeLastVisit("2026-06-12T00:00:00Z")).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// snapshotLastVisit / persistVisitOnce — remount-safe page-load-session boundary
// ---------------------------------------------------------------------------

describe("snapshotLastVisit / persistVisitOnce", () => {
  beforeEach(() => {
    localStorage.clear();
    __resetVisitSessionForTest();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    __resetVisitSessionForTest();
  });

  it("snapshots the stored boundary on first read", () => {
    writeLastVisit("2026-06-12T00:00:00Z");
    expect(snapshotLastVisit()).toBe("2026-06-12T00:00:00Z");
  });

  it("returns null when there is no prior boundary (first visit)", () => {
    expect(snapshotLastVisit()).toBeNull();
  });

  it("FREEZES the boundary across a subsequent write (remount-safe)", () => {
    writeLastVisit("2026-06-12T00:00:00Z"); // previous visit T0
    expect(snapshotLastVisit()).toBe("2026-06-12T00:00:00Z");

    // This visit persists "now" (T1). storage advances...
    persistVisitOnce("2026-06-13T09:00:00Z");
    expect(readLastVisit()).toBe("2026-06-13T09:00:00Z");

    // ...but the session snapshot still reports T0, so a remount this page
    // load (SPA nav back / StrictMode double-invoke) keeps measuring from T0,
    // not the value we just wrote.
    expect(snapshotLastVisit()).toBe("2026-06-12T00:00:00Z");
  });

  it("freezes the prior boundary even when persistVisitOnce is called first", () => {
    writeLastVisit("2026-06-12T00:00:00Z");
    persistVisitOnce("2026-06-13T09:00:00Z"); // write before any snapshot
    // snapshot must still be the boundary as it was BEFORE this visit's write
    expect(snapshotLastVisit()).toBe("2026-06-12T00:00:00Z");
  });

  it("persists exactly once per page-load session", () => {
    persistVisitOnce("2026-06-13T09:00:00Z");
    persistVisitOnce("2026-06-13T10:00:00Z");
    persistVisitOnce("2026-06-13T11:00:00Z");
    // Only the first call writes; later calls are no-ops, so storage still
    // holds the first timestamp.
    expect(readLastVisit()).toBe("2026-06-13T09:00:00Z");
  });

  it("captures the new boundary after a session reset (next page load)", () => {
    writeLastVisit("2026-06-12T00:00:00Z");
    snapshotLastVisit();
    persistVisitOnce("2026-06-13T09:00:00Z");

    // New page load: the module session resets and the next snapshot reads the
    // boundary the previous session persisted.
    __resetVisitSessionForTest();
    expect(snapshotLastVisit()).toBe("2026-06-13T09:00:00Z");
  });
});
