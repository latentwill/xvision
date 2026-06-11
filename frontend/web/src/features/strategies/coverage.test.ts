// Tests for the strategy eval-coverage selector (frontend half of the
// eval-coverage undercount fix, beads xvision-eb5).
//
// The join must count an eval run toward a strategy when the run is keyed by
// ANY of the ids a run can carry:
//   - `run.strategy.id`     (enriched detail payloads)
//   - `run.agent_id`        (strategy workspace ULID — dashboard-launched runs)
//   - `run.agent_id`        (strategy bundle hash — older CLI-launched runs,
//                            matched against `strategy.bundle_hash`)
// and must trust the server-side `evaluated` flag when the run itself is
// outside the fetched runs page.

import { describe, expect, it } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import {
  coverageCounts,
  strategyEvalCoverage,
} from "./coverage";

const HASH_A =
  "a472499597277873fc0a9018084098fceecd4ffc903329aac889c7e2cf3a36bc";

function makeStrategy(
  id: string,
  overrides: Partial<StrategyListItem> = {},
): StrategyListItem {
  return {
    agent_id: id,
    display_name: `Strategy ${id}`,
    template: "custom",
    decision_cadence_minutes: 60,
    ...overrides,
  };
}

function makeRun(
  id: string,
  agentId: string,
  overrides: Partial<RunSummary> = {},
): RunSummary {
  return {
    id,
    agent_id: agentId,
    scenario_id: "scenario-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-01-01T00:00:00Z",
    completed_at: "2026-01-01T01:00:00Z",
    sharpe: 1.2,
    max_drawdown_pct: 5.0,
    total_return_pct: 8.0,
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
    ...overrides,
  };
}

describe("strategyEvalCoverage", () => {
  it("counts a strategy as evaluated when a completed run is keyed by its ULID", () => {
    const strategies = [makeStrategy("01AAA")];
    const runs = [makeRun("r1", "01AAA")];

    const [cov] = strategyEvalCoverage(strategies, runs);

    expect(cov.evaluated).toBe(true);
    expect(cov.completedRunCount).toBe(1);
    expect(cov.latestRun?.id).toBe("r1");
    expect(cov.origin).toBe("user");
  });

  it("counts hash-only runs via the strategy's bundle_hash", () => {
    // Older CLI-launched runs store only the bundle hash in run.agent_id;
    // the strategy list is keyed by ULID. Without the bundle_hash fallback
    // these strategies were miscounted as "no eval".
    const strategies = [makeStrategy("01BBB", { bundle_hash: HASH_A })];
    const runs = [makeRun("r1", HASH_A)];

    const [cov] = strategyEvalCoverage(strategies, runs);

    expect(cov.evaluated).toBe(true);
    expect(cov.latestRun?.id).toBe("r1");
  });

  it("uses run.strategy.id when the run payload is enriched", () => {
    const strategies = [makeStrategy("01CCC")];
    const runs = [
      makeRun("r1", "something-else", {
        strategy: { id: "01CCC", display_name: "X" },
      }),
    ];

    const [cov] = strategyEvalCoverage(strategies, runs);

    expect(cov.evaluated).toBe(true);
  });

  it("ignores non-completed runs", () => {
    const strategies = [makeStrategy("01DDD")];
    const runs = [
      makeRun("r1", "01DDD", { status: "running", completed_at: null }),
      makeRun("r2", "01DDD", { status: "failed", completed_at: null }),
    ];

    const [cov] = strategyEvalCoverage(strategies, runs);

    expect(cov.evaluated).toBe(false);
    expect(cov.completedRunCount).toBe(0);
    expect(cov.latestRun).toBeNull();
  });

  it("trusts the server-side evaluated flag when no run is in the fetched page", () => {
    // The home page fetches a single runs page; evals older than that page
    // (or CLI evals) are only visible through the server flag.
    const strategies = [
      makeStrategy("01EEE", {
        evaluated: true,
        last_eval_completed_at: "2026-05-01T00:00:00Z",
      }),
    ];

    const [cov] = strategyEvalCoverage(strategies, []);

    expect(cov.evaluated).toBe(true);
    expect(cov.latestRun).toBeNull();
    expect(cov.completedRunCount).toBe(0);
  });

  it("segments optimizer-origin strategies", () => {
    const strategies = [
      makeStrategy("01FFF", { origin: "optimizer" }),
      makeStrategy("01GGG"),
      makeStrategy("01HHH", { origin: "user" }),
    ];

    const cov = strategyEvalCoverage(strategies, []);

    expect(cov.map((c) => c.origin)).toEqual(["optimizer", "user", "user"]);
  });

  it("keeps the most recent completed run as latestRun", () => {
    const strategies = [makeStrategy("01JJJ")];
    const runs = [
      makeRun("old", "01JJJ", { completed_at: "2026-01-01T00:00:00Z" }),
      makeRun("new", "01JJJ", { completed_at: "2026-03-01T00:00:00Z" }),
      makeRun("mid", "01JJJ", { completed_at: "2026-02-01T00:00:00Z" }),
    ];

    const [cov] = strategyEvalCoverage(strategies, runs);

    expect(cov.latestRun?.id).toBe("new");
    expect(cov.completedRunCount).toBe(3);
  });
});

describe("coverageCounts", () => {
  it("splits un-evaluated strategies into user-awaiting and optimizer-lineage segments", () => {
    const strategies = [
      // user, evaluated
      makeStrategy("01AAA"),
      // user, never evaluated → awaiting first eval
      makeStrategy("01BBB"),
      makeStrategy("01CCC"),
      // optimizer-origin, never directly evaluated → evaluated in lineage
      makeStrategy("01DDD", { origin: "optimizer" }),
      // optimizer-origin WITH a direct eval → plain evaluated, not a segment
      makeStrategy("01EEE", { origin: "optimizer" }),
    ];
    const runs = [makeRun("r1", "01AAA"), makeRun("r2", "01EEE")];

    const counts = coverageCounts(strategyEvalCoverage(strategies, runs));

    expect(counts).toEqual({
      total: 5,
      evaluated: 2,
      userAwaitingFirstEval: 2,
      optimizerLineage: 1,
    });
  });

  it("mixed join keys: hash-only run + ULID run + server flag all count", () => {
    const strategies = [
      makeStrategy("01AAA", { bundle_hash: HASH_A }), // hash-only CLI run
      makeStrategy("01BBB"), // ULID run
      makeStrategy("01CCC", { evaluated: true }), // server flag only
      makeStrategy("01DDD"), // truly unevaluated
    ];
    const runs = [makeRun("r1", HASH_A), makeRun("r2", "01BBB")];

    const counts = coverageCounts(strategyEvalCoverage(strategies, runs));

    expect(counts.evaluated).toBe(3);
    expect(counts.userAwaitingFirstEval).toBe(1);
    expect(counts.optimizerLineage).toBe(0);
  });
});
