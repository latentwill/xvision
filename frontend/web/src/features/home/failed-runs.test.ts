// frontend/web/src/features/home/failed-runs.test.ts
//
// Unit tests for the failed-run triage split (bead xvision-1zs):
//   - infra errors (provider/network/timeout) that are STALE (>2h old)
//     surface as calm NagStrip rows
//   - suspicious failures (not obviously infra, not a deliberate stop)
//     route into the Recent Findings surface
//   - deliberate stops (safety-pause abort, budget-exceeded clean stop)
//     are excluded from BOTH lists
//
// All selectors are pure: staleness uses an injected `nowMs` so the
// 2-hour boundary is deterministic.

import { describe, expect, it } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import {
  STALE_FAILED_HOURS,
  classifyFailedRun,
  failedRunFindings,
  failedRunNags,
} from "./failed-runs";

// A fixed "now" so staleness math is deterministic.
const NOW = Date.parse("2026-06-13T12:00:00Z");
const TWO_HOURS_MS = STALE_FAILED_HOURS * 60 * 60 * 1000;

function makeRun(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "agent-1",
    scenario_id: "scenario-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "failed",
    started_at: "2026-06-13T08:00:00Z",
    completed_at: "2026-06-13T08:30:00Z",
    sharpe: null,
    max_drawdown_pct: null,
    total_return_pct: null,
    error: "boom",
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

describe("classifyFailedRun", () => {
  it("classifies provider/network/timeout errors as infra", () => {
    const cases = [
      "Connection refused (os error 61)",
      "request timed out after 30s",
      "429 Too Many Requests — rate limit exceeded",
      "upstream provider returned 503 Service Unavailable",
      "dns error: failed to lookup address information",
      "TLS handshake failed",
      "socket hang up / ECONNRESET",
    ];
    for (const error of cases) {
      expect(classifyFailedRun(makeRun({ error })).kind).toBe("infra");
    }
  });

  it("classifies non-infra failures as suspicious", () => {
    const cases = [
      "strategy produced no decisions for any cycle",
      "[repeated_broker_error] aborted after 3 consecutive broker_min_order_size rejections",
      "panic: index out of bounds in scoring",
      "unexpected null in equity curve",
    ];
    for (const error of cases) {
      expect(classifyFailedRun(makeRun({ error })).kind).toBe("suspicious");
    }
  });

  it("excludes deliberate safety-pause aborts", () => {
    const run = makeRun({
      error: "aborted: safety_paused — operator paused trading",
    });
    expect(classifyFailedRun(run).kind).toBe("excluded");
  });

  it("excludes budget-exceeded clean stops", () => {
    const run = makeRun({
      error: "[budget_exceeded] budget_wall_ms_exceeded",
    });
    expect(classifyFailedRun(run).kind).toBe("excluded");
  });

  it("excludes other deliberate safety aborts (limit, venue mismatch)", () => {
    expect(
      classifyFailedRun(
        makeRun({ error: "aborted: safety_limit — daily_loss value=0.06 limit=0.05" }),
      ).kind,
    ).toBe("excluded");
    expect(
      classifyFailedRun(
        makeRun({
          error: "aborted: venue_label_mismatch — scenario=Paper broker=Live",
        }),
      ).kind,
    ).toBe("excluded");
  });

  it("is not-a-failure for non-failed runs", () => {
    expect(classifyFailedRun(makeRun({ status: "completed", error: null })).kind).toBe(
      "none",
    );
    expect(classifyFailedRun(makeRun({ status: "running", error: null })).kind).toBe(
      "none",
    );
  });

  it("treats a failed run with no error string as suspicious (unexplained)", () => {
    expect(classifyFailedRun(makeRun({ error: null })).kind).toBe("suspicious");
    expect(classifyFailedRun(makeRun({ error: "" })).kind).toBe("suspicious");
  });
});

describe("failedRunNags (stale infra only)", () => {
  it("emits a nag for an infra failure older than the staleness threshold", () => {
    const stale = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({ id: "r-stale", error: "Connection refused", completed_at: stale }),
    ];
    const nags = failedRunNags(runs, NOW);
    expect(nags).toHaveLength(1);
    expect(nags[0].link?.to).toBe("/eval-runs/r-stale");
    // routed "view run" affordance
    expect(nags[0].link?.label).toMatch(/view run/i);
    // calm tone — never danger for infra nags
    expect(nags[0].tone).toBe("warn");
  });

  it("does NOT emit a nag for an infra failure younger than the threshold", () => {
    const fresh = new Date(NOW - 60_000).toISOString(); // 1 minute ago
    const runs = [
      makeRun({ id: "r-fresh", error: "Connection refused", completed_at: fresh }),
    ];
    expect(failedRunNags(runs, NOW)).toHaveLength(0);
  });

  it("uses completed_at, then started_at, to judge staleness", () => {
    const staleStart = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({
        id: "r-noend",
        error: "request timed out",
        completed_at: null,
        started_at: staleStart,
      }),
    ];
    expect(failedRunNags(runs, NOW)).toHaveLength(1);
  });

  it("excludes suspicious and deliberate-stop runs from nags", () => {
    const stale = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({ id: "r-susp", error: "panic in scoring", completed_at: stale }),
      makeRun({
        id: "r-pause",
        error: "aborted: safety_paused — manual",
        completed_at: stale,
      }),
    ];
    expect(failedRunNags(runs, NOW)).toHaveLength(0);
  });

  it("includes the strategy name in the nag title when present", () => {
    const stale = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({
        id: "r-named",
        error: "503 Service Unavailable",
        completed_at: stale,
        strategy: { id: "s-1", display_name: "Momentum V2" },
      }),
    ];
    const nags = failedRunNags(runs, NOW);
    expect(nags[0].title).toMatch(/Momentum V2/);
  });
});

