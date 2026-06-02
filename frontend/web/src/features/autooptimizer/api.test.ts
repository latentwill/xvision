import { describe, expect, it } from "vitest";

import {
  buildCycleSummaries,
  normalizeCycleProgressEnvelope,
  type LineageNode,
} from "./api";

const lineage: LineageNode[] = [
  {
    bundle_hash: "child-b",
    parent_hash: "root-a",
    gate_verdict: "rejected:overfit",
    status: "rejected",
    cycle_id: "cycle-2",
    created_at: "2026-06-02T02:00:00Z",
    diversity_score: 0.4,
  },
  {
    bundle_hash: "root-a",
    parent_hash: null,
    gate_verdict: "passed",
    status: "active",
    cycle_id: "cycle-2",
    created_at: "2026-06-02T01:00:00Z",
    diversity_score: 0.1,
  },
  {
    bundle_hash: "older",
    parent_hash: null,
    gate_verdict: "passed",
    status: "active",
    cycle_id: "cycle-1",
    created_at: "2026-06-01T01:00:00Z",
    diversity_score: 0.2,
  },
];

describe("buildCycleSummaries", () => {
  it("derives recent cycles from lineage nodes only", () => {
    expect(buildCycleSummaries(lineage)).toEqual([
      {
        cycleId: "cycle-2",
        active: 1,
        rejected: 1,
        experiments: 2,
        latestAt: "2026-06-02T02:00:00Z",
      },
      {
        cycleId: "cycle-1",
        active: 1,
        rejected: 0,
        experiments: 1,
        latestAt: "2026-06-01T01:00:00Z",
      },
    ]);
  });
});

describe("normalizeCycleProgressEnvelope", () => {
  it("unwraps backend SSE envelopes without reading flywheel history", () => {
    const event = normalizeCycleProgressEnvelope({
      kind: "mutation_proposed",
      display_label: "Experiment proposed",
      data: {
        cycle_id: "cycle-live",
        bundle_hash: "bundle-1",
        ts: "2026-06-02T03:00:00Z",
      },
    });

    expect(event).toEqual({
      event_type: "mutation_proposed",
      kind: "mutation_proposed",
      display_label: "Experiment proposed",
      cycle_id: "cycle-live",
      bundle_hash: "bundle-1",
      parent_hash: undefined,
      child_hash: undefined,
      ts: "2026-06-02T03:00:00Z",
      payload: {
        cycle_id: "cycle-live",
        bundle_hash: "bundle-1",
        ts: "2026-06-02T03:00:00Z",
      },
    });
  });
});
