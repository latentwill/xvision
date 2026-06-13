import { describe, expect, it } from "vitest";

import type { MutatorScore, StatsRow } from "@/features/autooptimizer/api";
import {
  bestHoldoutDelta,
  costAnomaly,
  cumulativeSpendUsd,
  cycleTrend,
  ladderTotals,
  lastCycle,
  rollingAcceptanceRate,
  shortModelName,
  topWriters,
} from "./optimizer-summary";

function score(over: Partial<MutatorScore>): MutatorScore {
  return {
    provider: "openrouter",
    model: "google/gemini-3.1-flash-lite",
    prompt_version: "v1",
    proposals: 5,
    accepted: 2,
    rejected_overfit: 3,
    avg_delta_sharpe: 0.54,
    ...over,
  };
}

function stat(over: Partial<StatsRow>): StatsRow {
  return {
    cycle_id: "c1",
    session_id: "s1",
    ts: "2026-06-10T10:00:00Z",
    kept: 1,
    suspect: 0,
    dropped: 2,
    best_delta_holdout: null,
    cost_usd: 0.1,
    cum_cost_usd: 0.5,
    ...over,
  };
}

describe("ladderTotals", () => {
  it("sums proposals / accepted / rejected-overfit", () => {
    const totals = ladderTotals([
      score({ proposals: 5, accepted: 2, rejected_overfit: 3 }),
      score({ proposals: 7, accepted: 0, rejected_overfit: 7 }),
    ]);
    expect(totals).toEqual({ proposals: 12, accepted: 2, rejectedOverfit: 10 });
  });

  it("returns zeros for an empty ladder", () => {
    expect(ladderTotals([])).toEqual({
      proposals: 0,
      accepted: 0,
      rejectedOverfit: 0,
    });
  });
});

describe("topWriters", () => {
  it("ranks by avg ΔSharpe, then accepted, then proposals; drops zero-proposal rows", () => {
    const a = score({ model: "a", avg_delta_sharpe: 0.5 });
    const b = score({ model: "b", avg_delta_sharpe: 0.0, accepted: 1, proposals: 3 });
    const c = score({ model: "c", avg_delta_sharpe: 0.0, accepted: 0, proposals: 8 });
    const zero = score({ model: "zero", proposals: 0 });
    expect(topWriters([c, zero, b, a]).map((s) => s.model)).toEqual([
      "a",
      "b",
      "c",
    ]);
  });

  it("caps at n", () => {
    const rows = [1, 2, 3, 4].map((i) =>
      score({ model: `m${i}`, avg_delta_sharpe: i }),
    );
    expect(topWriters(rows, 2).map((s) => s.model)).toEqual(["m4", "m3"]);
  });
});

describe("shortModelName", () => {
  it("strips org/path prefixes", () => {
    expect(shortModelName("google/gemini-3.1-flash-lite")).toBe(
      "gemini-3.1-flash-lite",
    );
    expect(
      shortModelName("hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL"),
    ).toBe("Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL");
    expect(shortModelName("lfm2.5:8b")).toBe("lfm2.5:8b");
  });
});

describe("cycleTrend", () => {
  it("returns the last n cycles oldest→newest", () => {
    const rows = [3, 1, 2, 4].map((i) =>
      stat({ cycle_id: `c${i}`, ts: `2026-06-10T0${i}:00:00Z`, kept: i }),
    );
    const trend = cycleTrend(rows, 3);
    expect(trend.map((t) => t.cycleId)).toEqual(["c2", "c3", "c4"]);
    expect(trend[2]).toMatchObject({ kept: 4, suspect: 0, dropped: 2 });
  });
});

describe("cumulativeSpendUsd", () => {
  it("returns the newest finite cum_cost_usd, skipping null tails", () => {
    const rows = [
      stat({ ts: "2026-06-10T01:00:00Z", cum_cost_usd: 0.3 }),
      stat({ ts: "2026-06-10T02:00:00Z", cum_cost_usd: 0.62 }),
      stat({ ts: "2026-06-10T03:00:00Z", cum_cost_usd: null as unknown as number }),
    ];
    expect(cumulativeSpendUsd(rows)).toBe(0.62);
  });

  it("returns null when no row has a finite value", () => {
    expect(cumulativeSpendUsd([])).toBeNull();
    expect(
      cumulativeSpendUsd([stat({ cum_cost_usd: null as unknown as number })]),
    ).toBeNull();
  });
});

