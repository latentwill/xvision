import { describe, expect, it } from "vitest";
import { buildDigest, deriveBestFind, formatTokensCompact } from "./buildDigest";
import type { CycleRunSummary, CycleRunDetail, StatsRow } from "../api";

const NOW = new Date("2026-06-11T12:00:00Z").getTime();

function statsRow(over: Partial<StatsRow>): StatsRow {
  return {
    cycle_id: "c-1",
    session_id: "s-1",
    ts: "2026-06-10T12:00:00Z",
    kept: 1,
    suspect: 0,
    dropped: 2,
    best_delta_holdout: null,
    cost_usd: 1.5,
    cum_cost_usd: 1.5,
    ...over,
  };
}

function cycle(over: Partial<CycleRunSummary>): CycleRunSummary {
  return {
    cycle_id: "c-1",
    node_count: 3,
    active_count: 1,
    rejected_count: 2,
    first_created_at: "2026-06-10T11:00:00Z",
    last_created_at: "2026-06-10T12:00:00Z",
    ...over,
  };
}

describe("buildDigest", () => {
  it("returns null when there are no stats rows in the trailing 7 days", () => {
    expect(buildDigest([], [], NOW)).toBeNull();
    expect(
      buildDigest([statsRow({ ts: "2026-05-01T00:00:00Z" })], [], NOW),
    ).toBeNull();
  });

  it("sums experiments, kept and spend over the trailing 7 days only", () => {
    const rows = [
      statsRow({ cycle_id: "c-1", kept: 2, suspect: 1, dropped: 4, cost_usd: 3.25 }),
      statsRow({
        cycle_id: "c-2",
        ts: "2026-06-09T12:00:00Z",
        kept: 1,
        suspect: 0,
        dropped: 6,
        cost_usd: 2.0,
      }),
      // outside the window — ignored
      statsRow({ cycle_id: "c-old", ts: "2026-05-20T12:00:00Z", kept: 9, cost_usd: 99 }),
    ];
    const d = buildDigest(rows, [], NOW);
    expect(d).not.toBeNull();
    expect(d!.experiments).toBe(14); // (2+1+4) + (1+0+6)
    expect(d!.kept).toBe(3);
    expect(d!.spend).toBe("$5.25");
  });

  it("sums tokens from recent cycles' input+output and formats compactly", () => {
    const rows = [statsRow({})];
    const cycles = [
      cycle({ cycle_id: "c-1", input_tokens: 20_000_000, output_tokens: 1_800_000 }),
      cycle({
        cycle_id: "c-2",
        last_created_at: "2026-06-09T12:00:00Z",
        input_tokens: 9_000_000,
        output_tokens: 1_000_000,
      }),
      // outside the 7-day window — ignored
      cycle({
        cycle_id: "c-old",
        last_created_at: "2026-05-01T12:00:00Z",
        input_tokens: 500_000_000,
        output_tokens: 0,
      }),
    ];
    const d = buildDigest(rows, cycles, NOW);
    expect(d!.tokens).toBe("31.8M");
  });

  it("omits the tokens stat when every recent cycle's token fields are null", () => {
    const d = buildDigest(
      [statsRow({})],
      [cycle({ input_tokens: null, output_tokens: null })],
      NOW,
    );
    expect(d).not.toBeNull();
    expect(d!.tokens).toBeUndefined();
  });

  it("omits the tokens stat when there are no cycles at all", () => {
    const d = buildDigest([statsRow({})], [], NOW);
    expect(d!.tokens).toBeUndefined();
  });
});

describe("formatTokensCompact", () => {
  it("formats thousands / millions / billions", () => {
    expect(formatTokensCompact(820)).toBe("820");
    expect(formatTokensCompact(12_400)).toBe("12.4k");
    expect(formatTokensCompact(31_800_000)).toBe("31.8M");
    expect(formatTokensCompact(2_100_000_000)).toBe("2.1B");
  });
});

describe("deriveBestFind", () => {
  const detail = {
    cycle_id: "c-1",
    node_count: 2,
    active_count: 1,
    rejected_count: 1,
    first_created_at: "2026-06-10T11:00:00Z",
    last_created_at: "2026-06-10T12:00:00Z",
    nodes: [
      { bundle_hash: "deadbeef00", status: "rejected", created_at: "x", regime_results: [] },
      { bundle_hash: "abcd1234ef", status: "active", created_at: "x", regime_results: [] },
    ],
  } as unknown as CycleRunDetail;

  it("returns hash+delta when the last cycle has a stats delta and a kept node", () => {
    const stats = [statsRow({ cycle_id: "c-1", best_delta_holdout: 0.21 })];
    expect(deriveBestFind(stats, cycle({ cycle_id: "c-1" }), detail)).toEqual({
      hash: "abcd1234ef",
      delta: 0.21,
    });
  });

  it("is null when there is no kept node, no stats row, or thin data", () => {
    const stats = [statsRow({ cycle_id: "c-1", best_delta_holdout: 0.21 })];
    const noKept = {
      ...detail,
      nodes: [detail.nodes[0]],
    } as unknown as CycleRunDetail;
    expect(deriveBestFind(stats, cycle({ cycle_id: "c-1" }), noKept)).toBeNull();
    expect(deriveBestFind([], cycle({ cycle_id: "c-1" }), detail)).toBeNull();
    expect(
      deriveBestFind(
        [statsRow({ cycle_id: "c-1", best_delta_holdout: null })],
        cycle({ cycle_id: "c-1" }),
        detail,
      ),
    ).toBeNull();
    expect(deriveBestFind(stats, null, detail)).toBeNull();
    expect(deriveBestFind(stats, cycle({ cycle_id: "c-1" }), undefined)).toBeNull();
  });
});
