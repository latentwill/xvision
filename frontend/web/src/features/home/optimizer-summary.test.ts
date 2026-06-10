import { describe, expect, it } from "vitest";

import type { MutatorScore, StatsRow } from "@/features/autooptimizer/api";
import {
  cumulativeSpendUsd,
  cycleTrend,
  ladderTotals,
  lastCycle,
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