describe("lastCycle", () => {
  it("returns the newest row by ts", () => {
    const rows = [
      stat({ cycle_id: "old", ts: "2026-06-10T01:00:00Z" }),
      stat({ cycle_id: "new", ts: "2026-06-10T09:00:00Z" }),
    ];
    expect(lastCycle(rows)?.cycle_id).toBe("new");
  });

  it("returns null for empty stats", () => {
    expect(lastCycle([])).toBeNull();
  });
});

// ─── zn2: FE-derivable digest slices (acceptance rate / holdout Δ / cost) ─────

describe("rollingAcceptanceRate", () => {
  const NOW = new Date("2026-06-13T00:00:00Z");

  it("computes kept / (kept+suspect+dropped) over the 30d window", () => {
    const rows = [
      stat({ ts: "2026-06-10T00:00:00Z", kept: 3, suspect: 1, dropped: 0 }),
      stat({ ts: "2026-06-11T00:00:00Z", kept: 1, suspect: 0, dropped: 4 }),
    ];
    // kept 4 / total 9 = 0.444…
    const r = rollingAcceptanceRate(rows, { now: NOW });
    expect(r.rate).toBeCloseTo(4 / 9, 6);
    expect(r.kept).toBe(4);
    expect(r.total).toBe(9);
  });

  it("excludes rows older than the 30d window", () => {
    const rows = [
      // 40 days before NOW — out of window, must not count
      stat({ ts: "2026-05-04T00:00:00Z", kept: 100, suspect: 0, dropped: 0 }),
      stat({ ts: "2026-06-12T00:00:00Z", kept: 1, suspect: 0, dropped: 1 }),
    ];
    const r = rollingAcceptanceRate(rows, { now: NOW });
    expect(r.total).toBe(2);
    expect(r.kept).toBe(1);
    expect(r.rate).toBeCloseTo(0.5, 6);
  });

  it("returns null rate (0 denominator) when no in-window cycles produced candidates", () => {
    const rows = [stat({ ts: "2026-06-12T00:00:00Z", kept: 0, suspect: 0, dropped: 0 })];
    const r = rollingAcceptanceRate(rows, { now: NOW });
    expect(r.rate).toBeNull();
    expect(r.total).toBe(0);
    expect(r.degraded).toBe(false);
  });

  it("returns null rate for an empty window", () => {
    const r = rollingAcceptanceRate([], { now: NOW });
    expect(r.rate).toBeNull();
    expect(r.degraded).toBe(false);
  });

  it("flags degradation when the recent half's rate drops well below the older half", () => {
    // older half (4 cycles) mostly kept; recent half (4 cycles) mostly dropped.
    const rows = [
      stat({ ts: "2026-06-01T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      stat({ ts: "2026-06-02T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      stat({ ts: "2026-06-03T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      stat({ ts: "2026-06-04T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      stat({ ts: "2026-06-10T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      stat({ ts: "2026-06-11T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      stat({ ts: "2026-06-12T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      stat({ ts: "2026-06-13T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
    ];
    const r = rollingAcceptanceRate(rows, { now: NOW });
    expect(r.degraded).toBe(true);
  });

  it("does not flag degradation when the recent half holds up", () => {
    const rows = [
      stat({ ts: "2026-06-01T00:00:00Z", kept: 2, suspect: 0, dropped: 2 }),
      stat({ ts: "2026-06-02T00:00:00Z", kept: 2, suspect: 0, dropped: 2 }),
      stat({ ts: "2026-06-12T00:00:00Z", kept: 2, suspect: 0, dropped: 2 }),
      stat({ ts: "2026-06-13T00:00:00Z", kept: 2, suspect: 0, dropped: 2 }),
    ];
    const r = rollingAcceptanceRate(rows, { now: NOW });
    expect(r.degraded).toBe(false);
  });
});

describe("bestHoldoutDelta", () => {
  it("returns the max best_delta_holdout across rows", () => {
    const rows = [
      stat({ best_delta_holdout: 0.12 }),
      stat({ best_delta_holdout: 0.41 }),
      stat({ best_delta_holdout: -0.05 }),
    ];
    expect(bestHoldoutDelta(rows)).toBeCloseTo(0.41, 6);
  });

  it("skips null/undefined deltas (unpriced or pre-gate cycles)", () => {
    const rows = [
      stat({ best_delta_holdout: null }),
      stat({ best_delta_holdout: 0.2 }),
      stat({ best_delta_holdout: undefined as unknown as number }),
    ];
    expect(bestHoldoutDelta(rows)).toBeCloseTo(0.2, 6);
  });

  it("returns null when no row carries a finite delta", () => {
    expect(bestHoldoutDelta([])).toBeNull();
    expect(bestHoldoutDelta([stat({ best_delta_holdout: null })])).toBeNull();
  });

  it("can return a negative best when all deltas are negative", () => {
    const rows = [
      stat({ best_delta_holdout: -0.3 }),
      stat({ best_delta_holdout: -0.1 }),
    ];
    expect(bestHoldoutDelta(rows)).toBeCloseTo(-0.1, 6);
  });
});

describe("costAnomaly", () => {
  it("flags the current cycle as anomalous when its cost far exceeds the trailing median", () => {
    // trailing median of [0.10, 0.12, 0.11, 0.09] ≈ 0.105; current 0.80 ≫ 2×median.
    const rows = [
      stat({ ts: "2026-06-09T00:00:00Z", cost_usd: 0.1 }),
      stat({ ts: "2026-06-10T00:00:00Z", cost_usd: 0.12 }),
      stat({ ts: "2026-06-11T00:00:00Z", cost_usd: 0.11 }),
      stat({ ts: "2026-06-12T00:00:00Z", cost_usd: 0.09 }),
      stat({ ts: "2026-06-13T00:00:00Z", cost_usd: 0.8 }),
    ];
    const a = costAnomaly(rows);
    expect(a.anomalous).toBe(true);
    expect(a.currentUsd).toBeCloseTo(0.8, 6);
    expect(a.medianUsd).toBeCloseTo(0.105, 6);
  });

  it("does not flag when the current cost is in line with the trailing median", () => {
    const rows = [
      stat({ ts: "2026-06-11T00:00:00Z", cost_usd: 0.1 }),
      stat({ ts: "2026-06-12T00:00:00Z", cost_usd: 0.12 }),
      stat({ ts: "2026-06-13T00:00:00Z", cost_usd: 0.11 }),
    ];
    const a = costAnomaly(rows);
    expect(a.anomalous).toBe(false);
  });

  it("returns a non-anomalous result with null median when there is no trailing history", () => {
    const a = costAnomaly([stat({ cost_usd: 0.5 })]);
    expect(a.anomalous).toBe(false);
    expect(a.medianUsd).toBeNull();
    expect(a.currentUsd).toBeCloseTo(0.5, 6);
  });

  it("returns nulls for an empty stats list", () => {
    const a = costAnomaly([]);
    expect(a.anomalous).toBe(false);
    expect(a.currentUsd).toBeNull();
    expect(a.medianUsd).toBeNull();
  });

  it("never flags when the trailing median is zero (no division blow-up)", () => {
    const rows = [
      stat({ ts: "2026-06-11T00:00:00Z", cost_usd: 0 }),
      stat({ ts: "2026-06-12T00:00:00Z", cost_usd: 0 }),
      stat({ ts: "2026-06-13T00:00:00Z", cost_usd: 0.5 }),
    ];
    const a = costAnomaly(rows);
    expect(a.anomalous).toBe(false);
    expect(a.medianUsd).toBe(0);
  });
});
