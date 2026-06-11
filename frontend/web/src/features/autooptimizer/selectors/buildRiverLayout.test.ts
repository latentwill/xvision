import { describe, expect, it } from "vitest";
import { buildRiverLayout } from "./buildRiverLayout";

const node = (
  hash: string,
  parent: string | null,
  status: string,
  score: number | null,
  delta: number | null,
  at: string,
) => ({
  bundle_hash: hash,
  parent_hash: parent,
  cycle_id: `c-${hash}`,
  status,
  created_at: at,
  child_day_score: score,
  delta_day: delta,
});

describe("buildRiverLayout", () => {
  it("chains kept nodes into a lineage line and hangs rejected as stubs", () => {
    const layout = buildRiverLayout([
      node("root", null, "active", 1.0, null, "2026-06-01"),
      node("kept1", "root", "active", 1.2, 0.2, "2026-06-02"),
      node("rej1", "root", "rejected", 0.9, -0.1, "2026-06-02"),
      node("sus1", "kept1", "quarantined", 1.5, 0.3, "2026-06-03"),
      node("kept2", "kept1", "active", 1.4, 0.2, "2026-06-03"),
    ]);
    const lines = layout.lines;
    expect(lines).toHaveLength(1);
    expect(lines[0].points.map((p) => p.hash)).toEqual(["root", "kept1", "kept2"]);
    expect(lines[0].points.map((p) => p.y)).toEqual([1.0, 1.2, 1.4]);
    const stubKinds = layout.stubs.map((s) => [s.hash, s.kind]);
    expect(stubKinds).toContainEqual(["rej1", "rejected"]);
    expect(stubKinds).toContainEqual(["sus1", "suspect"]);
  });

  it("assigns stub ageRank by created_at for fade-with-age rendering", () => {
    const layout = buildRiverLayout([
      node("root", null, "active", 1.0, null, "2026-06-01"),
      node("oldRej", "root", "rejected", 0.9, -0.1, "2026-06-02"),
      node("kept1", "root", "active", 1.2, 0.2, "2026-06-03"),
      node("newRej", "kept1", "rejected", 1.1, -0.1, "2026-06-09"),
    ]);
    const old = layout.stubs.find((s) => s.hash === "oldRej")!;
    const recent = layout.stubs.find((s) => s.hash === "newRej")!;
    expect(old.ageRank).toBeLessThan(recent.ageRank);
    expect(recent.ageRank).toBe(1);
  });

  it("marks a line dead (retired) when its tip stopped producing while newer cycles exist", () => {
    const layout = buildRiverLayout([
      node("a", null, "active", 1.0, null, "2026-06-01"),
      node("a2", "a", "active", 1.3, 0.3, "2026-06-09"), // alive: tip in latest cycle window
      node("b", null, "active", 0.9, null, "2026-06-01"), // dead: no descendants since, newer activity exists
    ]);
    const lineA = layout.lines.find((l) => l.points.some((p) => p.hash === "a2"))!;
    const lineB = layout.lines.find((l) => l.points.at(-1)?.hash === "b")!;
    expect(lineA.alive).toBe(true);
    expect(lineB.alive).toBe(false);
  });

  it("marks the highest-scoring live line as champion", () => {
    const layout = buildRiverLayout([
      node("a", null, "active", 1.0, null, "2026-06-01"),
      node("a2", "a", "active", 1.6, 0.6, "2026-06-02"),
      node("b", null, "active", 1.1, null, "2026-06-01"),
    ]);
    expect(layout.lines.find((l) => l.champion)?.points.at(-1)?.hash).toBe("a2");
  });

  it("handles a single node with no score", () => {
    const layout = buildRiverLayout([node("only", null, "active", null, null, "2026-06-01")]);
    expect(layout.lines).toHaveLength(1);
    expect(layout.lines[0].points[0].y).toBe(1.0); // default baseline when score missing
    expect(layout.yDomain[0]).toBeLessThan(layout.yDomain[1]);
  });

  it("returns empty layout for no nodes", () => {
    expect(buildRiverLayout([]).lines).toEqual([]);
  });

  it("zero-kept case: roots with only rejected children yield 1-point lines and stubs only", () => {
    const layout = buildRiverLayout([
      node("root", null, "active", 1.0, null, "2026-06-01"),
      node("r1", "root", "rejected", 0.9, -0.1, "2026-06-02"),
    ]);
    expect(layout.lines[0].points).toHaveLength(1);
    expect(layout.stubs).toHaveLength(1);
  });

  it("forks off non-primary lines get their own sub-lines (recursive fork scan)", () => {
    // root → A (active, first by created_at: continues the primary line)
    // root → B (active, second: starts a sub-line)
    // B → B2 (active, first: continues B's sub-line)
    // B → B3 (active, second: must start its OWN sub-line — previously dropped
    //         because only the primary line's points were scanned for forks)
    const layout = buildRiverLayout([
      node("root", null, "active", 1.0, null, "2026-06-01"),
      node("A", "root", "active", 1.1, 0.1, "2026-06-02"),
      node("B", "root", "active", 1.2, 0.2, "2026-06-03"),
      node("B2", "B", "active", 1.3, 0.1, "2026-06-04"),
      node("B3", "B", "active", 1.4, 0.2, "2026-06-05"),
    ]);

    // Primary: root → A. Sub-line: B → B2. Sub-sub-line: B3.
    const lineB = layout.lines.find((l) => l.points[0]?.hash === "B");
    expect(lineB, "sub-line starting at B exists").toBeDefined();
    expect(lineB!.points.map((p) => p.hash)).toEqual(["B", "B2"]);

    const lineB3 = layout.lines.find((l) => l.points.some((p) => p.hash === "B3"));
    expect(lineB3, "B3 (extra active child of a non-primary line) gets a line").toBeDefined();
    expect(lineB3!.points[0]!.hash).toBe("B3");
    // B3 forks off B (x=1 on B's line) → starts at x=2
    expect(lineB3!.points[0]!.x).toBe(2);

    // Every active node appears in exactly one line
    const allHashes = layout.lines.flatMap((l) => l.points.map((p) => p.hash)).sort();
    expect(allHashes).toEqual(["A", "B", "B2", "B3", "root"]);
  });
});
