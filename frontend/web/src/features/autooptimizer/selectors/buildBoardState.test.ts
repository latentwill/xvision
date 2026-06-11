import { describe, expect, it } from "vitest";
import { boardFromNodes, buildBoardState } from "./buildBoardState";
import type { CycleNodeDetail } from "../api";

const ev = (type: string, extra: object = {}) => ({ type, ...extra });

describe("buildBoardState", () => {
  it("tracks experiments through propose → gate (real flattened wire shapes)", () => {
    const s = buildBoardState([
      ev("cycle_started", { cycle_id: "c1", parent_count: 1 }),
      ev("mutation_proposed", { child_hash: "aaa", mutator_model: "gemini-2.5-pro", parent_hash: "p" }),
      ev("mutation_proposed", { child_hash: "bbb", mutator_model: "gpt-5.2", parent_hash: "p" }),
      ev("mutation_gated", { child_hash: "aaa", passed: true, outcome: "kept", delta_day: 0.21 }),
    ]);
    expect(s.phase).toBe("gate");
    expect(s.cards).toHaveLength(2);
    expect(s.cards[0]).toMatchObject({ hash: "aaa", writer: "gemini-2.5-pro", state: "kept", delta: 0.21 });
    expect(s.cards[1]).toMatchObject({ hash: "bbb", state: "evaluating" });
  });

  it("maps the 3-way gate outcomes and finished cycles", () => {
    const s = buildBoardState([
      ev("cycle_started", { cycle_id: "c1" }),
      ev("mutation_proposed", { child_hash: "aaa" }),
      ev("mutation_proposed", { child_hash: "bbb" }),
      ev("mutation_gated", { child_hash: "aaa", passed: false, outcome: "dropped", delta_day: -0.1 }),
      ev("mutation_gated", { child_hash: "bbb", passed: false, outcome: "suspect" }),
      ev("cycle_finished", { active_count: 0, suspect_count: 1, rejected_count: 1 }),
    ]);
    expect(s.phase).toBe("done");
    expect(s.cards[0].state).toBe("rejected");
    expect(s.cards[1].state).toBe("suspect");
  });

  it("is empty for no events", () => {
    const s = buildBoardState([]);
    expect(s.phase).toBe("idle");
    expect(s.cards).toEqual([]);
  });
});

describe("boardFromNodes", () => {
  const node = (
    bundle_hash: string,
    status: string,
    extra: object = {},
  ): CycleNodeDetail =>
    ({
      bundle_hash,
      parent_hash: "p",
      status,
      cycle_id: "cyc-1",
      created_at: "2026-06-10T00:00:00Z",
      regime_results: [],
      ...extra,
    }) as CycleNodeDetail;

  it("maps node statuses to board card states (kept/rejected/suspect)", () => {
    const cards = boardFromNodes([
      node("aaa", "active"),
      node("bbb", "rejected"),
      node("ccc", "quarantined"),
    ]);
    expect(cards).toHaveLength(3);
    expect(cards[0]).toMatchObject({ hash: "aaa", state: "kept", writer: null });
    expect(cards[1]).toMatchObject({ hash: "bbb", state: "rejected", writer: null });
    expect(cards[2]).toMatchObject({ hash: "ccc", state: "suspect", writer: null });
  });

  it("is delta null-safe: uses delta_day when present, null otherwise", () => {
    const cards = boardFromNodes([
      node("aaa", "active", { delta_day: 0.21 }),
      node("bbb", "rejected"),
    ]);
    expect(cards[0].delta).toBe(0.21);
    expect(cards[1].delta).toBeNull();
  });

  it("returns empty for no nodes", () => {
    expect(boardFromNodes([])).toEqual([]);
  });
});