describe("failedRunFindings (suspicious only)", () => {
  it("returns suspicious failures as findings, regardless of age", () => {
    const fresh = new Date(NOW - 60_000).toISOString();
    const old = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({ id: "r-fresh", error: "panic in scoring", completed_at: fresh }),
      makeRun({ id: "r-old", error: "no decisions produced", completed_at: old }),
    ];
    const findings = failedRunFindings(runs, NOW);
    expect(findings.map((f) => f.runId)).toEqual(
      expect.arrayContaining(["r-fresh", "r-old"]),
    );
    expect(findings).toHaveLength(2);
  });

  it("excludes infra failures and deliberate stops from findings", () => {
    const old = new Date(NOW - TWO_HOURS_MS - 60_000).toISOString();
    const runs = [
      makeRun({ id: "r-infra", error: "Connection refused", completed_at: old }),
      makeRun({
        id: "r-pause",
        error: "aborted: safety_paused — manual",
        completed_at: old,
      }),
      makeRun({
        id: "r-budget",
        error: "[budget_exceeded] budget_wall_ms_exceeded",
        completed_at: old,
      }),
    ];
    expect(failedRunFindings(runs, NOW)).toHaveLength(0);
  });

  it("carries strategy name and a routed run id into the finding", () => {
    const runs = [
      makeRun({
        id: "r-x",
        error: "no decisions produced",
        strategy: { id: "s-9", display_name: "Mean Reversion" },
      }),
    ];
    const [finding] = failedRunFindings(runs, NOW);
    expect(finding.runId).toBe("r-x");
    expect(finding.strategyName).toBe("Mean Reversion");
    expect(finding.summary.length).toBeGreaterThan(0);
  });

  it("orders findings newest-first by completion stamp", () => {
    const older = new Date(NOW - 3 * 60 * 60 * 1000).toISOString();
    const newer = new Date(NOW - 60 * 60 * 1000).toISOString();
    const runs = [
      makeRun({ id: "r-older", error: "panic", completed_at: older }),
      makeRun({ id: "r-newer", error: "panic", completed_at: newer }),
    ];
    const findings = failedRunFindings(runs, NOW);
    expect(findings[0].runId).toBe("r-newer");
    expect(findings[1].runId).toBe("r-older");
  });
});
